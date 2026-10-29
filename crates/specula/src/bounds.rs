//! Bounds on what agents send and what the engine answers (docs/design/TOOLS.md, "Bounds").
//!
//! Recurring bug class D (2026-10-29): parsers looped over whatever arrived (`world_query
//! {fields: [...] * 1M}` took 3.8 s and answered 21 MB), a "did you mean" ran an edit distance on
//! a 4 MB key (4.4 s), and errors echoed any input verbatim (a 4 MB answer for a 4 MB input). The
//! root causes were that argument helpers had no default bounds, nothing capped answers, and
//! errors echoed input. This module is the one place each of those is fixed, for every tool:
//!
//! - **Arguments** are checked against the tool's own schema before any parser sees them
//!   ([`check_args`], called by `toolscope_tools::call` for every call, `tools_invoke` and
//!   playtest steps included). Every string and array a schema describes carries `maxLength` /
//!   `maxItems` ([`add_to_schemas`] writes the defaults, [`DEFAULT_STRING`] and [`DEFAULT_ITEMS`],
//!   or a tool's own override from [`OVERRIDES`]); arrays that mean sets carry `uniqueItems` and
//!   repeats are refused. Object keys longer than [`MAX_KEY_CHARS`] are refused before any
//!   "did you mean". Free-form data (an object schema with no `properties`: props, params, a
//!   playtest step's arguments) is bounded more loosely ([`DATA_STRING`], [`DATA_ITEMS`]): its
//!   consumer validates it, and the request body is at most 16 MiB.
//! - **Echoes** of input in errors are cut ([`echo`]: 128 characters and the size), and every
//!   error's message is at most [`MAX_ERROR_MESSAGE`] bytes ([`tidy_error`]).
//! - **Answers** have a ceiling per tool ([`answer_ceiling`], 256 KiB of text by default; images
//!   apart): past it the call answers `answer_too_large`, saying that the call itself completed.

use specula_core::Json;
use specula_protocol::{Content, ToolDef, ToolOutput};

/// An echoed value in an error: at most this many characters, then its size.
pub const MAX_ECHO_CHARS: usize = 128;
/// An error's `message`: at most this many bytes.
pub const MAX_ERROR_MESSAGE: usize = 2048;
/// Any other string in an error answer: at most this many bytes (a rendered compiler diagnostic
/// fits; a conflict's values are at most 2 KiB already: `conflict::MAX_VALUE_BYTES`).
const MAX_ERROR_FIELD: usize = 8192;
/// Nesting kept in an error answer (deeper values are summarised).
const MAX_ERROR_DEPTH: usize = 16;
/// An answer's text (images apart), unless the tool has its own ceiling ([`CEILINGS`]).
pub const DEFAULT_ANSWER: usize = 256 * 1024;
/// A string a schema describes, unless it says more.
pub const DEFAULT_STRING: usize = 4096;
/// An array a schema describes, unless it says more.
pub const DEFAULT_ITEMS: usize = 256;
/// An argument name (an object key a schema describes): no tool has a longer one.
pub const MAX_KEY_CHARS: usize = 64;
/// Keys of one described object (its properties are fewer).
const MAX_KEYS: usize = 256;
/// Free-form data (an object schema without `properties`, or a property without a type): strings,
/// arrays, keys, entries. Its consumer validates it (props at most 16 KiB, a Lex document at most
/// 10,000 items, a tile edit 65,536 cells), and the request body is at most 16 MiB: these only stop
/// what no consumer takes.
pub const DATA_STRING: usize = 1 << 20;
pub const DATA_ITEMS: usize = 65_536;
const DATA_KEY_CHARS: usize = 256;
const DATA_KEYS: usize = 16_384;

/// `s` as echoed in an error: itself when short, else its first [`MAX_ECHO_CHARS`] characters
/// and its size in bytes.
pub fn echo(s: &str) -> String {
    echo_at(s, MAX_ECHO_CHARS)
}

fn echo_at(s: &str, chars: usize) -> String {
    match s.char_indices().nth(chars) {
        None => s.to_owned(),
        Some((cut, _)) => format!("{}... ({} bytes)", &s[..cut], s.len()),
    }
}

/// `s` cut to at most `bytes` (on a character boundary), noting its size.
fn cut_bytes(s: &str, bytes: usize) -> String {
    if s.len() <= bytes {
        return s.to_owned();
    }
    let mut end = bytes.saturating_sub(24);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}... ({} bytes)", &s[..end], s.len())
}

