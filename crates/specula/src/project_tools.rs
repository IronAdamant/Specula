//! The `project_*` tools: project files through the protocol (see [`crate::project`] and
//! docs/design/PROJECT_DATA.md).
//!
//! **Threads.** The main thread only snapshots (text is shared, not copied) and commits
//! (`project_engine`). Patching, validation and diffs run here, on the calling agent's protocol
//! thread: an edit takes a snapshot, applies its ops, validates the result and sends a commit that
//! carries the version and hash it validated against. If the file changed meanwhile the commit is
//! refused and the edit is redone on the new text (four tries, then `busy`), unless the caller
//! passed `expect_version`: then it gets `stale_version` with conflict help.

use crate::project::{check_path, FileKind, ProjectFile, MAX_FILE_BYTES};
use crate::project_kinds::{self, Problem};
use crate::project_text::{self as text, Hunk, JsonChange};
use crate::schedule::Class;
use crate::tools::{argument_error, bool_arg, check_keys, string_arg, Command, Tools};
use crate::world::WorldError;
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;

/// How long a project call waits for its inner answer once the engine took the command: a commit
/// answers when its file is on disk (written on a worker), and calls that read the disk wait for a
/// sync in progress (docs/design/WORKERS.md). Comfortably inside the caller's own 30 s.
pub(crate) const DEFERRED_WAIT: std::time::Duration = std::time::Duration::from_secs(25);

/// Most bytes of text one `project_read` returns.
const READ_BYTES: usize = 64 * 1024;
/// Most ops per patch.
const MAX_OPS: usize = 200;
/// Conflict help is cut to fit this.
const CONFLICT_BYTES: usize = 8 * 1024;
/// A JSON value larger than this (compact) is shown by its size.
const VALUE_BYTES: usize = 256;
/// Tries before an edit gives up on a file others keep changing.
const EDIT_ATTEMPTS: usize = 4;
/// Problems listed in an answer.
const MAX_PROBLEMS: usize = 50;
/// project_list pages (default, most).
pub const LIST_LIMITS: (usize, usize) = (200, 500);

/// What the main thread does for a project tool.
pub enum ProjectCall {
    /// Files and mounts, paged.
    List { prefix: Option<String>, kind: Option<FileKind>, after: Option<String>, limit: usize },
    /// A file now (re-read from disk first if it changed there), the text it had at `version`
    /// and the changes since, and (`context`) the files validating it needs.
    Snapshot { path: String, version: Option<u64>, context: bool, reply: Sender<Snap> },
    /// Writes `file` (`None`: deletes) if the file is still at `base` (version, hash; version 0 =
    /// absent), then syncs the disk and hot-reloads. `replace_unreadable`: replace a file held
    /// because the disk holds something that can't be mirrored (its bytes are backed up first).
    /// `also`: an entity change committed with the file in the same journal entry (claims
    /// checked on both, one undo takes both back): terrain_edit's collider.
    Commit { path: String, base: (u64, u64), file: Option<(FileKind, Arc<str>)>, replace_unreadable: bool, also: Option<Also>, reply: Sender<Committed> },
    /// Mirrors a module crate's folder as `modules/<name>`.
    Mount { dir: PathBuf, name: String },
    /// Before `logic_build {fix: true}` writes fixes into a module: refused while another agent
    /// claims any of its files (when it is mounted).
    FixGate { dir: PathBuf },
    /// A mounted module's files as the store holds them (what a fix build compiles).
    ModuleTexts { dir: PathBuf, reply: Sender<Result<Vec<ModuleText>, Json>> },
    /// A fix build's fixed texts `(path, version, hash it was worked out from, new text)`,
    /// committed as the caller's change; files changed since are skipped.
    CommitFixes { files: Vec<(String, u64, u64, String)>, reply: Sender<FixesCommitted> },
    /// Who the caller is (its identity key): uploads belong to it.
    Owner { reply: Sender<String> },
    /// Every file of one kind as the store holds it now (`(path, version, text)`, at most
    /// `max`; text shared, not copied): the string tables `l10n_*` read together.
    OfKind { kind: FileKind, max: usize, reply: Sender<Vec<(String, u64, Arc<str>)>> },
    /// Several files written in one journal entry (each still at its base, each checked as
    /// [`ProjectCall::Commit`] checks one): all or none, one undo takes them all back
    /// (`sprite_bake`'s atlas and its frame data).
    CommitFiles { files: Vec<FileWrite>, reply: Sender<Committed> },
}

/// One file of a multi-file commit ([`ProjectCall::CommitFiles`]): its path, the base
/// (version, hash) it must still be at, and its kind and text.
pub type FileWrite = (String, (u64, u64), (FileKind, Arc<str>));

/// An entity change committed with a project file ([`ProjectCall::Commit`]'s `also`).
pub type Also = Box<(crate::world::EntityKey, crate::world::UpdateSpec)>;

/// Makes [`ProjectCall::Commit`]'s `also` from the file's new text (before anything is
/// committed; an error answers the call and nothing changes).
pub type AlsoFrom<'a> = &'a dyn Fn(&str) -> Result<Option<Also>, ToolOutput>;

/// One mounted module file (see [`ProjectCall::ModuleTexts`]).
pub struct ModuleText {
    /// Canonical disk path (what rustc's diagnostics name).
    pub disk: PathBuf,
    pub path: String,
    pub version: u64,
    pub hash: u64,
    pub text: Arc<str>,
}

