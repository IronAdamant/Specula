//! Arguments and definitions of the review queue (`review_*`), critique (`critique_*`), merge
//! (`merge_suggest`) and director (`task_plan`, `task_progress`) tools. The engine side is in
//! `review_engine`; the data in `review`, `merge` and `director`.

use crate::board::check_text;
use crate::claims::Scope;
use crate::merge::{self, Kind as MergeKind, Prefer};
use crate::response::Format;
use crate::review::{
    self, CStatus, Capture, Check, CritiqueAction, CritiqueQuery, CritiqueSpec, Metric, Status, SubmissionQuery, Target, CAPTURE_KINDS, CHECK_KINDS, KINDS, SEVERITIES,
};
use crate::schedule::Class;
use crate::tools::{bool_arg, check_keys, int_arg, string_arg};
use specula_core::Json;
use specula_protocol::{ToolDef, ToolOutput};

/// Largest id accepted (JSON numbers are exact up to 2^53).
const MAX_ID: u64 = u64::MAX >> 11;
/// Submissions one `review_decide` call decides.
pub const MAX_DECISIONS: usize = 64;

/// What `review_submit` asks for, before the engine resolves paths and checks files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubmitArgs {
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub batch: Option<String>,
    pub task: Option<u64>,
    pub paths: Vec<String>,
    pub captures: Vec<Capture>,
    pub checks: Vec<Check>,
    /// Results the engine recorded, referenced by id and epoch (`crate::records`).
    pub results: Vec<review::ResultRef>,
    pub journal: Vec<u64>,
    pub journal_since: Option<u64>,
    pub promote: Option<review::Promote>,
    pub supersedes: Option<u64>,
}

/// Which submissions `review_decide` decides.
#[derive(Clone, Debug, PartialEq)]
pub enum Which {
    Ids(Vec<u64>),
    /// Every pending submission of this batch.
    Batch(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecideOne {
    pub id: u64,
    pub verdict: Status,
    pub note: String,
    pub revert: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScreenOp {
    Open(Option<u64>),
    Close,
    Next,
    Prev,
    Status,
}

#[derive(Debug)]
pub enum ReviewCall {
    Submit(SubmitArgs),
    List { query: SubmissionQuery, id: Option<u64>, detailed: bool },
    /// `which` + one verdict, or explicit decisions.
    Decide { which: Option<Which>, verdict: Option<Status>, note: String, revert: bool, decisions: Vec<DecideOne>, promote: bool, force: bool },
    Screen(ScreenOp),
    CritiqueAdd(Vec<(CritiqueSpec, Option<String>)>),
    CritiqueList(CritiqueQuery),
    CritiqueUpdate { ids: Vec<u64>, action: CritiqueAction, expect: Option<u64> },
    Plan(PlanArgs),
    Progress { plan: u64, detailed: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanSource {
    File { file: String, section: String },
    Text { text: String, section: String },
    Items(Vec<crate::director::PlanItem>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanArgs {
    pub source: PlanSource,
    pub title: Option<String>,
    pub by_role: bool,
    pub dry_run: bool,
    pub priority: Option<u8>,
    /// An existing task the plan goes under.
    pub parent: Option<u64>,
}

impl ReviewCall {
    pub fn class(&self) -> Class {
        match self {
            ReviewCall::List { .. } | ReviewCall::CritiqueList(_) | ReviewCall::Progress { .. } | ReviewCall::Screen(ScreenOp::Status) => Class::Read,
            ReviewCall::Submit(_) | ReviewCall::Plan(PlanArgs { source: PlanSource::File { .. }, .. }) => Class::Disk,
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

fn name_arg(args: &Json, key: &str) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(n) if !n.trim().is_empty() && n.chars().count() <= 64 && !n.chars().any(char::is_control) => Ok(Some(n.trim().to_owned())),
        Some(_) => Err(format!("'{key}' must be 1..64 characters")),
        None => Ok(None),
    }
}

fn one_of(args: &Json, key: &str, allowed: &[&str], default: Option<&str>) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(s) if allowed.contains(&s) => Ok(Some(s.to_owned())),
        Some(s) => Err(format!("'{key}' must be one of {}, not '{s}'", allowed.join(", "))),
        None => Ok(default.map(str::to_owned)),
    }
}

fn file_arg(args: &Json, key: &str) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(p) if !p.is_empty() && p.chars().count() <= review::MAX_FILE_PATH && !p.chars().any(char::is_control) => Ok(Some(p.to_owned())),
        Some(_) => Err(format!("'{key}' must be a file path of 1..{} characters", review::MAX_FILE_PATH)),
        None => Ok(None),
    }
}

fn ids_arg(args: &Json, key: &str, max: usize) -> Result<Vec<u64>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(v) => {
            let ids: Option<Vec<u64>> = v.as_array().and_then(|a| a.iter().map(|x| x.as_u64().filter(|n| (1..=MAX_ID).contains(n))).collect());
            match ids {
                Some(ids) if ids.len() <= max => Ok(ids),
                _ => Err(format!("'{key}' must be an array of at most {max} ids (integers >= 1)")),
            }
        }
    }
}

fn paths_arg(args: &Json, key: &str, max: usize) -> Result<Vec<String>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(v) => {
            let items = v.as_array().filter(|a| a.len() <= max).ok_or(format!("'{key}' must be an array of at most {max} paths"))?;
            let mut out: Vec<String> = Vec::new();
            for p in items {
                let s = p.as_str().ok_or(format!("'{key}' must hold paths like \"/staging/props\""))?;
                Scope::path(s).map_err(|e| e.replace("'path'", &format!("'{key}'")))?;
                if !out.iter().any(|o| o == s) {
                    out.push(s.to_owned());
                }
            }
            Ok(out)
        }
    }
}