/// Quoted runs without spaces longer than [`MAX_ECHO_CHARS`] (an echoed key, name or value:
/// prose has spaces) cut to an [`echo`]. Quotes are `'` and `"`.
fn cut_quoted(message: &str) -> String {
    let mut out = String::with_capacity(message.len().min(MAX_ERROR_MESSAGE + 64));
    let mut rest = message;
    while let Some(open) = rest.find(['\'', '"']) {
        let q = rest.as_bytes()[open] as char;
        out.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
        match rest.find(q) {
            Some(close) if close > MAX_ECHO_CHARS && !rest[..close].contains(char::is_whitespace) => {
                out.push_str(&echo(&rest[..close]));
                out.push(q);
                rest = &rest[close + 1..];
            }
            _ => {}
        }
    }
    out.push_str(rest);
    out
}

/// A bounded copy of an error answer's value: long strings and keys cut, deep nesting summarised.
fn bound_error_value(v: &Json, depth: usize) -> Json {
    match v {
        Json::Str(s) if s.len() > MAX_ERROR_FIELD => Json::from(cut_bytes(s, MAX_ERROR_FIELD)),
        Json::Arr(a) if depth > MAX_ERROR_DEPTH => Json::from(format!("({} items)", a.len())),
        Json::Arr(a) => Json::Arr(a.iter().map(|x| bound_error_value(x, depth + 1)).collect()),
        Json::Obj(o) if depth > MAX_ERROR_DEPTH => Json::from(format!("({} fields)", o.len())),
        Json::Obj(o) => Json::Obj(o.iter().map(|(k, x)| (echo(k), bound_error_value(x, depth + 1))).collect()),
        other => other.clone(),
    }
}

/// The marker [`shrink`] leaves at the end of an array it cut.
fn omitted_marker(n: usize) -> Json {
    Json::obj([("omitted", Json::from(n))])
}

/// How many items an array's marker says were cut already (0: no marker).
fn omitted_in(a: &[Json]) -> usize {
    a.last().and_then(|m| m.as_object()).filter(|o| o.len() == 1).and_then(|o| (o[0].0 == "omitted").then(|| o[0].1.as_u64())).flatten().map_or(0, |n| n as usize)
}

/// The path (indices into objects and arrays) of the array whose text is largest, among arrays
/// with at least two items besides a marker.
fn largest_array(v: &Json, path: &mut Vec<usize>, best: &mut Option<(usize, Vec<usize>)>) {
    match v {
        Json::Arr(a) => {
            let real = a.len() - usize::from(omitted_in(a) > 0);
            if real >= 2 {
                let size = v.to_string().len();
                if best.as_ref().is_none_or(|(s, _)| size > *s) {
                    *best = Some((size, path.clone()));
                }
            }
            for (i, x) in a.iter().enumerate() {
                path.push(i);
                largest_array(x, path, best);
                path.pop();
            }
        }
        Json::Obj(o) => {
            for (i, (_, x)) in o.iter().enumerate() {
                path.push(i);
                largest_array(x, path, best);
                path.pop();
            }
        }
        _ => {}
    }
}

fn at_path<'a>(v: &'a mut Json, path: &[usize]) -> Option<&'a mut Json> {
    let Some((&first, rest)) = path.split_first() else { return Some(v) };
    match v {
        Json::Arr(a) => at_path(a.get_mut(first)?, rest),
        Json::Obj(o) => at_path(&mut o.get_mut(first)?.1, rest),
        _ => None,
    }
}

/// Shrinks `j` to at most `limit` bytes of text by halving its largest arrays, each ending in
/// `{"omitted": n}` (what was cut is counted, never silently gone); the fields stay, so the
/// message never points at something removed. `false` when it still doesn't fit.
fn shrink(j: &mut Json, limit: usize) -> bool {
    for _ in 0..64 {
        if j.to_string().len() <= limit {
            return true;
        }
        let mut best = None;
        largest_array(j, &mut Vec::new(), &mut best);
        let Some((_, path)) = best else { return false };
        let Some(Json::Arr(a)) = at_path(j, &path) else { return false };
        let before = omitted_in(a);
        if before > 0 {
            a.pop();
        }
        let keep = a.len() / 2;
        let cut = a.len() - keep;
        a.truncate(keep);
        a.push(omitted_marker(before + cut));
    }
    j.to_string().len() <= limit
}