/// How [`ProjectCall::CommitFixes`] went.
#[derive(Default)]
pub struct FixesCommitted {
    pub committed: Vec<String>,
    /// `(path, why)`: files whose fixes were not applied.
    pub skipped: Vec<(String, String)>,
    /// The commit was refused (a claim): nothing was written.
    pub error: Option<Json>,
    pub journal: Option<u64>,
    /// Committed, but a write didn't reach the disk: no more rounds.
    pub stop: bool,
}

impl ProjectCall {
    pub fn class(&self) -> Class {
        match self {
            ProjectCall::List { .. } | ProjectCall::Snapshot { .. } => Class::Read,
            ProjectCall::FixGate { .. } | ProjectCall::ModuleTexts { .. } | ProjectCall::Owner { .. } | ProjectCall::OfKind { .. } => Class::Read,
            ProjectCall::Commit { .. } | ProjectCall::Mount { .. } | ProjectCall::CommitFixes { .. } | ProjectCall::CommitFiles { .. } => Class::Disk,
        }
    }
}

/// A snapshot (see [`ProjectCall::Snapshot`]).
#[derive(Default)]
pub struct Snap {
    /// The path's kind (`None`: not in a mount, or not a kind that is mirrored).
    pub kind: Option<FileKind>,
    /// The module crate's folder, for a module file.
    pub module_dir: Option<PathBuf>,
    pub current: Option<ProjectFile>,
    /// The text at the version asked for (`None`: not kept, or the file didn't exist then).
    pub at_version: Option<Arc<str>>,
    /// Journal entries that changed the file after that version, newest first.
    pub later: Vec<Json>,
    pub context: Vec<(String, FileKind, Arc<str>)>,
    pub error: Option<WorldError>,
    /// The file is held (the disk holds bytes that can't be mirrored).
    pub held: bool,
    /// Why the caller couldn't write the file now (another agent's claim on it): checked even
    /// when a write turns out to change nothing, so a no-op answers as a write would (M4 final
    /// audit: unchanged text skipped claims).
    pub claimed: Option<WorldError>,
}

/// How a commit went.
pub enum Committed {
    /// Done: the answer's fields (version, journal, disk, reloaded).
    Done(Json),
    /// The file isn't at the base any more (someone changed it, or the disk did).
    Changed,
    Refused(WorldError),
}

fn err(code: &'static str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn world_err(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_string())
}

/// Answers a `project_*` call (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let out = match name {
        "project_list" => list_tool(tools, session, args),
        "project_read" => read_tool(tools, session, args),
        "project_write" => edit_tool(tools, session, args, "project_write", None),
        "project_patch" => edit_tool(tools, session, args, "project_patch", None),
        "project_delete" => edit_tool(tools, session, args, "project_delete", None),
        "project_mount" => mount_tool(tools, session, args),
        "project_upload" => crate::project_binary::upload_tool(tools, session, args),
        other => Err(format!("unknown project tool '{other}'")),
    };
    out.unwrap_or_else(|m| ToolOutput::error(argument_error(&m).to_string()))
}

pub(crate) fn path_arg(args: &Json) -> Result<String, String> {
    let p = string_arg(args, "path")?.ok_or("'path' is required: a project path like \"assets/ui/hud.json\" (project_list shows them)")?;
    Ok(p.trim_start_matches('/').strip_prefix("project/").unwrap_or(p.trim_start_matches('/')).to_owned())
}

fn uint(args: &Json, key: &str, lo: u64, hi: u64) -> Result<Option<u64>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if (lo..=hi).contains(&n) => Ok(Some(n)),
            _ => Err(format!("'{key}' must be an integer {lo}..{hi}")),
        },
    }
}

pub(crate) fn snapshot(tools: &Tools, session: &SessionInfo, path: &str, version: Option<u64>, context: bool) -> Result<Snap, ToolOutput> {
    let (tx, rx) = mpsc::channel();
    let out = tools.send(session, Command::Project(ProjectCall::Snapshot { path: path.to_owned(), version, context, reply: tx }));
    if out.is_error {
        return Err(out);
    }
    let snap = rx.recv_timeout(crate::project_tools::DEFERRED_WAIT).map_err(|_| err("no_answer", "the engine didn't answer the snapshot; nothing was changed"))?;
    match &snap.error {
        Some(e) => Err(world_err(e)),
        None => Ok(snap),
    }
}

fn list_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "project_list", &[&["prefix", "kind", "cursor", "limit"]])?;
    let kind = match string_arg(args, "kind")? {
        None => None,
        Some(k) => Some(FileKind::parse(k).ok_or_else(|| format!("'kind' {k:?} is not a file kind; kinds: {}", FileKind::ALL.map(FileKind::name).join(", ")))?),
    };
    let limit = uint(args, "limit", 1, LIST_LIMITS.1 as u64)?.map_or(LIST_LIMITS.0, |l| l as usize);
    let prefix = string_arg(args, "prefix")?.map(|p| p.trim_start_matches('/').strip_prefix("project/").unwrap_or(p.trim_start_matches('/')).to_owned());
    let after = string_arg(args, "cursor")?.map(str::to_owned);
    Ok(tools.send(session, Command::Project(ProjectCall::List { prefix, kind, after, limit })))
}

fn problems_json(problems: &[Problem]) -> Json {
    let mut v: Vec<&Problem> = problems.iter().collect();
    v.sort_by_key(|p| !p.is_error());
    Json::arr(v.into_iter().take(MAX_PROBLEMS).map(Problem::to_json))
}