fn path_arg(args: &Json, key: &str) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(p) => Scope::path(p).map(|_| Some(p.to_owned())).map_err(|e| e.replace("'path'", &format!("'{key}'"))),
        None => Ok(None),
    }
}

fn limit_arg(args: &Json, default: u64, max: u64) -> Result<usize, String> {
    Ok(int_arg(args, "limit", 1, max)?.unwrap_or(default) as usize)
}

fn num_arg(args: &Json, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|x| x.is_finite()).map(Some).ok_or(format!("'{key}' must be a number")),
    }
}

fn detailed(args: &Json) -> Result<bool, String> {
    Ok(Format::arg(args, Format::Concise)? == Format::Detailed)
}

fn captures_arg(args: &Json) -> Result<Vec<Capture>, String> {
    let Some(v) = args.get("captures").filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let items = v.as_array().filter(|a| a.len() <= review::MAX_CAPTURES).ok_or(format!("'captures' must be an array of at most {} captures", review::MAX_CAPTURES))?;
    items
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let c = match c {
                Json::Str(_) => Json::obj([("path", c.clone())]),
                other => other.clone(),
            };
            check_keys(&c, &format!("captures[{i}]"), &[&["path", "kind", "packet", "caption"]])?;
            Ok(Capture {
                path: file_arg(&c, "path")?.ok_or(format!("captures[{i}] needs 'path': a PNG the engine wrote (sight_frame, sight_orbit, sight_compare, anim_filmstrip)"))?,
                kind: one_of(&c, "kind", &CAPTURE_KINDS, Some("frame"))?.unwrap_or_default(),
                packet: file_arg(&c, "packet")?,
                caption: text_arg(&c, "caption", 200, false)?.unwrap_or_default(),
                // Only the engine marks a capture as recorded (from `results`), never the caller.
                recorded: None,
            })
        })
        .collect()
}

fn checks_arg(args: &Json) -> Result<Vec<Check>, String> {
    let Some(v) = args.get("checks").filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let items = v.as_array().filter(|a| a.len() <= review::MAX_CHECKS).ok_or(format!("'checks' must be an array of at most {} checks", review::MAX_CHECKS))?;
    items
        .iter()
        .enumerate()
        .map(|(i, c)| {
            check_keys(c, &format!("checks[{i}]"), &[&["kind", "name", "passed", "severity", "metric", "value", "limit", "detail"]])?;
            Ok(Check {
                kind: one_of(c, "kind", &CHECK_KINDS, Some("other"))?.unwrap_or_default(),
                name: text_arg(c, "name", 100, true).map_err(|e| format!("checks[{i}]: {e}"))?.unwrap_or_default(),
                // Reported by the submitter (filled in at submit): never counted as the engine's.
                passed: review::Fact::Claimed(review::Claimed { by: String::new(), value: bool_arg(c, "passed")?.ok_or(format!("checks[{i}] needs 'passed': true or false"))? }),
                severity: one_of(c, "severity", &SEVERITIES, Some("error"))?.unwrap_or_default(),
                metric: text_arg(c, "metric", 100, false)?,
                value: num_arg(c, "value")?,
                limit: num_arg(c, "limit")?,
                detail: text_arg(c, "detail", 500, false)?.unwrap_or_default(),
                recorded: None,
            })
        })
        .collect()
}

/// `results`: references to results the engine recorded (`[{id, epoch}]`, each from a
/// `recorded` in a style_check, studio_check or capture answer), at most
/// [`crate::records::MAX_REFS`], no duplicates.
fn results_arg(args: &Json) -> Result<Vec<review::ResultRef>, String> {
    let Some(v) = args.get("results").filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let max = crate::records::MAX_REFS;
    let items = v.as_array().filter(|a| a.len() <= max).ok_or(format!("'results' must be an array of at most {max} references [{{id, epoch}}]"))?;
    let mut out: Vec<review::ResultRef> = Vec::with_capacity(items.len());
    for (i, r) in items.iter().enumerate() {
        check_keys(r, &format!("results[{i}]"), &[&["id", "epoch"]])?;
        let id = int_arg(r, "id", 1, MAX_ID)?.ok_or(format!("results[{i}] needs 'id': the 'recorded.id' of a style_check, studio_check or capture answer"))?;
        let epoch = match string_arg(r, "epoch")? {
            Some(e) if (1..=crate::records::MAX_EPOCH).contains(&e.len()) && e.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.') => e.to_owned(),
            Some(_) => return Err(format!("results[{i}].epoch must be the 'recorded.epoch' of the answer (1..{} letters, digits and dots)", crate::records::MAX_EPOCH)),
            None => return Err(format!("results[{i}] needs 'epoch': the 'recorded.epoch' of the answer (which engine run recorded it)")),
        };
        if out.iter().any(|o| o.id == id) {
            return Err(format!("results[{i}]: result {id} is listed twice"));
        }
        out.push(review::ResultRef { id, epoch });
    }
    Ok(out)
}

fn promote_arg(args: &Json) -> Result<Option<review::Promote>, String> {
    let Some(p) = args.get("promote").filter(|v| !v.is_null()) else { return Ok(None) };
    check_keys(p, "promote", &[&["to", "replace", "remove_tag"]])?;
    let to = path_arg(p, "to")?.ok_or("'promote' needs 'to': the parent path the work moves under on approval (\"/world/props\")")?;
    let remove_tag = match p.get("remove_tag") {
        None => Some("staging".to_owned()),
        Some(Json::Null) => None,
        Some(Json::Str(s)) if (1..=64).contains(&s.chars().count()) => Some(s.clone()),
        Some(_) => return Err("'promote.remove_tag' must be a tag (1..64 characters) or null".into()),
    };
    Ok(Some(review::Promote { to, replace: bool_arg(p, "replace")?.unwrap_or(false), remove_tag }))
}