/// Bounds an error answer's text (already `{error, message, ...}` JSON from `response::tidy`)
/// to `limit` bytes: the message's long echoes cut and the message at most [`MAX_ERROR_MESSAGE`]
/// bytes, other strings at most [`MAX_ERROR_FIELD`], then the largest arrays halved (with an
/// `omitted` count) until it fits; only if that can't make it fit, `error` and `message` alone
/// (review of 998af3d: forty compiler errors in `logic_build`'s diagnostics were dropped whole).
pub fn tidy_error(text: &str, limit: usize) -> String {
    let Ok(Json::Obj(pairs)) = Json::parse(text) else {
        return cut_bytes(&cut_quoted(text), MAX_ERROR_MESSAGE);
    };
    let mut out: Vec<(String, Json)> = Vec::with_capacity(pairs.len());
    for (k, v) in &pairs {
        let v = match (k.as_str(), v) {
            ("message", Json::Str(m)) => Json::from(cut_bytes(&cut_quoted(m), MAX_ERROR_MESSAGE)),
            _ => bound_error_value(v, 0),
        };
        out.push((echo(k), v));
    }
    let mut j = Json::Obj(out);
    let size = j.to_string().len();
    if size <= limit || shrink(&mut j, limit) {
        return j.to_string();
    }
    let keep: Vec<(String, Json)> = j.as_object().unwrap_or_default().iter().filter(|(k, _)| k == "error" || k == "message").cloned().collect();
    let mut j = Json::Obj(keep);
    j.set("details_omitted", Json::from(format!("the error's details were {size} bytes and couldn't be cut to fit {limit}: only its code and message are kept")));
    j.to_string()
}

/// Tools whose answers may be larger than [`DEFAULT_ANSWER`], and why (bytes of text).
///
/// - `project_read`: a project file is at most 1 MiB (`project::MAX_FILE_BYTES`), answered whole
///   with its metadata.
/// - `tools_activate`, `tools_catalog`: activating every family answers every schema (about
///   210 KiB with a game's tools, and growing with the engine).
/// - `lex_get`, `lex_export`: a rule set's document; past the inline cap it is written to a file.
const CEILINGS: [(&str, usize); 5] = [
    ("project_read", 1200 * 1024),
    ("tools_activate", 1024 * 1024),
    ("tools_catalog", 512 * 1024),
    ("lex_get", 512 * 1024),
    ("lex_export", 512 * 1024),
];

/// The most text `tool` may answer.
pub fn answer_ceiling(tool: &str) -> usize {
    CEILINGS.iter().find(|(t, _)| *t == tool).map_or(DEFAULT_ANSWER, |(_, n)| *n)
}

/// The text bytes of an answer (images apart: a capture's PNG is bounded by its size cap).
fn text_bytes(out: &ToolOutput) -> usize {
    out.content.iter().map(|c| if let Content::Text(t) = c { t.len() } else { 0 }).sum()
}

/// The one choke point every answer leaves through (after `response::tidy`): errors bounded
/// ([`tidy_error`]), and answers past their tool's ceiling replaced by `answer_too_large`.
pub fn cap_answer(tool: &str, mut out: ToolOutput) -> ToolOutput {
    if out.is_error {
        for c in &mut out.content {
            if let Content::Text(t) = c {
                if t.len() > MAX_ERROR_MESSAGE / 2 || t.contains(['\'', '"']) {
                    *t = tidy_error(t, answer_ceiling(tool));
                }
            }
        }
        return out;
    }
    let bytes = text_bytes(&out);
    let limit = answer_ceiling(tool);
    if bytes <= limit {
        return out;
    }
    let shown = echo(tool);
    ToolOutput::error(
        Json::obj([
            ("error", Json::from("answer_too_large")),
            (
                "message",
                Json::from(format!(
                    "{shown} completed (any change it made stands), but its answer was {bytes} bytes, past its {limit}-byte ceiling, so it was withheld: \
                     ask for less (a smaller limit or page, fewer fields, a narrower filter) or use the tool's paging cursor"
                )),
            ),
            ("completed", Json::Bool(true)),
            ("tool", Json::from(shown)),
            ("bytes", Json::from(bytes)),
            ("limit", Json::from(limit)),
        ])
        .to_string(),
    )
}

// ── argument bounds from the schema ─────────────────────────────────────────

/// A tool's own limit on one of its schema's strings or arrays, over the defaults.
#[derive(Clone, Copy, Debug)]
pub struct Override {
    pub tool: &'static str,
    /// The property's path in the schema: names joined with `.`, `[]` for an array's items
    /// (`ops`, `rows[].clip`).
    pub path: &'static str,
    /// `maxLength` for a string, `maxItems` for an array.
    pub max: usize,
    /// The array means a set: repeats are refused (`uniqueItems`).
    pub set: bool,
    pub why: &'static str,
}

const fn o(tool: &'static str, path: &'static str, max: usize, set: bool, why: &'static str) -> Override {
    Override { tool, path, max, set, why }
}