fn read_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "project_read", &[&["path", "version", "from_line", "to_line", "json_path", "problems", "bytes", "offset", "length"]])?;
    let path = path_arg(args)?;
    let version = uint(args, "version", 1, u64::MAX >> 11)?;
    let from = uint(args, "from_line", 1, u32::MAX as u64)?.unwrap_or(1) as usize;
    let to = uint(args, "to_line", 1, u32::MAX as u64)?.map(|t| t as usize);
    let json_path = string_arg(args, "json_path")?;
    let want_problems = bool_arg(args, "problems")?.unwrap_or(true);
    let snap = match snapshot(tools, session, &path, version, want_problems) {
        Ok(s) => s,
        Err(out) => return Ok(out),
    };
    let Some(cur) = &snap.current else {
        let mut j = Json::obj([("error", Json::from("not_found")), ("message", Json::from(format!("there is no project file {path} (project_list shows them; project_write creates one)"))), ("path", Json::from(path.as_str()))]);
        if let Some(k) = snap.kind {
            j.set("kind", Json::from(k.name()));
        }
        return Ok(ToolOutput::error(j.to_string()));
    };
    // Binary files: metadata (and bytes on request) from the content store.
    if cur.kind.is_binary() {
        return crate::project_binary::read(tools, &path, &snap, version, args);
    }
    if args.get("bytes").is_some() || args.get("offset").is_some() || args.get("length").is_some() {
        return Err(format!("'bytes', 'offset' and 'length' read binary files; {path} is a {} file (text): use from_line / to_line", cur.kind.name()));
    }
    let (body, shown_version): (Arc<str>, u64) = match version {
        Some(v) if v != cur.version => match &snap.at_version {
            Some(t) => (Arc::clone(t), v),
            None => {
                return Ok(err("not_kept", format!("version {v} of {path} isn't in the journal any more (or never existed); the current version is {}", cur.version)));
            }
        },
        _ => (Arc::clone(&cur.text), cur.version),
    };
    let mut out = vec![("path", Json::from(path.as_str())), ("kind", Json::from(cur.kind.name())), ("version", Json::from(shown_version))];
    if shown_version != cur.version {
        out.push(("current_version", Json::from(cur.version)));
    } else {
        out.push(("modified_by", Json::from(&*cur.modified_by)));
    }
    out.push(("bytes", Json::from(body.len())));
    if let Some(jp) = json_path {
        let doc = Json::parse(&body).map_err(|e| format!("'json_path' needs a JSON file, and {path} doesn't parse ({e})"))?;
        let steps = text::parse_path(jp)?;
        let mut at = &doc;
        for (k, st) in steps.iter().enumerate() {
            at = match st {
                text::Step::Key(key) => at.get(key),
                text::Step::Index(i) => at.as_array().and_then(|a| a.get(*i)),
            }
            .ok_or_else(|| format!("'json_path' {jp:?}: {} doesn't exist in {path}", text::path_string(&steps[..=k])))?;
        }
        let s = at.to_string();
        if s.len() > READ_BYTES {
            return Ok(err("too_large", format!("{jp} is {} bytes; read a part of it (a deeper json_path) or by lines", s.len())));
        }
        out.push(("json_path", Json::from(jp)));
        out.push(("value", at.clone()));
    } else {
        let lines: Vec<&str> = body.split_inclusive('\n').collect();
        let total = lines.len();
        if from > total.max(1) {
            return Err(format!("'from_line' {from} is past the end ({total} lines)"));
        }
        let last = to.unwrap_or(total).min(total);
        let mut chunk = String::new();
        let mut end = from.saturating_sub(1);
        for l in lines.iter().take(last).skip(from - 1) {
            if !chunk.is_empty() && chunk.len() + l.len() > READ_BYTES {
                break;
            }
            chunk.push_str(l);
            end += 1;
        }
        out.push(("lines", Json::arr([Json::from(from), Json::from(end)])));
        out.push(("total_lines", Json::from(total)));
        out.push(("text", Json::from(chunk)));
        if end < last {
            out.push(("next_line", Json::from(end + 1)));
        }
    }
    if want_problems && shown_version == cur.version {
        let p = project_kinds::validate(&path, cur.kind, &cur.text, None, &snap.context);
        if !p.is_empty() {
            out.push(("problems", problems_json(&p)));
        }
        if cur.kind.is_module() {
            out.push(("validated_by", Json::from("logic_build (pass build: true or \"check\" on a write)")));
        }
    }
    Ok(ToolOutput::text(Json::obj(out).to_string()))
}

/// One patch op.
enum Op {
    Set(String, Json),
    Remove(String),
    Find(String, String),
    Lines(usize, usize, String),
}