fn statuses<T: Copy>(args: &Json, parse: fn(&str) -> Option<T>, names: &[&str]) -> Result<Vec<T>, String> {
    let all = || names.join(", ");
    match args.get("status") {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Str(s)) => parse(s).map(|s| vec![s]).ok_or(format!("'status' must be one of {}", all())),
        Some(Json::Arr(a)) if a.len() <= 8 => a.iter().map(|s| s.as_str().and_then(parse).ok_or(format!("'status' entries must be one of {}", all()))).collect(),
        Some(_) => Err(format!("'status' must be a status or an array of them ({})", all())),
    }
}

fn verdict(s: &str) -> Option<Status> {
    match s {
        "approve" | "approved" => Some(Status::Approved),
        "reject" | "rejected" => Some(Status::Rejected),
        "withdraw" | "withdrawn" => Some(Status::Withdrawn),
        _ => None,
    }
}

fn critique_item(c: &Json, i: usize) -> Result<(CritiqueSpec, Option<String>), String> {
    check_keys(c, &format!("items[{i}]"), &[&["text", "severity", "source", "capture", "packet", "metric", "path", "entity", "submission", "assign"]])?;
    let metric = match c.get("metric").filter(|m| !m.is_null()) {
        None => None,
        Some(m) => {
            check_keys(m, "metric", &[&["name", "value", "limit"]])?;
            Some(Metric { name: text_arg(m, "name", 100, true)?.unwrap_or_default(), value: num_arg(m, "value")?, limit: num_arg(m, "limit")? })
        }
    };
    let entity = int_arg(c, "entity", 1, MAX_ID)?;
    let path = path_arg(c, "path")?;
    if entity.is_some() && path.is_some() {
        return Err(format!("items[{i}]: give 'entity' (an id) or 'path', not both"));
    }
    let target = match (entity, path) {
        (Some(id), _) => Some(Target::Entity { id, path: String::new() }),
        (_, Some(p)) => Some(Target::Path(p)),
        _ => None,
    };
    let spec = CritiqueSpec {
        text: text_arg(c, "text", review::MAX_NOTE, true).map_err(|e| format!("items[{i}]: {e}"))?.unwrap_or_default(),
        severity: one_of(c, "severity", &SEVERITIES, Some("warning"))?.unwrap_or_default(),
        source: name_arg(c, "source")?.unwrap_or_default(),
        capture: file_arg(c, "capture")?,
        packet: file_arg(c, "packet")?,
        metric,
        target,
        submission: int_arg(c, "submission", 1, MAX_ID)?,
        assignee: None,
    };
    Ok((spec, name_arg(c, "assign")?))
}