/// Every tool's own limit that differs from the defaults (or marks a set), with the reason.
/// A test checks each names a real schema property.
pub const OVERRIDES: &[Override] = &[
    // Bigger than the defaults: what the tool really takes.
    o("world_batch", "ops", 1000, false, "MAX_BATCH_OPS: a batch applies up to 1000 operations atomically"),
    o("tilemap_set", "cells", 65_536, false, "two_d::MAX_TILE_EDITS cells per call"),
    o("project_write", "text", 1 << 20, false, "a project file is at most 1 MiB (project::MAX_FILE_BYTES)"),
    o("project_upload", "data", 5_592_420, false, "a chunk carries at most 4 MiB (blobs::MAX_CHUNK_BYTES), base64 (4/3 larger)"),
    o("task_plan", "text", 1 << 20, false, "a plan document is at most 1 MiB (director::MAX_DOC_BYTES)"),
    o("lex_simulate", "commands", 5000, false, "a simulation takes at most 5000 commands (lex_tools::MAX_SIM_COMMANDS)"),
    o("lex_edit", "ops", 500, false, "at most 500 ops per lex_edit (lex_tools::MAX_OPS)"),
    // Sets: repeats are refused (each was a list a loop ran over once per item).
    o("world_query", "fields", 32, true, "fields of an entity (about 20 exist): a repeated field is a mistake"),
    o("world_query", "tags", 32, true, "tags an entity must carry (at most 32 per entity)"),
    o("world_list", "tags", 32, true, "tags an entity must carry (at most 32 per entity)"),
    o("world_set", "add_tags", 32, true, "tags (at most 32 per entity)"),
    o("world_set", "remove_tags", 32, true, "tags (at most 32 per entity)"),
    o("world_history", "kinds", 32, true, "journal kinds"),
    o("world_undo", "ids", 256, true, "journal entries to undo: each once"),
    o("anim_check", "clips", 256, true, "clips to check: each once (256 copies of one clip took 1.2 s)"),
    o("sight_frame", "overlays", 32, true, "overlay names"),
    o("sight_frame", "overlay_entities", 256, true, "subtrees to label"),
    o("sight_orbit", "overlays", 32, true, "overlay names"),
    o("sight_orbit", "overlay_entities", 256, true, "subtrees to label"),
    o("sight_compare", "overlays", 32, true, "overlay names"),
    o("sight_compare", "overlay_entities", 256, true, "subtrees to label"),
    o("lex_get", "ids", 50, true, "item ids (1..50)"),
    o("review_decide", "ids", 64, true, "submissions, at most 64"),
    o("critique_update", "ids", 256, true, "critiques"),
    o("studio_sheet", "passes", 8, true, "passes: colour, value, silhouette"),
    o("studio_compare", "passes", 8, true, "passes: colour, value, silhouette"),
    o("tools_activate", "tools", 256, true, "tool and family names"),
    o("tools_deactivate", "tools", 256, true, "tool and family names"),
    o("tools_scope", "tools", 256, true, "tool and family names"),
];

/// A refused argument: what an agent changes, as data.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundsError {
    pub code: &'static str,
    pub message: String,
    pub field: String,
    pub limit: usize,
    pub got: usize,
}

impl BoundsError {
    pub fn to_output(&self) -> ToolOutput {
        ToolOutput::error(
            Json::obj([
                ("error", Json::from(self.code)),
                ("message", Json::from(self.message.as_str())),
                ("field", Json::from(echo(&self.field))),
                ("limit", Json::from(self.limit)),
                ("got", Json::from(self.got)),
            ])
            .to_string(),
        )
    }
}

fn types_of(schema: &Json) -> (bool, bool, bool) {
    let has = |t: &str| match schema.get("type") {
        Some(Json::Str(s)) => s == t,
        Some(Json::Arr(a)) => a.iter().any(|x| x.as_str() == Some(t)),
        _ => false,
    };
    (has("string"), has("array"), has("object"))
}

/// Whether `schema` describes free-form data: an object without `properties`, a property
/// without a type, or no schema.
fn free_form(schema: Option<&Json>) -> bool {
    match schema {
        None => true,
        Some(Json::Obj(_)) => {
            let s = schema.expect("some");
            s.get("properties").is_none() && (types_of(s).2 || s.get("type").is_none())
        }
        Some(_) => false,
    }
}

/// Writes the limits into every string and array a schema describes (the defaults, or the
/// tool's [`OVERRIDES`]), so what a client reads is what [`check_args`] enforces.
pub fn add_to_schemas(defs: &mut [ToolDef]) {
    for d in defs.iter_mut() {
        let name = d.name.clone();
        annotate(&mut d.input_schema, &name, "", 0);
    }
}

fn join(path: &str, name: &str) -> String {
    if path.is_empty() { name.to_owned() } else { format!("{path}.{name}") }
}