fn parse_ops(args: &Json) -> Result<Vec<Op>, String> {
    let ops = args.get("ops").and_then(Json::as_array).ok_or("'ops' is required: an array of {set, value} / {remove} / {find, replace} / {lines: [from, to], text}")?;
    if ops.is_empty() || ops.len() > MAX_OPS {
        return Err(format!("'ops' must hold 1..{MAX_OPS} ops"));
    }
    ops.iter()
        .enumerate()
        .map(|(i, o)| {
            let s = |k: &str| o.get(k).and_then(Json::as_str).map(str::to_owned);
            let keys: Vec<&str> = o.as_object().map(|p| p.iter().map(|(k, _)| k.as_str()).collect()).unwrap_or_default();
            let only = |allowed: &[&str]| keys.iter().all(|k| allowed.contains(k));
            if let Some(p) = s("set") {
                if !only(&["set", "value"]) || o.get("value").is_none() {
                    return Err(format!("'ops[{i}]': a set op is {{\"set\": \"json.path\", \"value\": ...}}"));
                }
                Ok(Op::Set(p, o.get("value").cloned().unwrap_or(Json::Null)))
            } else if let Some(p) = s("remove") {
                if !only(&["remove"]) {
                    return Err(format!("'ops[{i}]': a remove op is {{\"remove\": \"json.path\"}}"));
                }
                Ok(Op::Remove(p))
            } else if let Some(f) = s("find") {
                match (s("replace"), only(&["find", "replace"])) {
                    (Some(r), true) => Ok(Op::Find(f, r)),
                    _ => Err(format!("'ops[{i}]': a find op is {{\"find\": \"exact text\", \"replace\": \"new text\"}}")),
                }
            } else if let Some(l) = o.get("lines").and_then(Json::as_array) {
                let n = |j: Option<&Json>| j.and_then(Json::as_u64).map(|v| v as usize);
                match (n(l.first()), n(l.get(1)), s("text"), l.len() == 2 && only(&["lines", "text"])) {
                    (Some(a), Some(b), Some(t), true) => Ok(Op::Lines(a, b, t)),
                    _ => Err(format!("'ops[{i}]': a lines op is {{\"lines\": [from, to], \"text\": \"...\"}} (1-based, inclusive; [k, k-1] inserts before line k)")),
                }
            } else {
                Err(format!("'ops[{i}]' must have 'set', 'remove', 'find' or 'lines'"))
            }
        })
        .collect()
}

fn apply_ops(body: &str, kind: FileKind, ops: &[Op]) -> Result<String, (usize, String)> {
    let mut t = body.to_owned();
    for (i, op) in ops.iter().enumerate() {
        let json_only = |what: &str| if kind.is_json() { Ok(()) } else { Err((i, format!("{what} ops need a JSON file; this is {} (use find or lines)", kind.name()))) };
        t = match op {
            Op::Set(p, v) => {
                json_only("set")?;
                text::json_set(&t, p, v)
            }
            Op::Remove(p) => {
                json_only("remove")?;
                text::json_remove(&t, p)
            }
            Op::Find(f, r) => text::find_replace(&t, f, r),
            Op::Lines(a, b, n) => text::replace_lines(&t, *a, *b, n),
        }
        .map_err(|e| (i, e))?;
        if t.len() > MAX_FILE_BYTES {
            return Err((i, format!("the file would be {} bytes; at most {MAX_FILE_BYTES}", t.len())));
        }
    }
    Ok(t)
}

/// A diff between two texts of a file: by JSON path when both parse, else by lines.
enum Diff {
    Paths(Vec<JsonChange>, bool),
    Lines(Vec<Hunk>, bool),
}

/// Above this many bytes (both texts together) files are diffed by lines (bounded work).
const JSON_DIFF_BYTES: usize = 512 * 1024;

fn diff(kind: FileKind, a: &str, b: &str) -> Diff {
    if kind.is_json() && a.len() + b.len() <= JSON_DIFF_BYTES {
        if let (Ok(x), Ok(y)) = (Json::parse(a), Json::parse(b)) {
            let (c, cut) = text::json_diff(&x, &y);
            return Diff::Paths(c, cut);
        }
    }
    let (h, cut) = text::line_diff(a, b);
    Diff::Lines(h, cut)
}

fn small(v: &Option<Json>) -> Json {
    match v {
        None => Json::from("(absent)"),
        Some(v) => {
            let s = v.to_string();
            if s.len() <= VALUE_BYTES {
                v.clone()
            } else {
                Json::obj([("omitted_bytes", Json::from(s.len()))])
            }
        }
    }
}

impl Diff {
    /// At most `n` items.
    fn to_json(&self, n: usize) -> Json {
        match self {
            Diff::Paths(c, cut) => Json::obj([
                ("paths", Json::arr(c.iter().take(n).map(|c| Json::obj([("path", Json::from(c.path.as_str())), ("before", small(&c.before)), ("after", small(&c.after))])))),
                ("cut", Json::Bool(*cut || c.len() > n)),
            ]),
            Diff::Lines(h, cut) => Json::obj([
                (
                    "hunks",
                    Json::arr(h.iter().take(n).map(|h| {
                        Json::obj([
                            // 1-based, inclusive; [k, k-1] is an insertion before line k.
                            ("base_lines", Json::arr([Json::from(h.base.0 + 1), Json::from(h.base.1)])),
                            ("removed", Json::arr(h.removed.iter().map(|l| Json::from(l.as_str())))),
                            ("added", Json::arr(h.added.iter().map(|l| Json::from(l.as_str())))),
                        ])
                    })),
                ),
                ("cut", Json::Bool(*cut || h.len() > n)),
            ]),
        }
    }

    /// Where both diffs (from the same base) touch the same thing.
    fn overlap(&self, other: &Diff) -> Option<Json> {
        match (self, other) {
            (Diff::Paths(a, _), Diff::Paths(b, _)) => Some(Json::arr(text::paths_overlap(a, b).into_iter().map(Json::from))),
            (Diff::Lines(a, _), Diff::Lines(b, _)) => {
                Some(Json::arr(text::hunks_overlap(a, b).into_iter().map(|(s, e)| Json::obj([("base_lines", Json::arr([Json::from(s + 1), Json::from(e)]))]))))
            }
            _ => None,
        }
    }
}

/// What a refused write was going to do.
enum Yours<'a> {
    Text(Option<String>),
    Ops(&'a [Op]),
    Delete,
}