/// `Some(parsed)` when `name` is one of these tools (`merge_suggest` runs on the caller's thread:
/// see [`merge_suggest`]).
pub fn parse(name: &str, args: &Json) -> Option<Result<ReviewCall, String>> {
    let parsed = match name {
        "review_submit" => (|| {
            check_keys(
                args,
                name,
                &[&["title", "kind", "summary", "batch", "task", "paths", "captures", "checks", "results", "journal", "journal_since", "promote", "supersedes"]],
            )?;
            let a = SubmitArgs {
                kind: one_of(args, "kind", &KINDS, Some("changes"))?.unwrap_or_default(),
                title: text_arg(args, "title", review::MAX_TITLE, true)?.unwrap_or_default(),
                summary: text_arg(args, "summary", review::MAX_SUMMARY, false)?.unwrap_or_default(),
                batch: match string_arg(args, "batch")? {
                    Some(b) if (1..=64).contains(&b.chars().count()) && !b.chars().any(char::is_control) => Some(b.to_owned()),
                    Some(_) => return Err("'batch' must be a label of 1..64 characters".into()),
                    None => None,
                },
                task: int_arg(args, "task", 1, MAX_ID)?,
                paths: paths_arg(args, "paths", review::MAX_PATHS)?,
                captures: captures_arg(args)?,
                checks: checks_arg(args)?,
                results: results_arg(args)?,
                journal: ids_arg(args, "journal", review::MAX_JOURNAL)?,
                journal_since: int_arg(args, "journal_since", 0, MAX_ID)?,
                promote: promote_arg(args)?,
                supersedes: int_arg(args, "supersedes", 1, MAX_ID)?,
            };
            if a.paths.is_empty() && a.journal.is_empty() && a.journal_since.is_none() {
                return Err("review_submit needs what the work covers: 'paths' (entity subtrees), 'journal' (entry ids) or 'journal_since'".into());
            }
            if !a.journal.is_empty() && a.journal_since.is_some() {
                return Err("give 'journal' or 'journal_since', not both".into());
            }
            if a.promote.is_some() && a.paths.is_empty() {
                return Err("'promote' moves the submission's 'paths' on approval: give 'paths' too".into());
            }
            Ok(ReviewCall::Submit(a))
        })(),
        "review_list" => (|| {
            check_keys(args, name, &[&["id", "status", "kind", "batch", "by", "task", "changed_since", "cursor", "limit", "response_format"]])?;
            Ok(ReviewCall::List {
                id: int_arg(args, "id", 1, MAX_ID)?,
                query: SubmissionQuery {
                    statuses: statuses(args, Status::parse, &["pending", "approved", "rejected", "withdrawn"])?,
                    kind: one_of(args, "kind", &KINDS, None)?,
                    batch: string_arg(args, "batch")?.map(str::to_owned),
                    by: name_arg(args, "by")?,
                    task: int_arg(args, "task", 1, MAX_ID)?,
                    changed_since: int_arg(args, "changed_since", 0, MAX_ID)?,
                    after: int_arg(args, "cursor", 0, MAX_ID)?,
                    limit: limit_arg(args, 20, 100)?,
                },
                detailed: detailed(args)?,
            })
        })(),
        "review_decide" => (|| {
            check_keys(args, name, &[&["ids", "batch", "verdict", "note", "revert", "decisions", "promote", "force"]])?;
            let ids = ids_arg(args, "ids", MAX_DECISIONS)?;
            let batch = string_arg(args, "batch")?.map(str::to_owned);
            let decisions = match args.get("decisions").filter(|d| !d.is_null()) {
                None => Vec::new(),
                Some(d) => {
                    let items = d.as_array().filter(|a| !a.is_empty() && a.len() <= MAX_DECISIONS).ok_or(format!("'decisions' must be an array of 1..{MAX_DECISIONS} {{id, verdict, note, revert}}"))?;
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, x)| {
                            check_keys(x, &format!("decisions[{i}]"), &[&["id", "verdict", "note", "revert"]])?;
                            let v = string_arg(x, "verdict")?.and_then(verdict).ok_or(format!("decisions[{i}].verdict must be \"approve\", \"reject\" or \"withdraw\""))?;
                            Ok(DecideOne {
                                id: id_arg(x, "id", "the submission")?,
                                verdict: v,
                                note: text_arg(x, "note", review::MAX_NOTE, false)?.unwrap_or_default(),
                                revert: bool_arg(x, "revert")?.unwrap_or(false),
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?
                }
            };
            let which = match (ids.is_empty(), batch) {
                (false, Some(_)) => return Err("give 'ids' or 'batch', not both".into()),
                (false, None) => Some(Which::Ids(ids)),
                (true, Some(b)) => Some(Which::Batch(b)),
                (true, None) => None,
            };
            let v = match string_arg(args, "verdict")? {
                Some(s) => Some(verdict(s).ok_or("'verdict' must be \"approve\", \"reject\" or \"withdraw\"")?),
                None => None,
            };
            match (&which, v, decisions.is_empty()) {
                (Some(_), Some(_), true) | (None, None, false) => {}
                (Some(_), None, _) => return Err("'ids' / 'batch' need a 'verdict' (approve, reject or withdraw)".into()),
                _ => return Err("review_decide takes 'ids' or 'batch' with one 'verdict' (and 'note'), or 'decisions': [{id, verdict, note}]".into()),
            }
            Ok(ReviewCall::Decide {
                which,
                verdict: v,
                note: text_arg(args, "note", review::MAX_NOTE, false)?.unwrap_or_default(),
                revert: bool_arg(args, "revert")?.unwrap_or(false),
                decisions,
                promote: bool_arg(args, "promote")?.unwrap_or(true),
                force: bool_arg(args, "force")?.unwrap_or(false),
            })
        })(),
        "review_screen" => (|| {
            check_keys(args, name, &[&["action", "id"]])?;
            let id = int_arg(args, "id", 1, MAX_ID)?;
            let op = match string_arg(args, "action")?.unwrap_or("status") {
                "open" => ScreenOp::Open(id),
                "show" => ScreenOp::Open(Some(id.ok_or("action \"show\" needs 'id': the submission to show")?)),
                "close" => ScreenOp::Close,
                "next" => ScreenOp::Next,
                "prev" => ScreenOp::Prev,
                "status" => ScreenOp::Status,
                _ => return Err("'action' must be \"open\", \"show\" (with id), \"next\", \"prev\", \"close\" or \"status\"".into()),
            };
            if id.is_some() && !matches!(op, ScreenOp::Open(_)) {
                return Err("'id' is for action \"open\" or \"show\"".into());
            }
            Ok(ReviewCall::Screen(op))
        })(),
        "critique_add" => (|| {
            check_keys(args, name, &[&["items"]])?;
            let items = args
                .get("items")
                .and_then(Json::as_array)
                .filter(|a| !a.is_empty() && a.len() <= review::MAX_ADD)
                .ok_or(format!("'items' is required: 1..{} findings, each {{text, severity, source, capture?, metric?, entity? | path?, submission?}}", review::MAX_ADD))?;
            Ok(ReviewCall::CritiqueAdd(items.iter().enumerate().map(|(i, c)| critique_item(c, i)).collect::<Result<_, _>>()?))
        })(),
        "critique_list" => (|| {
            check_keys(args, name, &[&["status", "severity", "source", "submission", "path_prefix", "assignee", "changed_since", "cursor", "limit", "response_format"]])?;
            Ok(ReviewCall::CritiqueList(CritiqueQuery {
                statuses: statuses(args, CStatus::parse, &["open", "assigned", "in_task", "resolved", "dismissed"])?,
                severity: one_of(args, "severity", &SEVERITIES, None)?,
                source: name_arg(args, "source")?,
                submission: int_arg(args, "submission", 1, MAX_ID)?,
                path_prefix: path_arg(args, "path_prefix")?,
                assignee: name_arg(args, "assignee")?,
                changed_since: int_arg(args, "changed_since", 0, MAX_ID)?,
                after: int_arg(args, "cursor", 0, MAX_ID)?,
                limit: limit_arg(args, 50, 200)?,
                detailed: detailed(args)?,
            }))
        })(),
        "critique_update" => (|| {
            check_keys(args, name, &[&["id", "ids", "action", "assignee", "note", "expect_version"]])?;
            let mut ids = ids_arg(args, "ids", review::MAX_ADD)?;
            if let Some(id) = int_arg(args, "id", 1, MAX_ID)? {
                if !ids.is_empty() {
                    return Err("give 'id' or 'ids', not both".into());
                }
                ids.push(id);
            }
            if ids.is_empty() {
                return Err("critique_update needs 'id' (or 'ids')".into());
            }
            let note = text_arg(args, "note", review::MAX_NOTE, false)?.unwrap_or_default();
            let assignee = name_arg(args, "assignee")?;
            let action = match string_arg(args, "action")? {
                Some("assign") => CritiqueAction::Assign(assignee.clone().ok_or("action \"assign\" needs 'assignee'")?),
                Some("resolve") => CritiqueAction::Resolve(note.clone()),
                Some("dismiss") if note.trim().is_empty() => return Err("action \"dismiss\" needs a reason: 'note'".into()),
                Some("dismiss") => CritiqueAction::Dismiss(note.clone()),
                Some("reopen") => CritiqueAction::Reopen,
                Some("to_task") => CritiqueAction::ToTask { assign: assignee.clone() },
                _ => return Err("'action' must be \"assign\", \"resolve\", \"dismiss\" (with a 'note'), \"reopen\" or \"to_task\"".into()),
            };
            if assignee.is_some() && !matches!(action, CritiqueAction::Assign(_) | CritiqueAction::ToTask { .. }) {
                return Err("'assignee' is for action \"assign\" or \"to_task\"".into());
            }
            let expect = int_arg(args, "expect_version", 1, MAX_ID)?;
            if expect.is_some() && ids.len() > 1 {
                return Err("'expect_version' is for one critique ('id')".into());
            }
            Ok(ReviewCall::CritiqueUpdate { ids, action, expect })
        })(),
        "task_plan" => (|| {
            check_keys(args, name, &[&["file", "text", "section", "items", "title", "assign", "dry_run", "priority", "parent"]])?;
            let section = text_arg(args, "section", 200, false)?;
            let source = match (file_arg(args, "file")?, string_arg(args, "text")?, args.get("items").filter(|v| !v.is_null())) {
                (Some(file), None, None) => PlanSource::File { file, section: section.ok_or("'file' needs 'section': the heading to plan")? },
                (None, Some(text), None) => {
                    if text.len() > crate::director::MAX_DOC_BYTES {
                        return Err(format!("'text' is at most {} bytes", crate::director::MAX_DOC_BYTES));
                    }
                    PlanSource::Text { text: text.to_owned(), section: section.ok_or("'text' needs 'section': the heading to plan")? }
                }
                (None, None, Some(items)) => {
                    if section.is_some() {
                        return Err("'section' is for 'file' or 'text'".into());
                    }
                    PlanSource::Items(crate::director::parse_items(items)?)
                }
                _ => return Err("task_plan needs one source: 'file' + 'section' (a Markdown design doc in this project), 'text' + 'section', or 'items'".into()),
            };
            Ok(ReviewCall::Plan(PlanArgs {
                source,
                title: text_arg(args, "title", crate::board::MAX_TITLE, false)?.filter(|t| !t.trim().is_empty()),
                by_role: match string_arg(args, "assign")?.unwrap_or("none") {
                    "by_role" => true,
                    "none" => false,
                    _ => return Err("'assign' must be \"by_role\" or \"none\"".into()),
                },
                dry_run: bool_arg(args, "dry_run")?.unwrap_or(false),
                priority: int_arg(args, "priority", 0, 9)?.map(|p| p as u8),
                parent: int_arg(args, "parent", 1, MAX_ID)?,
            }))
        })(),
        "task_progress" => (|| {
            check_keys(args, name, &[&["plan", "response_format"]])?;
            Ok(ReviewCall::Progress { plan: id_arg(args, "plan", "the plan's task id (task_plan's answer)")?, detailed: detailed(args)? })
        })(),
        _ => return None,
    };
    Some(parsed)
}

/// `merge_suggest`: pure, so it runs on the calling agent's connection.
pub fn merge_suggest(args: &Json) -> ToolOutput {
    let run = || -> Result<Json, String> {
        check_keys(args, "merge_suggest", &[&["kind", "base", "theirs", "yours", "text", "prefer"]])?;
        let k = string_arg(args, "kind")?.unwrap_or("json");
        let kind = MergeKind::parse(k).ok_or(format!("'kind' must be one of {}", MergeKind::NAMES.join(", ")))?;
        let prefer = match string_arg(args, "prefer")?.unwrap_or("theirs") {
            "theirs" => Prefer::Theirs,
            "yours" => Prefer::Yours,
            _ => return Err("'prefer' must be \"theirs\" or \"yours\"".into()),
        };
        let text = bool_arg(args, "text")?.unwrap_or(false);
        let side = |k: &str| -> Result<Option<Json>, String> {
            let Some(v) = args.get(k) else { return Ok(None) };
            let size = v.to_string().len();
            if size > merge::MAX_SIDE_BYTES {
                return Err(format!("'{k}' is {size} bytes; at most {} per side", merge::MAX_SIDE_BYTES));
            }
            if !text {
                return Ok(Some(v.clone()));
            }
            match v {
                Json::Null => Ok(None),
                Json::Str(s) => Json::parse(s).map(Some).map_err(|e| format!("'{k}' is not JSON text: {e}")),
                _ => Err(format!("with text: true, '{k}' must be the document's text (a string) or null when absent")),
            }
        };
        let (b, t, y) = (side("base")?, side("theirs")?, side("yours")?);
        if t.is_none() || y.is_none() {
            return Err("'theirs' and 'yours' are required (base may be left out when the data didn't exist)".into());
        }
        let m = merge::merge3(b.as_ref(), t.as_ref(), y.as_ref(), kind, prefer);
        let mut j = m.to_json();
        if text {
            // The document's text instead of its value (not both: answers stay bounded).
            if let Json::Obj(pairs) = &mut j {
                pairs.retain(|(k, _)| k != "merged");
            }
            j.set("merged_text", m.merged.as_ref().map_or(Json::Null, |v| Json::from(v.to_pretty())));
        }
        j.set("kind", Json::from(kind.name()));
        j.set(
            "next",
            Json::from(if m.clean() {
                "clean: write 'merged' with the version you read theirs at (expect_version)"
            } else {
                "conflicts keep theirs in 'merged' (prefer: \"yours\" keeps yours): decide each, then write it with the version you read theirs at"
            }),
        );
        Ok(j)
    };
    match run() {
        Ok(j) => ToolOutput::text(j.to_string()),
        Err(e) => ToolOutput::error(crate::tools::argument_error(&e).to_string()),
    }
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
    let objs = |d: &str| Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("object"))])), ("description", Json::from(d))]);
    let any = |d: &str| Json::obj([("description", Json::from(d))]);
    let fmt = || Format::schema();
    let page = || vec![("cursor", t("integer", "next_cursor from the previous page")), ("limit", t("integer", "Page size"))];
    let def = |name: &str, description: &str, input_schema: Json| ToolDef { name: name.into(), description: description.into(), input_schema };
    vec![
        def(
            "review_submit",
            "Put finished work on the owner's review queue with its evidence. Covers entity subtrees (paths; recorded with their \
             versions, so later edits make it stale), journal entries (journal or journal_since: your own entries after an id) \
             and a task (default: your current one). captures: PNGs the engine wrote (sight_frame/sight_orbit/sight_compare/\
             anim_filmstrip; first one shows on the review screen). checks: results as data from anim_check, physics_check, style \
             or taste checks; what you type there is shown as reported by you, never as the engine's. results: results the \
             engine recorded itself, by the 'recorded' {id, epoch} a style_check, studio_check or capture answer gave you (yours, \
             this engine run, the recipe unchanged since): they show as engine results. promote {to}: approval moves the paths \
             under 'to' (journaled, claim-checked). supersedes: replaces your earlier submission. Needs agent_hello. Saved with \
             the board.",
            schema(
                vec![
                    ("title", t("string", "1..200 characters")),
                    ("kind", t("string", "asset, lex, scene, ui or changes (default)")),
                    ("summary", t("string", "What it is and how it was checked, up to 2000 characters")),
                    ("batch", t("string", "A label to decide many together (review_decide {batch})")),
                    ("task", t("integer", "The task it is for (default: your current task)")),
                    ("paths", arr("string", "Entity subtrees it covers (\"/staging/crypt_props\"), at most 64")),
                    ("captures", objs("[{path, kind: contact_sheet|before_after|frame|filmstrip|orbit|other, packet?, caption?}], at most 8")),
                    ("checks", objs("[{kind: style|taste|motion|physics|perf|ui|other, name, passed, severity: error|warning|note, metric?, value?, limit?, detail?}], at most 64 (reported by you)")),
                    ("results", objs("[{id, epoch}] from the 'recorded' of your style_check / studio_check answers and your captures' image.recorded, at most 16: engine-recorded checks and captures; a check must judge models under paths (refused: result_not_found, stale_result, foreign_result, unrelated_result, result_unverifiable, result_not_verified)")),
                    ("journal", arr("integer", "Journal ids it covers, at most 256")),
                    ("journal_since", t("integer", "Instead: your own journal entries after this id (newest 256)")),
                    ("promote", Json::obj([("type", Json::from("object")), ("description", Json::from("{to: parent path, replace?: delete a same-named entity there, remove_tag?: default \"staging\", null for none}"))])),
                    ("supersedes", t("integer", "Your earlier submission this replaces")),
                ],
                &["title"],
            ),
        ),
        def(
            "review_list",
            "The review queue, one page in id order: counts per status, pending batches, and per submission its status, kind, \
             title, submitter, checks (failed names), first capture, paths, decision. id: one submission in full (captures, every \
             check, roots, journal, decision, its critiques). Poll with changed_since: review_seq.",
            schema(
                [
                    vec![
                        ("id", t("integer", "One submission in full")),
                        ("status", t("string", "pending, approved, rejected or withdrawn (or an array)")),
                        ("kind", t("string", "asset, lex, scene, ui or changes")),
                        ("batch", t("string", "Batch label")),
                        ("by", t("string", "Submitter's name")),
                        ("task", t("integer", "Submissions for this task")),
                        ("changed_since", t("integer", "Only those changed after this review_seq")),
                        ("response_format", fmt()),
                    ],
                    page(),
                ]
                .concat(),
                &[],
            ),
        ),
        def(
            "review_decide",
            "Decide submissions in a batch (role owner, reviewer, lead or director; never your own). ids or batch with one \
             verdict (approve, reject, withdraw: your own only) and note, or decisions: [{id, verdict, note, revert}]. Approve: a \
             submission with promote moves its paths under promote.to in one journaled batch as you (claims apply; promote: false \
             records the verdict only). Reject: note required; posts a rework task assigned to the submitter (revert: true also \
             undoes its journal entries, refused for refs from an earlier run or world_load: journal_from_earlier_run). Refused per submission (others go on): stale_submission when its paths changed since it \
             was submitted (force: true to decide anyway), already_decided, self_review.",
            schema(
                vec![
                    ("ids", arr("integer", "Submissions, at most 64")),
                    ("batch", t("string", "Every pending submission of this batch")),
                    ("verdict", t("string", "approve, reject or withdraw")),
                    ("note", t("string", "Why (required to reject)")),
                    ("revert", t("boolean", "With reject: undo the submission's journal entries")),
                    ("decisions", objs("[{id, verdict, note?, revert?}] instead of ids + verdict")),
                    ("promote", t("boolean", "Move approved work as promote says (default true)")),
                    ("force", t("boolean", "Decide even if the work changed since it was submitted")),
                ],
                &[],
            ),
        ),
        def(
            "review_screen",
            "The window's review screen, where the owner pages through pending submissions and approves or rejects by mouse, \
             touch, keys (F9) or gamepad (LB + RB + Y). action: open (optionally at id), show (id), next, prev, close, status \
             (default: what it shows). Captures with include_ui show it.",
            schema(vec![("action", t("string", "open, show, next, prev, close or status")), ("id", t("integer", "Submission to show"))], &[]),
        ),
        def(
            "critique_add",
            "Record review findings as items to work through (sight-skeptic, lane reviewers, playtests, the owner): items: [{text, \
             severity: error|warning|note, source (who found it), capture?, packet?, metric? {name, value, limit}, entity? (id) | \
             path?, submission?, assign?}], at most 32. Returns their ids. Saved with the board.",
            schema(vec![("items", objs("Findings, 1..32"))], &["items"]),
        ),
        def(
            "critique_list",
            "Critiques, one page in id order, with counts per status. Filters: status (open, assigned, in_task, resolved, \
             dismissed), severity, source, submission, path_prefix, assignee, changed_since (review_seq). detailed: full text, \
             packet and resolution.",
            schema(
                [
                    vec![
                        ("status", t("string", "A status or an array")),
                        ("severity", t("string", "error, warning or note")),
                        ("source", t("string", "Who found them")),
                        ("submission", t("integer", "On this submission")),
                        ("path_prefix", t("string", "On entities or paths at or below this")),
                        ("assignee", t("string", "Assigned to this agent")),
                        ("changed_since", t("integer", "Only those changed after this review_seq")),
                        ("response_format", fmt()),
                    ],
                    page(),
                ]
                .concat(),
                &[],
            ),
        ),
        def(
            "critique_update",
            "Work a critique through: action assign (assignee), resolve (note), dismiss (note: the reason, required), reopen, or \
             to_task (posts a board task for it, optionally assigned; the critique resolves itself when that task is done and opens \
             again if it is cancelled). id, or ids (at most 32).",
            schema(
                vec![
                    ("id", t("integer", "Critique id")),
                    ("ids", arr("integer", "Several critiques")),
                    ("action", t("string", "assign, resolve, dismiss, reopen or to_task")),
                    ("assignee", t("string", "Agent (assign, to_task)")),
                    ("note", t("string", "Resolution or dismissal reason")),
                    ("expect_version", t("integer", "Refuse if it changed since you read it (one id)")),
                ],
                &[],
            ),
        ),
        def(
            "merge_suggest",
            "A suggested three-way merge of JSON data (Lex items, recipes, UI screens, style and project files) from the version \
             you both started from (base), theirs (now) and yours. Objects merge by key; lists of objects by id or name; tags as \
             sets; other lists by position when the lengths match. Returns merged, clean, and conflicts [{path, base, theirs, \
             yours}] (merged keeps theirs there unless prefer: \"yours\"). text: true when the three are documents' text. \
             world_set and lex_edit refusals already carry conflict.merge.",
            schema(
                vec![
                    ("kind", t("string", "json (default), lex, recipe, ui or props")),
                    ("base", any("The common original (leave out if it didn't exist)")),
                    ("theirs", any("Their version now")),
                    ("yours", any("Your version")),
                    ("text", t("boolean", "base/theirs/yours are JSON documents as strings")),
                    ("prefer", t("string", "Which side conflicts keep: theirs (default) or yours")),
                ],
                &["theirs", "yours"],
            ),
        ),
        def(
            "task_plan",
            "For the lead agent: turn a design-doc section into board tasks under one plan task. file + section: a Markdown \
             file in this project; its unchecked '- [ ]' items become tasks (nested ones subtasks, indented lines their detail, \
             #role words their role, '(after: words)' a dependency on the earlier item starting with those words). Or items: \
             [{key, title, detail?, role?, after?: [keys], parent?: key}]. assign: \"by_role\" gives each task with a role to the \
             connected agents of that agent_hello role, round-robin by name. Tasks are tagged role:<role> and plan:<id>. dry_run \
             shows the plan without posting. Track it with task_progress.",
            schema(
                vec![
                    ("file", t("string", "A .md file under the project (docs/ROADMAP.md, games/sepulcra/DESIGN.md)")),
                    ("text", t("string", "Or the Markdown itself")),
                    ("section", t("string", "The heading to plan (matched without case)")),
                    ("items", objs("Or the plan as data")),
                    ("title", t("string", "The plan task's title (default: Plan: <heading>)")),
                    ("assign", t("string", "\"by_role\" or \"none\" (default)")),
                    ("dry_run", t("boolean", "Show the plan, post nothing")),
                    ("priority", t("integer", "0..9 for every task")),
                    ("parent", t("integer", "A task the plan goes under")),
                ],
                &[],
            ),
        ),
        def(
            "task_progress",
            "A plan's dashboard for its lead: tasks by status and by role, percent done, blocked tasks with blockers, open \
             questions, and the review checkpoint (its submissions pending, approved, rejected; open critiques), with the next \
             step and complete: true when every task and submission is settled.",
            schema(vec![("plan", t("integer", "The plan task (task_plan's answer)")), ("response_format", fmt())], &["plan"]),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, a: &str) -> Result<ReviewCall, String> {
        parse(name, &Json::parse(a).unwrap()).expect("one of ours")
    }

    #[test]
    fn submit_arguments_are_checked() {
        let ReviewCall::Submit(a) = call(
            "review_submit",
            r#"{"title": "props", "kind": "asset", "paths": ["/staging/a", "/staging/a"], "promote": {"to": "/world"}, "captures": ["c.png", {"path": "d.png", "kind": "contact_sheet"}], "checks": [{"kind": "style", "name": "budget", "passed": true}]}"#,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(a.paths, ["/staging/a"], "duplicates dropped");
        assert_eq!(a.promote.unwrap().remove_tag.as_deref(), Some("staging"));
        assert_eq!((a.captures[0].kind.as_str(), a.captures[1].kind.as_str()), ("frame", "contact_sheet"));
        assert_eq!(a.checks[0].severity, "error");
        assert!(call("review_submit", r#"{"title": "x"}"#).unwrap_err().contains("covers"));
        assert!(call("review_submit", r#"{"title": "x", "journal": [1], "journal_since": 3}"#).is_err());
        assert!(call("review_submit", r#"{"title": "x", "journal": [1], "promote": {"to": "/w"}}"#).unwrap_err().contains("paths"));
        assert!(call("review_submit", r#"{"title": "x", "paths": ["/a"], "kind": "mesh"}"#).unwrap_err().contains("asset"));
        assert!(call("review_submit", r#"{"title": "x", "paths": ["/a"], "checks": [{"name": "n"}]}"#).unwrap_err().contains("passed"));
        assert!(call("review_submit", r#"{"title": "x", "paths": ["/a"], "titel": 1}"#).unwrap_err().contains("did you mean"));
    }

    #[test]
    fn decide_and_critique_arguments_are_checked() {
        let ReviewCall::Decide { which, verdict, .. } = call("review_decide", r#"{"batch": "crypt", "verdict": "approve"}"#).unwrap() else { panic!() };
        assert_eq!((which, verdict), (Some(Which::Batch("crypt".into())), Some(Status::Approved)));
        assert!(call("review_decide", r#"{"ids": [1]}"#).unwrap_err().contains("verdict"));
        assert!(call("review_decide", r#"{"ids": [1], "batch": "b", "verdict": "approve"}"#).is_err());
        assert!(call("review_decide", r#"{"decisions": [{"id": 1, "verdict": "maybe"}]}"#).is_err());
        assert!(call("review_decide", r#"{"decisions": [{"id": 1, "verdict": "reject", "note": "no"}]}"#).is_ok());
        assert!(call("review_decide", "{}").is_err());
        assert!(matches!(call("review_screen", r#"{"action": "show", "id": 3}"#).unwrap(), ReviewCall::Screen(ScreenOp::Open(Some(3)))));
        assert!(call("review_screen", r#"{"action": "show"}"#).is_err());
        let ReviewCall::CritiqueAdd(items) = call("critique_add", r#"{"items": [{"text": "flat light", "source": "sight-skeptic", "metric": {"name": "contrast", "value": 0.1, "limit": 0.3}, "path": "/a"}]}"#).unwrap() else {
            panic!()
        };
        assert_eq!(items[0].0.severity, "warning");
        assert!(call("critique_add", r#"{"items": [{"text": "x", "entity": 1, "path": "/a"}]}"#).is_err());
        assert!(call("critique_add", r#"{"items": []}"#).is_err());
        assert!(call("critique_update", r#"{"id": 1, "action": "dismiss"}"#).unwrap_err().contains("reason"));
        assert!(call("critique_update", r#"{"ids": [1, 2], "action": "resolve", "expect_version": 2}"#).is_err());
        assert!(matches!(call("critique_update", r#"{"id": 1, "action": "to_task", "assignee": "ann"}"#).unwrap(), ReviewCall::CritiqueUpdate { action: CritiqueAction::ToTask { .. }, .. }));
        assert!(call("task_plan", r#"{"file": "docs/ROADMAP.md"}"#).unwrap_err().contains("section"));
        assert!(call("task_plan", r#"{"file": "a.md", "section": "x", "items": []}"#).is_err());
        assert!(call("task_plan", r#"{"items": [{"key": "a", "title": "A"}], "assign": "by_role"}"#).is_ok());
    }

    #[test]
    fn merge_suggest_answers_with_the_merge() {
        let out = merge_suggest(&Json::parse(r#"{"kind": "lex", "base": {"hp": 1, "sp": 1}, "theirs": {"hp": 2, "sp": 1}, "yours": {"hp": 1, "sp": 3}}"#).unwrap());
        assert!(!out.is_error);
        let j = crate::lex_tools::output_json(&out);
        assert_eq!(j.get("clean"), Some(&Json::Bool(true)));
        assert!(merge::same(j.get("merged").unwrap(), &Json::parse(r#"{"hp": 2, "sp": 3}"#).unwrap()));
        let out = merge_suggest(&Json::parse(r#"{"text": true, "base": "{\"a\": 1}", "theirs": "{\"a\": 2}", "yours": "{\"a\": 3}"}"#).unwrap());
        let j = crate::lex_tools::output_json(&out);
        assert_eq!(j.get("clean"), Some(&Json::Bool(false)));
        assert!(j.get("merged_text").and_then(Json::as_str).is_some());
        assert!(merge_suggest(&Json::parse(r#"{"theirs": 1}"#).unwrap()).is_error);
        assert!(merge_suggest(&Json::parse(r#"{"text": true, "theirs": "{", "yours": "{}"}"#).unwrap()).is_error);
    }

    #[test]
    fn definitions_are_well_formed_and_every_tool_parses() {
        for d in definitions() {
            assert!(d.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && d.name.len() <= 64);
            assert!(d.input_schema.get("properties").is_some());
            assert!(!d.description.contains("  "), "{}: runs of spaces", d.name);
            if d.name != "merge_suggest" {
                assert!(parse(&d.name, &Json::Null).is_some(), "{} has no parser", d.name);
            }
        }
    }
}