fn annotate(schema: &mut Json, tool: &str, path: &str, depth: usize) {
    if depth > 16 || !matches!(schema, Json::Obj(_)) {
        return;
    }
    let (string, array, _) = types_of(schema);
    let over = OVERRIDES.iter().find(|o| o.tool == tool && o.path == path && !path.is_empty());
    // A property without a type with an override (a list of cells): declared as an array's.
    let array = array || (over.is_some() && schema.get("type").is_none());
    if string && schema.get("maxLength").is_none() {
        schema.set("maxLength", Json::from(over.map_or(DEFAULT_STRING, |o| o.max)));
    }
    if array {
        if schema.get("maxItems").is_none() {
            schema.set("maxItems", Json::from(over.map_or(DEFAULT_ITEMS, |o| o.max)));
        }
        if over.is_some_and(|o| o.set) {
            schema.set("uniqueItems", Json::Bool(true));
        }
    }
    if let Some(Json::Obj(props)) = schema.get("properties").cloned() {
        let mut props = props;
        for (k, v) in &mut props {
            annotate(v, tool, &join(path, k), depth + 1);
        }
        schema.set("properties", Json::Obj(props));
    }
    if let Some(mut items) = schema.get("items").cloned() {
        match &mut items {
            Json::Arr(list) => {
                for it in list.iter_mut() {
                    annotate(it, tool, &format!("{path}[]"), depth + 1);
                }
            }
            it => annotate(it, tool, &format!("{path}[]"), depth + 1),
        }
        schema.set("items", items);
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(Json::Arr(mut alts)) = schema.get(key).cloned() {
            for a in &mut alts {
                annotate(a, tool, path, depth + 1);
            }
            schema.set(key, Json::Arr(alts));
        }
    }
}

/// Checks `args` against `def`'s schema (as [`add_to_schemas`] left it) before any parser sees
/// them: strings, arrays and keys within their limits, sets without repeats. Cheap: lengths are
/// compared before anything is looked at, so a 1 MB string or a million items costs nothing.
pub fn check_args(def: &ToolDef, args: &Json) -> Result<(), BoundsError> {
    let mut c = Checker { tool: &def.name };
    c.value(Some(&def.input_schema), args, "", 0)
}

struct Checker<'a> {
    tool: &'a str,
}

/// The name a refusal gives a place in the arguments.
fn shown(path: &str) -> String {
    if path.is_empty() { "the arguments".to_owned() } else { format!("'{}'", echo(path)) }
}