/// The `stale_version` error for an edit expecting `expected`, with conflict help.
fn stale(path: &str, kind: FileKind, expected: u64, snap: &Snap, yours: Yours, tool: &str) -> ToolOutput {
    let current = snap.current.as_ref().map_or(0, |f| f.version);
    let base_text: Option<&str> = if expected == 0 { Some("") } else { snap.at_version.as_deref() };
    let cur_text: &str = snap.current.as_ref().map_or("", |f| &f.text);
    let theirs_diff = base_text.map(|b| diff(kind, b, cur_text));
    let yours_text: Option<String> = match &yours {
        Yours::Text(t) => t.clone(),
        Yours::Ops(ops) => base_text.and_then(|b| apply_ops(b, kind, ops).ok()),
        Yours::Delete => None,
    };
    let yours_diff = match (&yours_text, base_text) {
        (Some(y), Some(b)) => Some(diff(kind, b, y)),
        _ => None,
    };
    let overlap = match (&theirs_diff, &yours_diff, &yours) {
        (_, _, Yours::Delete) => Some(Json::arr([Json::from("(delete)")])),
        (Some(t), Some(y), _) => t.overlap(y),
        _ => None,
    };
    let clear = overlap.as_ref().and_then(Json::as_array).is_some_and(<[Json]>::is_empty);
    let what = if kind.is_json() { "values" } else { "lines" };
    let advice = match (&overlap, clear) {
        (Some(_), true) => format!("theirs and yours don't touch the same {what}: apply your edit to version {current} (for {tool}, send it again with expect_version {current}; for project_write, re-read and merge your change into the current text)"),
        (Some(_), false) => format!("both change the {what} listed in overlap: project_read the current version, decide, then retry with expect_version {current}"),
        (None, _) => format!("the base version isn't kept or the texts can't be compared: project_read the current version, redo your edit on it and pass expect_version {current}"),
    };
    let build = |n: usize| {
        let mut base = vec![("version", Json::from(expected)), ("kept", Json::Bool(base_text.is_some()))];
        if expected == 0 {
            base.push(("note", Json::from("expect_version 0 means the file must not exist yet")));
        }
        let mut theirs = vec![("version", Json::from(current)), ("changes", Json::arr(snap.later.iter().take(8).cloned()))];
        if let Some(d) = &theirs_diff {
            theirs.push(("diff", d.to_json(n)));
        }
        let yours_j = match (&yours, &yours_diff) {
            (Yours::Delete, _) => Json::obj([("delete", Json::Bool(true))]),
            (_, Some(d)) => Json::obj([("diff", d.to_json(n))]),
            (Yours::Ops(ops), None) => Json::obj([("ops", Json::from(ops.len())), ("note", Json::from("your ops don't apply to the base version (or it isn't kept)"))]),
            (Yours::Text(_), None) => Json::obj([("note", Json::from("the base version isn't kept"))]),
        };
        let mut c = vec![("base", Json::obj(base)), ("theirs", Json::obj(theirs)), ("yours", yours_j)];
        if let Some(o) = &overlap {
            c.push(("overlap", o.clone()));
        }
        c.push(("rebase", Json::from(advice.as_str())));
        Json::obj([
            ("error", Json::from("stale_version")),
            ("message", Json::from(format!("{path} is at version {current}, not {expected}: someone changed it since you read it; nothing was changed"))),
            ("path", Json::from(path)),
            ("expected_version", Json::from(expected)),
            ("version", Json::from(current)),
            ("conflict", Json::obj(c)),
        ])
    };
    let mut n = 24;
    let mut j = build(n);
    while j.to_string().len() > CONFLICT_BYTES && n > 1 {
        n /= 2;
        j = build(n);
    }
    ToolOutput::error(j.to_string())
}

/// `project_patch` with an entity change made from the patched text and committed with it (one
/// journal entry): terrain_edit's recipe and collider.
pub(crate) fn patch_with(tools: &Tools, session: &SessionInfo, args: &Json, also: AlsoFrom) -> ToolOutput {
    edit_tool(tools, session, args, "project_patch", Some(also)).unwrap_or_else(|m| ToolOutput::error(argument_error(&m).to_string()))
}

fn edit_tool(tools: &Tools, session: &SessionInfo, args: &Json, tool: &'static str, also_from: Option<AlsoFrom>) -> Result<ToolOutput, String> {
    let common: &[&str] = &["path", "expect_version", "allow_errors", "dry_run", "build", "replace_unreadable"];
    let extra: &[&str] = match tool {
        "project_write" => &["text"],
        "project_patch" => &["ops"],
        _ => &[],
    };
    check_keys(args, tool, &[common, extra])?;
    let path = path_arg(args)?;
    let expect = uint(args, "expect_version", 0, u64::MAX >> 11)?;
    let allow_errors = bool_arg(args, "allow_errors")?.unwrap_or(false);
    let dry_run = bool_arg(args, "dry_run")?.unwrap_or(false);
    let replace_unreadable = bool_arg(args, "replace_unreadable")?.unwrap_or(false);
    let build = match args.get("build") {
        None | Some(Json::Null) | Some(Json::Bool(false)) => None,
        Some(Json::Bool(true)) => Some(false),
        Some(Json::Str(s)) if s == "check" => Some(true),
        Some(_) => return Err("'build' must be true (build and reload the module) or \"check\" (cargo check only)".into()),
    };
    let new_text: Option<String> = match tool {
        "project_write" => Some(string_arg(args, "text")?.ok_or("'text' is required: the file's whole new text")?.to_owned()),
        _ => None,
    };
    if new_text.as_ref().is_some_and(|t| t.len() > MAX_FILE_BYTES) {
        return Ok(err("too_large", format!("project files are at most {MAX_FILE_BYTES} bytes")));
    }
    let ops = if tool == "project_patch" { parse_ops(args)? } else { Vec::new() };
    check_path(&path).map_err(|e| e.message)?;
    for _ in 0..EDIT_ATTEMPTS {
        let snap = match snapshot(tools, session, &path, expect.filter(|v| *v > 0), true) {
            Ok(s) => s,
            Err(out) => return Ok(out),
        };
        let Some(kind) = snap.kind else {
            return Ok(err("not_mirrored", format!("{path} isn't a project file: project files are recipes (a .json whose folder is recipes or models), ui screens and themes, sounds, anim sets and style files under assets/, and module sources under modules/<name>/ (project_list shows the mounts)")));
        };
        if kind.is_binary() && tool != "project_delete" {
            return Ok(err("binary_file", format!("{path} is a binary {} file: write it with project_upload (project_read shows it; project_delete removes it)", kind.name())));
        }
        let current_version = snap.current.as_ref().map_or(0, |f| f.version);
        let current_hash = snap.current.as_ref().map_or(0, |f| f.hash);
        if let Some(e) = expect {
            if e != current_version {
                let yours = match tool {
                    "project_write" => Yours::Text(new_text.clone()),
                    "project_patch" => Yours::Ops(&ops),
                    _ => Yours::Delete,
                };
                return Ok(stale(&path, kind, e, &snap, yours, tool));
            }
        }
        let old = snap.current.as_ref().map(|f| Arc::clone(&f.text));
        let next: Option<String> = match tool {
            "project_delete" => {
                if old.is_none() {
                    return Ok(err("not_found", format!("there is no project file {path}")));
                }
                None
            }
            "project_write" => new_text.clone(),
            _ => {
                let Some(o) = &old else { return Ok(err("not_found", format!("there is no project file {path} to patch; project_write creates it"))) };
                match apply_ops(o, kind, &ops) {
                    Ok(t) => Some(t),
                    Err((i, m)) => {
                        return Ok(ToolOutput::error(
                            Json::obj([("error", Json::from("patch_failed")), ("message", Json::from(format!("ops[{i}]: {m}; nothing was changed"))), ("op_index", Json::from(i)), ("path", Json::from(path.as_str()))]).to_string(),
                        ))
                    }
                }
            }
        };
        if let (Some(n), Some(o), false) = (&next, &old, snap.held && replace_unreadable) {
            if n.as_str() == &**o {
                // Claims first: a write that changes nothing is still refused where a write would be.
                if let Some(e) = &snap.claimed {
                    let mut j = e.to_json();
                    j.set("hint", Json::from(format!("{path} is claimed (claims on files are /project/<path>): work elsewhere, ask the owner to claim_share, or wait for the claim to end")));
                    return Ok(ToolOutput::error(j.to_string()));
                }
                return Ok(ToolOutput::text(Json::obj([("path", Json::from(path.as_str())), ("version", Json::from(current_version)), ("unchanged", Json::Bool(true))]).to_string()));
            }
        }
        let problems = match (next.as_deref(), old.as_deref()) {
            (Some(n), o) if kind == FileKind::Cargo => match &snap.module_dir {
                Some(dir) => project_kinds::check_module_manifest_at(dir, &path, n),
                None => project_kinds::validate(&path, kind, n, o, &snap.context),
            },
            (Some(n), o) => project_kinds::validate(&path, kind, n, o, &snap.context),
            // A delete is checked against what depends on the file (a style's recipes, a theme's screens).
            (None, Some(o)) => project_kinds::validate_delete(&path, kind, o, &snap.context),
            (None, None) => Vec::new(),
        };
        let errors = problems.iter().filter(|p| p.is_error()).count();
        // A module's manifest and sources are built (and its manifest read by the whole Cargo
        // workspace it may belong to): these never reach the disk, draft or not.
        let hard = problems.iter().filter(|p| p.is_error() && project_kinds::never_allowed(kind, &p.code)).count();
        if errors > 0 && (!allow_errors || hard > 0) {
            let why = if hard > 0 && kind == crate::project::FileKind::Strings {
                format!("{path}: a string table that doesn't parse is never saved, allow_errors or not (the game would show no words, and the table's reviews would be lost to the review gate); nothing was changed. Fix what the problems say")
            } else if hard > 0 && kind == crate::project::FileKind::Game {
                format!("{path}: a game file that is not one is never saved, allow_errors or not (the project would not open at its next start); nothing was changed. Fix what the problems say")
            } else if hard > 0 {
                format!("{path}: {hard} problem(s) that allow_errors can't override (a module's manifest and sources must stay a module's: no crates.io dependencies, build scripts or files from outside); nothing was changed")
            } else {
                format!("{path} would not load ({errors} error(s)); nothing was changed. Fix what the problems say (allow_errors stores a draft anyway)")
            };
            return Ok(ToolOutput::error(
                Json::obj([
                    ("error", Json::from("invalid")),
                    ("message", Json::from(why)),
                    ("path", Json::from(path.as_str())),
                    ("kind", Json::from(kind.name())),
                    ("problems", problems_json(&problems)),
                ])
                .to_string(),
            ));
        }
        // Marking string-table entries reviewed is a reviewer's (l10n_tools::review_gate).
        if kind == FileKind::Strings {
            if let Err(out) = crate::l10n_tools::review_gate(session, old.as_deref(), next.as_deref()) {
                return Ok(out);
            }
        }
        let change = diff(kind, old.as_deref().unwrap_or(""), next.as_deref().unwrap_or(""));
        // The entity change that goes with this text (checked on a dry run too).
        let also = match (also_from, next.as_deref()) {
            (Some(f), Some(n)) => match f(n) {
                Ok(a) => a,
                Err(out) => return Ok(out),
            },
            _ => None,
        };
        if dry_run {
            let mut j = vec![("dry_run", Json::Bool(true)), ("path", Json::from(path.as_str())), ("kind", Json::from(kind.name())), ("version", Json::from(current_version)), ("changes", change.to_json(24))];
            if !problems.is_empty() {
                j.push(("problems", problems_json(&problems)));
            }
            return Ok(ToolOutput::text(Json::obj(j).to_string()));
        }
        let (tx, rx) = mpsc::channel();
        let file = next.map(|n| (kind, Arc::<str>::from(n)));
        let out = tools.send(session, Command::Project(ProjectCall::Commit { path: path.clone(), base: (current_version, current_hash), file, replace_unreadable, also, reply: tx }));
        if out.is_error {
            return Ok(out);
        }
        match rx.recv_timeout(crate::project_tools::DEFERRED_WAIT) {
            Ok(Committed::Changed) => continue,
            Ok(Committed::Refused(e)) => {
                let mut j = e.to_json();
                if e.code == "claimed" || e.code == "claims_overlap" {
                    j.set("hint", Json::from(format!("{path} is claimed (claims on files are /project/<path>): work elsewhere, ask the owner to claim_share, or wait for the claim to end")));
                }
                return Ok(ToolOutput::error(j.to_string()));
            }
            Ok(Committed::Done(mut j)) => {
                j.set("kind", Json::from(kind.name()));
                j.set("changes", change.to_json(8));
                if !problems.is_empty() {
                    j.set("problems", problems_json(&problems));
                }
                if let (Some(check), true) = (build, kind.is_module()) {
                    if let Some(dir) = &snap.module_dir {
                        let rel = dir.strip_prefix(std::env::current_dir().map(|d| specula_logic::fix::canonical(&d)).unwrap_or_default()).unwrap_or(dir);
                        let a = Json::obj([("path", Json::from(rel.to_string_lossy().replace('\\', "/"))), ("check", Json::Bool(check)), ("reload", Json::Bool(!check))]);
                        let report = crate::logic::tools::build(tools.engine(), session, &a);
                        let text = report.content.iter().find_map(|c| match c {
                            specula_protocol::Content::Text(t) => Some(t.clone()),
                            _ => None,
                        });
                        let rj = text.as_deref().and_then(|t| Json::parse(t).ok()).unwrap_or(Json::Null);
                        j.set("build", rj);
                        j.set("build_ok", Json::Bool(!report.is_error));
                        if report.is_error {
                            j.set("hint", Json::from("the source is saved (journaled; world_undo takes it back) but didn't build: the running module keeps its last good build"));
                        }
                    }
                }
                return Ok(ToolOutput::text(j.to_string()));
            }
            Err(_) => return Ok(err("no_answer", "the engine didn't answer the commit")),
        }
    }
    Ok(err("busy", format!("others kept changing {path} while this edit was validated ({EDIT_ATTEMPTS} tries); claim_take /project/{path} first, then edit")))
}