impl Checker<'_> {
    fn too_large(&self, code: &'static str, path: &str, what: &str, limit: usize, got: usize) -> BoundsError {
        let why = OVERRIDES.iter().find(|o| o.tool == self.tool && o.path == path).map_or(String::new(), |o| format!(" ({})", o.why));
        BoundsError {
            code,
            message: format!("{} has {got} {what}; {} takes at most {limit} there{why} (the schema says so): send less, or page", shown(path), echo(self.tool)),
            field: path.to_owned(),
            limit,
            got,
        }
    }

    fn value(&mut self, schema: Option<&Json>, v: &Json, path: &str, depth: usize) -> Result<(), BoundsError> {
        // The parser stops past depth 128 already; no tool's arguments nest past 64.
        if depth > 64 {
            return Err(self.too_large("argument_too_deep", path, "levels of nesting", 64, depth));
        }
        let free = free_form(schema);
        let limit = |key: &str, default: usize| schema.and_then(|s| s.get(key)).and_then(Json::as_u64).map_or(default, |n| n as usize);
        match v {
            Json::Str(s) => {
                let max = if free { limit("maxLength", DATA_STRING) } else { limit("maxLength", DEFAULT_STRING) };
                // `maxLength` counts characters (JSON Schema's rule, and what descriptions say:
                // "up to 2000 characters"); bytes bound them from both sides, so most strings are
                // decided without counting (review of 998af3d: 1,500 CJK characters were refused).
                if s.len() > max && (s.len() > max.saturating_mul(4) || s.chars().count() > max) {
                    let (got, what) = if s.len() > max.saturating_mul(4) { (s.len() / 4, "characters at least") } else { (s.chars().count(), "characters") };
                    return Err(self.too_large("argument_too_large", path, what, max, got));
                }
            }
            Json::Arr(items) => {
                let max = if free { limit("maxItems", DATA_ITEMS) } else { limit("maxItems", DEFAULT_ITEMS) };
                if items.len() > max {
                    return Err(self.too_large("argument_too_large", path, "items", max, items.len()));
                }
                if schema.and_then(|s| s.get("uniqueItems")).and_then(Json::as_bool) == Some(true) {
                    let mut seen = std::collections::BTreeSet::new();
                    for (i, it) in items.iter().enumerate() {
                        let k = it.to_string();
                        if !seen.insert(k.clone()) {
                            return Err(BoundsError {
                                code: "duplicate_items",
                                message: format!("{} lists {} twice (items {i} and an earlier one): each may appear once", shown(path), echo(&k)),
                                field: path.to_owned(),
                                limit: max,
                                got: items.len(),
                            });
                        }
                    }
                }
                let item_schema = match schema.and_then(|s| s.get("items")) {
                    Some(Json::Arr(list)) => list.first(),
                    other => other,
                };
                // Items of a free-form array are free-form data too.
                let item_schema = if free && item_schema.is_none() { None } else { item_schema };
                for it in items {
                    self.value(item_schema.or(if free { None } else { Some(&EMPTY) }), it, &format!("{path}[]"), depth + 1)?;
                }
            }
            Json::Obj(pairs) => {
                let props = schema.and_then(|s| s.get("properties"));
                let (key_max, keys_max) = if props.is_some() { (MAX_KEY_CHARS, MAX_KEYS) } else { (DATA_KEY_CHARS, DATA_KEYS) };
                if pairs.len() > keys_max {
                    return Err(self.too_large("argument_too_large", path, "keys", keys_max, pairs.len()));
                }
                let extra = schema.and_then(|s| s.get("additionalProperties")).filter(|a| matches!(a, Json::Obj(_)));
                for (k, x) in pairs {
                    if k.chars().count() > key_max {
                        return Err(BoundsError {
                            code: "argument_key_too_long",
                            message: format!("{} has a key of {} characters ('{}'); {} names are at most {key_max} characters", shown(path), k.chars().count(), echo(k), if props.is_some() { "argument" } else { "data" }),
                            field: join(path, &echo(k)),
                            limit: key_max,
                            got: k.chars().count(),
                        });
                    }
                    // tools_invoke's arguments are the invoked tool's: checked against its schema.
                    if self.tool == "tools_invoke" && path.is_empty() && k == "arguments" {
                        continue;
                    }
                    let child = props.and_then(|p| p.get(k)).or(extra);
                    // An undescribed key of a described object: the parser refuses it by name;
                    // its value is bounded as data until then.
                    self.value(child, x, &join(path, k), depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// A schema that describes nothing beyond "a value" (items of a described array without an
/// `items` schema): the strict defaults apply.
static EMPTY: Json = Json::Null;

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
    fn echoes_are_cut_with_their_size() {
        assert_eq!(echo("short"), "short");
        let long = "k".repeat(4 << 20);
        let e = echo(&long);
        assert!(e.len() < 200 && e.ends_with("(4194304 bytes)"), "{e}");
        // Multi-byte characters are cut on a boundary.
        let wide = "é".repeat(300);
        assert!(echo(&wide).starts_with(&"é".repeat(128)));
    }

    #[test]
    fn errors_are_bounded_whoever_made_them() {
        let key = "k".repeat(4 << 20);
        let plain = format!("'{key}' is not an argument of world_spawn; did you mean 'name'?");
        let e = tidy_error(&plain, DEFAULT_ANSWER);
        assert!(e.len() <= MAX_ERROR_MESSAGE, "{}", e.len());
        let j = Json::obj([("error", Json::from("invalid_argument")), ("message", Json::from(plain.as_str())), ("field", Json::from(key.as_str()))]);
        let e = tidy_error(&j.to_string(), DEFAULT_ANSWER);
        assert!(e.len() < MAX_ERROR_MESSAGE + MAX_ERROR_FIELD + 512, "{}", e.len());
        let back = Json::parse(&e).unwrap();
        assert!(back.get("message").and_then(Json::as_str).unwrap().contains("did you mean 'name'"), "prose survives: {e}");
        // Prose with apostrophes is left alone.
        let prose = "the agent's claim covers it; it can't be written ('claimed')";
        assert_eq!(tidy_error(prose, DEFAULT_ANSWER), prose);
        // Huge details shrink by their largest arrays, counted, never dropped whole.
        let big = Json::obj([("error", Json::from("x")), ("message", Json::from("m")), ("list", Json::arr((0..10_000).map(|i| Json::Arr(vec![Json::from(i); 40]))))]);
        let e = tidy_error(&big.to_string(), 32 * 1024);
        assert!(e.len() <= 32 * 1024, "{}", e.len());
        let back = Json::parse(&e).unwrap();
        let list = back.get("list").and_then(Json::as_array).unwrap();
        assert!(list.len() > 10 && omitted_in(list) + list.len() - 1 == 10_000, "{} kept", list.len());
    }

    /// Review of 998af3d (major 4): a 48 KB compiler report kept nothing of its diagnostics. Its
    /// diagnostics stay (a cut copy and the count of what was cut), and nothing it names is gone.
    #[test]
    fn big_error_reports_keep_their_diagnostics() {
        let diag = |i: usize| Json::obj([("code", Json::from("E0308")), ("message", Json::from("mismatched types")), ("rendered", Json::from(format!("error[E0308]: mismatched types {i}\n{}", "  | let x: u32 = \"a\";\n".repeat(60))))]);
        let report = Json::obj([
            ("error", Json::from("compile_errors")),
            ("message", Json::from("the module didn't compile: 40 errors (diagnostics lists them)")),
            ("diagnostics", Json::arr((0..40).map(diag))),
        ]);
        let text = report.to_string();
        assert!(text.len() > 48 * 1024, "{}", text.len());
        let kept = Json::parse(&tidy_error(&text, DEFAULT_ANSWER)).unwrap();
        assert_eq!(kept.get("diagnostics").and_then(Json::as_array).map(<[Json]>::len), Some(40), "under the ceiling it is whole");
        let cut = Json::parse(&tidy_error(&text, 48 * 1024)).unwrap();
        let d = cut.get("diagnostics").and_then(Json::as_array).unwrap();
        assert!(d.len() > 2 && d.len() - 1 + omitted_in(d) == 40, "{} kept", d.len());
        assert!(cut.get("details_omitted").is_none());
    }

    #[test]
    fn answers_past_their_ceiling_say_the_call_completed() {
        let out = cap_answer("world_list", ToolOutput::text("x".repeat(DEFAULT_ANSWER + 1)));
        assert!(out.is_error);
        let j = Json::parse(text(&out)).unwrap();
        assert_eq!(j.get("error").and_then(Json::as_str), Some("answer_too_large"));
        assert_eq!(j.get("completed"), Some(&Json::Bool(true)));
        assert!(!cap_answer("project_read", ToolOutput::text("x".repeat(DEFAULT_ANSWER + 1))).is_error, "its own ceiling");
        let img = ToolOutput { content: vec![Content::Image { mime_type: "image/png".into(), data_base64: "A".repeat(DEFAULT_ANSWER * 4) }, Content::Text("{}".into())], is_error: false };
        assert!(!cap_answer("sight_frame", img).is_error, "images apart");
    }

    fn def(name: &str, schema: &str) -> ToolDef {
        let mut d = vec![ToolDef { name: name.into(), description: String::new(), input_schema: Json::parse(schema).unwrap() }];
        add_to_schemas(&mut d);
        d.remove(0)
    }

    #[test]
    fn schemas_carry_the_limits_they_enforce() {
        let d = def(
            "world_query",
            r#"{"type": "object", "properties": {"fields": {"type": "array", "items": {"type": "string"}}, "path_prefix": {"type": "string"},
                "region": {"type": "object", "properties": {"min": {"type": "array"}}}, "props": {"type": "object"}}}"#,
        );
        let p = d.input_schema.get("properties").unwrap();
        assert_eq!(p.get("fields").and_then(|f| f.get("maxItems")).and_then(Json::as_u64), Some(32));
        assert_eq!(p.get("fields").and_then(|f| f.get("uniqueItems")), Some(&Json::Bool(true)));
        assert_eq!(p.get("fields").and_then(|f| f.get("items")).and_then(|i| i.get("maxLength")).and_then(Json::as_u64), Some(DEFAULT_STRING as u64));
        assert_eq!(p.get("region").and_then(|r| r.get("properties")).and_then(|r| r.get("min")).and_then(|m| m.get("maxItems")).and_then(Json::as_u64), Some(DEFAULT_ITEMS as u64));
        let ok = |a: &str| check_args(&d, &Json::parse(a).unwrap());
        assert!(ok(r#"{"fields": ["id", "path"], "path_prefix": "/a"}"#).is_ok());
        let e = ok(r#"{"fields": ["id", "id"]}"#).unwrap_err();
        assert_eq!(e.code, "duplicate_items");
        let many = format!(r#"{{"fields": [{}]}}"#, vec!["\"id\""; 33].join(","));
        assert_eq!(ok(&many).unwrap_err().code, "argument_too_large");
        // Characters, not bytes (review of 998af3d, minor 1): 4096 three-byte characters fit.
        let wide = format!(r#"{{"path_prefix": "{}"}}"#, "日".repeat(DEFAULT_STRING));
        assert!(ok(&wide).is_ok());
        let wide = format!(r#"{{"path_prefix": "{}"}}"#, "日".repeat(DEFAULT_STRING + 1));
        assert_eq!(ok(&wide).unwrap_err().got, DEFAULT_STRING + 1);
        let long = format!(r#"{{"path_prefix": "/{}"}}"#, "a".repeat(DEFAULT_STRING));
        assert_eq!(ok(&long).unwrap_err().field, "path_prefix");
        let key = format!(r#"{{"{}": 1}}"#, "k".repeat(65));
        let e = ok(&key).unwrap_err();
        assert_eq!(e.code, "argument_key_too_long");
        assert!(e.to_output().content.iter().all(|c| matches!(c, Content::Text(t) if t.len() < 1024)));
        // Free-form data is bounded more loosely.
        let data = format!(r#"{{"props": {{"note": "{}"}}}}"#, "a".repeat(DEFAULT_STRING * 2));
        assert!(ok(&data).is_ok());
        let data = format!(r#"{{"props": {{"note": "{}"}}}}"#, "a".repeat(DATA_STRING + 1));
        assert_eq!(ok(&data).unwrap_err().field, "props.note");
        // An undescribed key's value (the parser refuses its name) is bounded too.
        let stray = format!(r#"{{"strays": [{}]}}"#, vec!["0"; DATA_ITEMS + 1].join(","));
        assert!(ok(&stray).is_err());
    }

    /// Every string and array property of every tool, with its schema: `(tool, path, schema)`.
    fn properties(defs: &[ToolDef]) -> Vec<(String, String, Json)> {
        fn visit(s: &Json, tool: &str, path: &str, out: &mut Vec<(String, String, Json)>) {
            let (string, array, _) = types_of(s);
            if (string || array || s.get("maxItems").is_some()) && !path.is_empty() {
                out.push((tool.to_owned(), path.to_owned(), s.clone()));
            }
            if let Some(Json::Obj(p)) = s.get("properties") {
                for (k, v) in p {
                    visit(v, tool, &join(path, k), out);
                }
            }
            match s.get("items") {
                Some(Json::Arr(list)) => list.iter().for_each(|i| visit(i, tool, &format!("{path}[]"), out)),
                Some(i) => visit(i, tool, &format!("{path}[]"), out),
                None => {}
            }
            for key in ["oneOf", "anyOf"] {
                for a in s.get(key).and_then(Json::as_array).unwrap_or_default() {
                    visit(a, tool, path, out);
                }
            }
        }
        let mut out = Vec::new();
        for d in defs {
            visit(&d.input_schema, &d.name, "", &mut out);
        }
        out
    }

    fn served() -> Vec<ToolDef> {
        let mut all = crate::toolscope_tools::definitions();
        all.extend(crate::tools::definitions());
        crate::response::add_to_schemas(&mut all);
        crate::toolscope::add_to_schemas(&mut all);
        add_to_schemas(&mut all);
        all
    }

    /// Every override names a real string or array property, with its reason.
    #[test]
    fn every_override_names_a_real_property() {
        let props = properties(&served());
        for o in OVERRIDES {
            assert!(!o.why.is_empty(), "{}.{} needs its reason", o.tool, o.path);
            assert!(props.iter().any(|(t, p, _)| t == o.tool && p == o.path), "override {}.{} names no string or array property of that tool", o.tool, o.path);
        }
    }

    /// Every string and array property of every tool declares its limit, and the limit it declares
    /// is the one enforced: at the limit it passes the bounds, one past it is refused.
    #[test]
    fn every_property_declares_the_limit_it_enforces() {
        let defs = served();
        let props = properties(&defs);
        assert!(props.len() > 300, "{} properties", props.len());
        let mut missing = Vec::new();
        for (tool, path, s) in &props {
            let (string, array, _) = types_of(s);
            if (string && s.get("maxLength").is_none()) || (array && s.get("maxItems").is_none()) {
                missing.push(format!("{tool}.{path}"));
            }
        }
        assert!(missing.is_empty(), "properties without a declared limit: {missing:?}");
        // Top-level properties: at the limit passes, past it is refused (nested ones are checked
        // by the same walker, through their parents).
        for d in &defs {
            let Some(Json::Obj(top)) = d.input_schema.get("properties") else { continue };
            for (k, s) in top {
                if d.name == "tools_invoke" && k == "arguments" {
                    continue;
                }
                let (string, array, _) = types_of(s);
                let args = |v: Json| Json::obj([(k.as_str(), v)]);
                if string {
                    let max = s.get("maxLength").and_then(Json::as_u64).unwrap() as usize;
                    assert!(check_args(d, &args(Json::from("a".repeat(max)))).is_ok(), "{}.{k} at {max}", d.name);
                    assert!(check_args(d, &args(Json::from("a".repeat(max + 1)))).is_err(), "{}.{k} past {max}", d.name);
                }
                if array && !string {
                    let max = s.get("maxItems").and_then(Json::as_u64).unwrap() as usize;
                    let at = Json::Arr((0..max).map(|i| Json::from(i as u64)).collect());
                    assert!(check_args(d, &args(at)).map_err(|e| e.code).err() != Some("argument_too_large"), "{}.{k} at {max}", d.name);
                    assert!(check_args(d, &args(Json::Arr(vec![Json::Null; max + 1]))).is_err(), "{}.{k} past {max}", d.name);
                }
            }
        }
    }
}