fn mount_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "project_mount", &[&["module"]])?;
    let module = string_arg(args, "module")?.ok_or("'module' is required: a module crate's folder under the working directory, e.g. \"games/ambulo\"")?;
    let dir = crate::logic::tools::project_path(module, "module")?;
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).map_err(|_| format!("'module': {module} has no Cargo.toml (it must be a module crate's folder)"))?;
    let doc = specula_core::toml::parse(&manifest).map_err(|e| format!("'module': {module}/Cargo.toml doesn't parse: {}", e.message))?;
    let Some(name) = doc.get("package").and_then(|p| p.get("name")).and_then(Json::as_str) else {
        return Ok(err("not_a_module", format!("{module}/Cargo.toml has no [package]: a workspace root isn't a module crate; nothing was mounted")));
    };
    let mount = format!("{}/{name}", crate::project::MODULES);
    check_path(&mount).map_err(|e| format!("'module': the crate name gives the mount {mount:?}: {}", e.message))?;
    Ok(tools.send(session, Command::Project(ProjectCall::Mount { dir, name: mount })))
}

/// The tools' schemas (their documentation for agents).
pub fn definitions() -> Vec<ToolDef> {
    let obj = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let path = || t("string", "A project path: mount then path, e.g. \"assets/ui/hud.json\", \"assets/recipes/tombstone.json\", \"modules/ambulo/src/lib.rs\" (project_list shows them). Claims on it are /project/<path>.");
    let expect = || t("integer", "The version you last read (project_read). If someone changed the file since, nothing is written and you get stale_version with conflict help: their diff, yours, the overlap and how to rebase. 0 means the file must not exist yet.");
    let common = |mut v: Vec<(&'static str, Json)>| {
        v.push(("path", path()));
        v.push(("expect_version", expect()));
        v.push(("allow_errors", t("boolean", "Store the edit even if validation finds errors (a draft). Default false: an edit that would leave the file unloadable is refused with every problem.")));
        v.push(("dry_run", t("boolean", "Validate and return the diff without writing.")));
        v.push(("build", Json::obj([("description", Json::from("Module files only: after the write, run logic_build for the module: true builds and reloads it, \"check\" only checks. Its diagnostics come back under 'build'; a failed build never replaces the running module."))])));
        v.push(("replace_unreadable", t("boolean", "The file on disk was changed into something that can't be mirrored (not UTF-8, or over 1 MiB) and is held (disk_unreadable): replace it anyway. Its bytes are backed up next to it first (the answer names the backup).")));
        v
    };
    vec![
        ToolDef {
            name: "project_list".into(),
            description: "Project files: the game's text files (model recipes, UI screens and themes, sound recipes, anim sets, style.json and style.toml under assets/; module sources under modules/<name>/) mirrored with versions, claims, the journal and undo like world entities. Lists the mounts, then files (path, kind, version, bytes, last writer), paged; files whose disk write failed and disk conflicts (edits made outside the engine) come first.".into(),
            input_schema: obj(
                vec![
                    ("prefix", t("string", "Only paths starting with this, e.g. \"assets/ui/\".")),
                    ("kind", t("string", "Only this kind: recipe, style, style_rules, ui_screen, ui_theme, sound, anim_set, rust, cargo, json; binary: png, glb, gltf, wav.")),
                    ("cursor", t("string", "next_cursor from the previous page.")),
                    ("limit", t("integer", "Files per page, 1..500 (default 200).")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "project_read".into(),
            description: "A project file's text and version (pass the version to a write as expect_version). At most 64 KiB per call: from_line/to_line pick lines, next_line continues. json_path returns one value of a JSON file (\"parts[1].shape\"). version reads an earlier version from the journal. 'problems' says what validation finds now (the same checks a write runs). Binary files (png, glb, gltf, wav) answer metadata (bytes, content id, and 'info': image size, audio rate and length, model counts); bytes: true or offset/length return up to 192 KiB of their bytes as base64 (next_offset continues).".into(),
            input_schema: obj(
                vec![
                    ("path", path()),
                    ("version", t("integer", "An earlier version (from the journal, while kept).")),
                    ("from_line", t("integer", "First line (1-based, default 1).")),
                    ("to_line", t("integer", "Last line (inclusive).")),
                    ("json_path", t("string", "Just this value, e.g. \"root.children[2].text\".")),
                    ("problems", t("boolean", "Validate and list problems (default true).")),
                    ("bytes", t("boolean", "Binary files: return bytes (base64) from offset (default 0), length (default 64 KiB).")),
                    ("offset", t("integer", "Binary files: first byte to return.")),
                    ("length", t("integer", "Binary files: bytes to return, 1..196608.")),
                ],
                &["path"],
            ),
        },
        ToolDef {
            name: "project_write".into(),
            description: "Writes a project file's whole text (creates it, and its folders, when new). Validated by kind before anything changes (a recipe against its style, a screen against its theme, a style or theme against everything using it, sounds, style.toml's [motion]; module sources through logic_build with build). Claim-checked (/project/<path>), journaled with you as author (world_history, world_changes; world_undo takes it back on disk too), written to disk atomically, and hot-reloaded (models rebuild, the game reloads screens and sounds). Answers the new version, the journal id, the disk state and the diff.".into(),
            input_schema: obj(common(vec![("text", t("string", "The whole new text (UTF-8, at most 1 MiB)."))]), &["path", "text"]),
        },
        ToolDef {
            name: "project_patch".into(),
            description: "Edits a project file with atomic ops, keeping its formatting: {\"set\": \"parts[1].shape.size\", \"value\": [0.6, 0.8]} and {\"remove\": \"notes\"} for JSON files (only that value's bytes change; missing objects on the way are made; an index one past the end appends), {\"find\": \"exact text\", \"replace\": \"new\"} (must occur once) and {\"lines\": [from, to], \"text\": \"...\"} for any file. All ops apply or none; validated, claim-checked, journaled, written and hot-reloaded like project_write.".into(),
            input_schema: obj(
                common(vec![(
                    "ops",
                    Json::obj([
                        ("type", Json::from("array")),
                        ("description", Json::from("1..200 ops, applied in order: {set, value} | {remove} | {find, replace} | {lines: [from, to], text} (lines 1-based inclusive; [k, k-1] inserts before line k).")),
                        ("items", Json::obj([("type", Json::from("object"))])),
                    ]),
                )]),
                &["path", "ops"],
            ),
        },
        ToolDef {
            name: "project_delete".into(),
            description: "Deletes a project file, on disk too (claim-checked, journaled; world_undo brings it back).".into(),
            input_schema: obj(vec![("path", path()), ("expect_version", expect()), ("allow_errors", t("boolean", "Delete even if a style's recipes or a theme's screens would no longer load.")), ("dry_run", t("boolean", "Check what the delete would break without deleting.")), ("replace_unreadable", t("boolean", "Delete a file held as disk_unreadable (its bytes are backed up first)."))], &["path"]),
        },
        ToolDef {
            name: "project_mount".into(),
            description: "Mirrors a module crate's sources as project files under modules/<crate name>/: Cargo.toml, top-level *.json (actions.json) and src/**/*.rs. Only real modules mount (a cdylib depending on specula-sdk alone, by path; no build.rs): never the engine's crates or the project root (not_a_module). Every Cargo.toml edit is checked the same way. Then edit them with project_write / project_patch and build: true (runs logic_build).".into(),
            input_schema: obj(vec![("module", t("string", "The crate's folder under the engine's working directory, e.g. \"games/ambulo\" (as for logic_build)."))], &["module"]),
        },
    ]
    .into_iter()
    .chain(crate::project_binary::definitions())
    .collect()
}
