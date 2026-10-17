//! The `lex_*` tools: gameplay rules as Lex data (`specula-lex`, docs/design/LEX.md).
//!
//! **Storage.** A rule set lives in the world as entities: a group `/lex/<set>` holding the
//! document header in its props (`{"lex_set": {...}}`), and one hidden child group per item,
//! `/lex/<set>/<item id>`, holding the item (`{"lex": {...}}`). So every item has a version
//! (`expect_version`), path claims cover a set or one item, edits are journaled with their
//! author and undone with `world_undo`, `world_changes` reports them, and `world_save` writes
//! them with the world's text files.
//!
//! **Threads.** The main thread only reads (snapshots, pages of summaries) and writes (one
//! atomic batch through the swarm's claims and journal). Validation, edits' before/after
//! checks and simulation run on the calling agent's protocol thread: an edit validates against
//! a snapshot, and its commit is refused (and redone) if the set changed meanwhile. So many
//! agents can edit and simulate at once without stalling the window.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::journal::{Kind, Writer};
use crate::swarm::Swarm;
use crate::tools::{check_keys, Command, Tools};
use crate::world::{EntityKey, Op, Shape, SpawnSpec, UpdateSpec, World, WorldError};
use specula_core::Json;
use specula_lex::diag::{did_you_mean, Diagnostic};
use specula_lex::run::{compile_query, run, show_val, RunSpec, Timed};
use specula_lex::{compile, schema, Document, Natives, Options, Sim};
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};

/// The top-level group that holds every rule set.
pub const ROOT: &str = "lex";
/// Most items `lex_get` returns at once.
const MAX_GET: usize = 50;
/// `lex_list` page sizes (default, most).
const LIST_LIMITS: (u64, u64) = (100, 500);
/// Most ops per `lex_edit`.
const MAX_OPS: usize = 500;
/// Most ticks one `lex_simulate` call runs.
pub const MAX_SIM_TICKS: u64 = 216_000;
/// Default ticks for `lex_simulate`.
const DEFAULT_SIM_TICKS: u64 = 36_000;
/// Wall time one `lex_simulate` call may take (it reports where it stopped).
const SIM_WALL_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);
/// Most commands per `lex_simulate`.
const MAX_SIM_COMMANDS: usize = 5_000;
/// Most watch / expect expressions.
const MAX_QUERIES: usize = 32;
/// Saved simulation sessions kept (oldest dropped first).
const MAX_RUNS: usize = 16;
/// Largest exported document.
const MAX_EXPORT_BYTES: usize = 2 * 1024 * 1024;

/// What the main thread does for a lex tool. Only cheap reads and the final write happen
/// there; validation and simulation run on the calling agent's protocol thread.
pub enum LexCall {
    /// A set's header, versions and items (`only`: just these items).
    Snapshot { set: String, only: Option<Vec<String>> },
    /// The rule sets in the world, by name, paged.
    Sets { after: Option<String>, limit: usize },
    /// One page of a set's item summaries.
    List { set: String, kind: Option<String>, after: Option<String>, limit: usize },
    /// Writes a validated edit, unless the set changed since it was validated.
    Commit(Commit),
}

impl LexCall {
    /// Everything but a commit only reads.
    pub fn is_read(&self) -> bool {
        !matches!(self, LexCall::Commit(_))
    }
}

pub struct Edit {
    pub set: String,
    pub header: Header,
    pub ops: Vec<EditOp>,
    /// Store the result even if the edit adds validation errors (a draft).
    pub allow_errors: bool,
    pub dry_run: bool,
    /// Every item not put by this edit is deleted (`lex_import`).
    pub replace: bool,
    /// For messages: `lex_edit` or `lex_import`.
    pub tool: &'static str,
}

#[derive(Default)]
pub struct Header {
    pub name: Option<String>,
    pub tick_hz: Option<u32>,
    pub notes: Option<Option<String>>,
}

pub enum EditOp {
    Put { id: String, item: Json, expect: Option<u64> },
    Patch { id: String, set: Vec<(String, Json)>, remove: Vec<String>, expect: Option<u64> },
    Delete { id: String, expect: Option<u64> },
}

impl EditOp {
    fn id(&self) -> &str {
        match self {
            EditOp::Put { id, .. } | EditOp::Patch { id, .. } | EditOp::Delete { id, .. } => id,
        }
    }

    fn expect(&self) -> Option<u64> {
        match self {
            EditOp::Put { expect, .. } | EditOp::Patch { expect, .. } | EditOp::Delete { expect, .. } => *expect,
        }
    }
}

/// A set's version and its items' versions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Versions {
    pub set: u64,
    /// Item name → (entity id, version): the id catches an item deleted and put again (a new
    /// entity that restarts at version 1).
    pub items: BTreeMap<String, (u64, u64)>,
}

/// A validated edit for the main thread to write.
pub struct Commit {
    pub set: String,
    /// The versions the edit was validated against (`None`: the set didn't exist). The commit is
    /// refused with `set_changed` if they no longer match, and the caller validates again.
    pub base: Option<Versions>,
    /// The new header, when it changed (always for a new set).
    pub header: Option<Json>,
    /// New or changed items (`true`: new).
    pub puts: Vec<(String, Json, bool)>,
    pub deletes: Vec<String>,
}

/// Per-set edit locks: edits through the tools queue per set instead of racing (and failing
/// with `set_changed`); the main thread never waits on them. Writes that bypass `lex_edit`
/// (`world_set`, `world_undo`) are still caught by the commit's version check.
#[derive(Default)]
pub struct Locks(Mutex<BTreeMap<String, Arc<Mutex<()>>>>);

/// How long an edit waits for its set's lock before giving up with `busy` (nothing applied),
/// well inside an agent's call timeout, so a queued edit never lands after its caller gave up.
const EDIT_LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Runs `f` holding the set's edit lock, or answers `busy` when the queue is too long.
fn locked(tools: &Tools, set: &str, f: impl FnOnce() -> ToolOutput) -> Result<ToolOutput, ToolOutput> {
    let lock = tools.lex_locks.for_set(set);
    let deadline = Instant::now() + EDIT_LOCK_WAIT;
    loop {
        match lock.try_lock() {
            Ok(_held) => return Ok(f()),
            Err(std::sync::TryLockError::Poisoned(p)) => {
                let _held = p.into_inner();
                return Ok(f());
            }
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(5)),
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(err("busy", format!("other agents' edits to set '{set}' kept it busy for {} s; nothing was applied, so try again (or claim_take /lex/{set} to work alone)", EDIT_LOCK_WAIT.as_secs())));
            }
        }
    }
}

impl Locks {
    fn for_set(&self, set: &str) -> Arc<Mutex<()>> {
        let mut m = self.0.lock().unwrap_or_else(|p| p.into_inner());
        Arc::clone(m.entry(set.to_owned()).or_default())
    }
}

/// Saved simulation sessions (`lex_simulate {save_as}`), shared by all agents.
#[derive(Default)]
pub struct Runs(Mutex<VecDeque<SavedRun>>);

struct SavedRun {
    name: String,
    /// The saving client's name (for lists) and session (only it may overwrite the run).
    owner: String,
    session: String,
    set: String,
    sim: Sim,
}

// ---- main thread ----

fn err(code: &'static str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_pretty())
}

fn err_with(code: &'static str, message: impl Into<String>, extra: Vec<(&str, Json)>) -> ToolOutput {
    let mut pairs = vec![("error", Json::from(code)), ("message", Json::from(message.into()))];
    pairs.extend(extra);
    ToolOutput::error(Json::obj(pairs).to_pretty())
}

fn set_path(set: &str) -> String {
    format!("/{ROOT}/{set}")
}

fn set_entity(world: &World, set: &str) -> Option<u64> {
    let id = world.find_path(&set_path(set)).ok()?;
    world.get(id).filter(|e| e.props.get("lex_set").is_some()).map(|e| e.id)
}

/// The set's versions, without copying any item.
fn versions(world: &World, set: &str) -> Option<Versions> {
    let id = set_entity(world, set)?;
    let items = world.children_of(Some(id)).filter_map(|c| world.get(c)).map(|e| (e.name.clone(), (e.id, e.version))).collect();
    Some(Versions { set: world.get(id)?.version, items })
}

fn snapshot_json(world: &World, set: &str, only: Option<&[String]>) -> Json {
    let Some(id) = set_entity(world, set) else {
        return Json::obj([("set", Json::from(set)), ("exists", Json::Bool(false))]);
    };
    let e = world.get(id).expect("found");
    let header = e.props.get("lex_set").cloned().unwrap_or_else(|| Json::obj::<&str>([]));
    let mut items = Vec::new();
    for c in world.children_of(Some(id)) {
        let Some(ch) = world.get(c) else { continue };
        if only.is_some_and(|o| !o.contains(&ch.name)) {
            continue;
        }
        let item = ch.props.get("lex").cloned().unwrap_or(Json::Null);
        items.push(Json::obj([("id", Json::from(ch.name.as_str())), ("version", Json::from(ch.version)), ("eid", Json::from(ch.id)), ("item", item)]));
    }
    let mut out = vec![("set", Json::from(set)), ("exists", Json::Bool(true)), ("header", header), ("set_version", Json::from(e.version)), ("items", Json::Arr(items))];
    if only.is_some() {
        let all: Vec<Json> = world.children_of(Some(id)).filter_map(|c| world.get(c)).map(|c| Json::from(c.name.as_str())).collect();
        if all.len() <= 1000 {
            out.push(("all_ids", Json::Arr(all)));
        }
    }
    Json::obj(out)
}

fn sets_json(world: &World, after: Option<&str>, limit: usize) -> Json {
    let Ok(root) = world.find_path(&format!("/{ROOT}")) else {
        return Json::obj([("sets", Json::arr([])), ("next_cursor", Json::Null)]);
    };
    let mut page = Vec::new();
    let mut next = Json::Null;
    for e in world.children_of(Some(root)).filter_map(|c| world.get(c)).filter(|e| e.props.get("lex_set").is_some() && after.is_none_or(|a| e.name.as_str() > a)) {
        if page.len() == limit {
            next = page.last().and_then(|p: &Json| p.get("set")).cloned().unwrap_or(Json::Null);
            break;
        }
        page.push(Json::obj([("set", Json::from(e.name.as_str())), ("items", Json::from(world.children_of(Some(e.id)).count())), ("version", Json::from(e.version))]));
    }
    Json::obj([("sets", Json::Arr(page)), ("next_cursor", next)])
}

fn list_json(world: &World, set: &str, kind: Option<&str>, after: Option<&str>, limit: usize) -> ToolOutput {
    let Some(id) = set_entity(world, set) else {
        return err("unknown_set", format!("there is no rule set '{set}'; lex_list without 'set' lists them"));
    };
    let e = world.get(id).expect("found");
    let mut page = Vec::new();
    let mut next = Json::Null;
    let mut total = 0usize;
    for ch in world.children_of(Some(id)).filter_map(|c| world.get(c)) {
        total += 1;
        if after.is_some_and(|a| ch.name.as_str() <= a) {
            continue;
        }
        let item = ch.props.get("lex").unwrap_or(&Json::Null);
        if kind.is_some_and(|k| item.get("kind").and_then(Json::as_str) != Some(k)) {
            continue;
        }
        if page.len() == limit {
            if next.is_null() {
                next = page.last().and_then(|p: &Json| p.get("id")).cloned().unwrap_or(Json::Null);
            }
            continue;
        }
        let mut s = summary_of(&ch.name, item);
        s.set("version", Json::from(ch.version));
        page.push(s);
    }
    ToolOutput::text(
        Json::obj([
            ("set", Json::from(set)),
            ("header", e.props.get("lex_set").cloned().unwrap_or(Json::Null)),
            ("total", Json::from(total)),
            ("items", Json::Arr(page)),
            ("next_cursor", next),
        ])
        .to_string(),
    )
}

/// Answers a lex call on the main thread.
pub fn handle(call: LexCall, world: &mut World, swarm: &mut Swarm, tick: u64, w: Writer, by: &str) -> ToolOutput {
    match call {
        LexCall::Snapshot { set, only } => ToolOutput::text(snapshot_json(world, &set, only.as_deref()).to_string()),
        LexCall::Sets { after, limit } => ToolOutput::text(sets_json(world, after.as_deref(), limit).to_string()),
        LexCall::List { set, kind, after, limit } => list_json(world, &set, kind.as_deref(), after.as_deref(), limit),
        LexCall::Commit(c) => commit(c, world, swarm, tick, w, by),
    }
}

fn commit(c: Commit, world: &mut World, swarm: &mut Swarm, tick: u64, w: Writer, by: &str) -> ToolOutput {
    let now = versions(world, &c.set);
    if now != c.base {
        return err("set_changed", "the set changed while the edit was being validated");
    }
    let root_path = format!("/{ROOT}");
    let sp = set_path(&c.set);
    let mut ops = Vec::new();
    if world.find_path(&root_path).is_err() {
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some(ROOT.into());
        s.visible = false;
        s.tags = BTreeSet::from(["lex".to_owned()]);
        ops.push(Op::Spawn(s));
    }
    match (&c.base, &c.header) {
        (None, header) => {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(c.set.clone());
            s.parent = Some(EntityKey::Path(root_path.clone()));
            s.visible = false;
            s.tags = BTreeSet::from(["lex".to_owned(), "lex_set".to_owned()]);
            s.props = Json::obj([("lex_set", header.clone().unwrap_or_else(|| Json::obj::<&str>([])))]);
            ops.push(Op::Spawn(s));
        }
        (Some(b), Some(h)) => ops.push(Op::Set(
            EntityKey::Path(sp.clone()),
            UpdateSpec { props: Some(Json::obj([("lex_set", h.clone())])), expect_version: Some(b.set), ..Default::default() },
        )),
        (Some(_), None) => {}
    }
    let base_version = |id: &str| c.base.as_ref().and_then(|b| b.items.get(id)).map(|(_, v)| *v);
    for (id, item, new) in &c.puts {
        if *new {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(id.clone());
            s.parent = Some(EntityKey::Path(sp.clone()));
            s.visible = false;
            s.tags = BTreeSet::from(["lex".to_owned()]);
            s.props = Json::obj([("lex", item.clone())]);
            ops.push(Op::Spawn(s));
        } else {
            let u = UpdateSpec { props: Some(Json::obj([("lex", item.clone())])), expect_version: base_version(id), ..Default::default() };
            ops.push(Op::Set(EntityKey::Path(format!("{sp}/{id}")), u));
        }
    }
    for id in &c.deletes {
        ops.push(Op::Delete(EntityKey::Path(format!("{sp}/{id}")), base_version(id)));
    }
    let apply = |wd: &mut World| wd.apply_batch(ops, by).map_err(|(i, e): (usize, WorldError)| e.with("op_index", i));
    match swarm.commit(world, tick, w, Kind::Batch, apply) {
        Ok((_, seq)) => {
            let v = versions(world, &c.set).map(|v| v.items).unwrap_or_default();
            let touched: Vec<(String, Json)> = c.puts.iter().map(|(id, _, _)| (id.clone(), Json::from(v.get(id).map(|(_, ver)| *ver)))).collect();
            ToolOutput::text(Json::obj([("journal", Json::from(seq)), ("versions", Json::Obj(touched))]).to_string())
        }
        Err(we) => {
            let mut j = we.to_json();
            j.set("hint", Json::from("claims: claim_take the set (/lex/<set>) or the item path, or ask its owner to claim_share; versions: lex_get the item and pass its version"));
            ToolOutput::error(j.to_pretty())
        }
    }
}

/// The header as stored, completed with defaults.
fn header_of(stored: Option<&Json>, set: &str) -> Json {
    let mut h = Json::obj([("lex", Json::from(specula_lex::FORMAT_VERSION)), ("name", Json::from(set)), ("tick_hz", Json::from(specula_lex::doc::DEFAULT_TICK_HZ))]);
    if let Some(Json::Obj(pairs)) = stored {
        for (k, v) in pairs {
            h.set(k.clone(), v.clone());
        }
    }
    h
}

/// A stored (or edited) set as a document. Problems with the header or with item ids become
/// diagnostics instead of failures, so a damaged set can still be listed, validated and fixed.
struct Loaded {
    doc: Document,
    problems: Vec<Diagnostic>,
}

fn load(header: &Json, items: &BTreeMap<String, Json>, set: &str) -> Loaded {
    let mut problems = Vec::new();
    let mut h = header.clone();
    h.set("items", Json::obj::<&str>([]));
    let mut doc = match Document::from_json(&h) {
        Ok(d) => d,
        Err(ds) => {
            problems.extend(ds.into_iter().map(|mut d| {
                d.message = format!("the set's header: {}", d.message);
                d
            }));
            Document::new(set, specula_lex::doc::DEFAULT_TICK_HZ)
        }
    };
    for (id, item) in items {
        match specula_lex::doc::check_id(id) {
            Ok(()) => {
                doc.items.insert(id.clone(), item.clone());
            }
            Err(d) => problems.push(d.suggest(format!("delete it with lex_edit {{\"op\": \"delete\", \"id\": \"{id}\"}} and put it again under a valid id"))),
        }
    }
    Loaded { doc, problems }
}

/// Every diagnostic, uncapped: the load problems first, then validation.
fn all_diags(l: &Loaded) -> Vec<Diagnostic> {
    let mut d = l.problems.clone();
    d.extend(specula_lex::compile::validate_all(&l.doc, &Options { natives: Some(Vec::new()), ..Default::default() }));
    d
}

fn error_keys(ds: &[Diagnostic]) -> BTreeSet<(String, String, String)> {
    ds.iter().filter(|d| d.is_error()).map(|d| (d.code.to_owned(), d.item.clone(), d.path.clone())).collect()
}

fn diag_json(ds: &[Diagnostic], limit: usize) -> Json {
    Json::arr(ds.iter().take(limit).map(Diagnostic::to_json))
}

/// Where an edit reads the stored set from and writes its result to: the engine, or a test.
trait Store {
    fn snapshot(&mut self, set: &str) -> Result<Json, ToolOutput>;
    fn commit(&mut self, c: Commit) -> ToolOutput;
}

struct EngineStore<'a> {
    tools: &'a Tools,
    session: &'a SessionInfo,
}

fn output_json(out: &ToolOutput) -> Json {
    match out.content.first() {
        Some(specula_protocol::Content::Text(t)) => Json::parse(t).unwrap_or_else(|_| Json::from(t.as_str())),
        _ => Json::Null,
    }
}

impl Store for EngineStore<'_> {
    fn snapshot(&mut self, set: &str) -> Result<Json, ToolOutput> {
        let out = self.tools.send(self.session, Command::Lex(LexCall::Snapshot { set: set.to_owned(), only: None }));
        if out.is_error {
            return Err(out);
        }
        Ok(output_json(&out))
    }

    fn commit(&mut self, c: Commit) -> ToolOutput {
        self.tools.send(self.session, Command::Lex(LexCall::Commit(c)))
    }
}

/// How often an edit is validated again when other agents keep changing the set under it.
const EDIT_ATTEMPTS: usize = 4;

/// An edit: read the set, apply the ops, validate before and after (on this thread), then
/// commit if nothing changed meanwhile (else start over).
fn edit_in(store: &mut dyn Store, e: &Edit) -> ToolOutput {
    for _ in 0..EDIT_ATTEMPTS {
        let snap = match store.snapshot(&e.set) {
            Ok(s) => s,
            Err(out) => return out,
        };
        let exists = snap.get("exists") == Some(&Json::Bool(true));
        let mut before = BTreeMap::new();
        let mut item_versions = BTreeMap::new();
        let mut base_items = BTreeMap::new();
        for it in snap.get("items").and_then(Json::as_array).unwrap_or_default() {
            let id = it.get("id").and_then(Json::as_str).unwrap_or("").to_owned();
            let version = it.get("version").and_then(Json::as_u64).unwrap_or(0);
            item_versions.insert(id.clone(), version);
            base_items.insert(id.clone(), (it.get("eid").and_then(Json::as_u64).unwrap_or(0), version));
            before.insert(id, it.get("item").cloned().unwrap_or(Json::Null));
        }
        let base = exists.then(|| Versions { set: snap.get("set_version").and_then(Json::as_u64).unwrap_or(0), items: base_items });
        let old_header = header_of(snap.get("header"), &e.set);
        let mut header = old_header.clone();
        if let Some(n) = &e.header.name {
            header.set("name", Json::from(n.as_str()));
        }
        if let Some(hz) = e.header.tick_hz {
            header.set("tick_hz", Json::from(hz));
        }
        match &e.header.notes {
            Some(Some(n)) => {
                header.set("notes", Json::from(n.as_str()));
            }
            Some(None) => {
                if let Json::Obj(p) = &mut header {
                    p.retain(|(k, _)| k != "notes");
                }
            }
            None => {}
        }
        let mut after = before.clone();
        let mut touched: BTreeSet<String> = BTreeSet::new();
        for (i, op) in e.ops.iter().enumerate() {
            let id = op.id().to_owned();
            let fail = |code: &'static str, msg: String, extra: Vec<(&str, Json)>| {
                let mut x = vec![("op_index", Json::from(i)), ("id", Json::from(id.as_str()))];
                x.extend(extra);
                err_with(code, format!("ops[{i}] ({id}): {msg}; no op was applied"), x)
            };
            // A badly named stored item can still be deleted.
            let deleting_existing = matches!(op, EditOp::Delete { .. }) && before.contains_key(&id);
            if !deleting_existing {
                if let Err(d) = specula_lex::doc::check_id(&id) {
                    return fail("invalid_id", d.message, vec![("suggestion", Json::from(d.suggestion.unwrap_or_default()))]);
                }
            }
            if let Some(want) = op.expect() {
                let have = item_versions.get(&id).copied().unwrap_or(0);
                if have != want {
                    return fail(
                        "version_conflict",
                        format!("expected version {want}, but the item is at version {have}{}", if have == 0 { " (it doesn't exist)" } else { "" }),
                        vec![("version", Json::from(have)), ("suggestion", Json::from("lex_get it again, redo the change on the current item, and pass its version"))],
                    );
                }
            }
            match op {
                EditOp::Put { item, .. } => {
                    after.insert(id.clone(), item.clone());
                }
                EditOp::Patch { set, remove, .. } => {
                    let Some(item) = after.get_mut(&id) else {
                        let hint = did_you_mean(&id, before.keys().map(String::as_str)).unwrap_or_else(|| "put it first".into());
                        return fail("unknown_item", format!("there is no item '{id}' to patch"), vec![("suggestion", Json::from(hint))]);
                    };
                    for (path, v) in set {
                        if let Err(m) = json_set(item, path, v.clone()) {
                            return fail("bad_path", format!("set '{path}': {m}"), vec![]);
                        }
                    }
                    for path in remove {
                        if let Err(m) = json_remove(item, path) {
                            return fail("bad_path", format!("remove '{path}': {m}"), vec![]);
                        }
                    }
                }
                EditOp::Delete { .. } => {
                    if after.remove(&id).is_none() {
                        let hint = did_you_mean(&id, before.keys().map(String::as_str)).unwrap_or_default();
                        return fail("unknown_item", format!("there is no item '{id}' to delete"), vec![("suggestion", Json::from(hint))]);
                    }
                }
            }
            touched.insert(id);
        }
        if e.replace {
            after.retain(|k, _| touched.contains(k));
        }
        // Validate before and after, uncapped: refuse an edit that adds errors (unless a draft).
        let old_keys = error_keys(&all_diags(&load(&old_header, &before, &e.set)));
        let new_diags = all_diags(&load(&header, &after, &e.set));
        let added: Vec<&Diagnostic> = new_diags.iter().filter(|d| d.is_error() && !old_keys.contains(&(d.code.to_owned(), d.item.clone(), d.path.clone()))).collect();
        let errors = new_diags.iter().filter(|d| d.is_error()).count();
        if !added.is_empty() && !e.allow_errors {
            return err_with(
                "invalid",
                format!("the edit would add {} validation error(s), so nothing was changed; fix them (each says what to change) or pass allow_errors: true to store a draft", added.len()),
                vec![("diagnostics", Json::arr(added.iter().take(50).map(|d| d.to_json()))), ("errors_after", Json::from(errors))],
            );
        }
        let header_changed = !exists || old_header.to_string() != header.to_string();
        let mut changes: Vec<(String, &'static str)> = Vec::new();
        let mut puts = Vec::new();
        for (id, item) in &after {
            match before.get(id) {
                None => {
                    puts.push((id.clone(), item.clone(), true));
                    changes.push((id.clone(), "added"));
                }
                Some(old) if specula_lex::doc::canonical(old).to_string() != specula_lex::doc::canonical(item).to_string() => {
                    puts.push((id.clone(), item.clone(), false));
                    changes.push((id.clone(), "changed"));
                }
                Some(_) => {}
            }
        }
        let deletes: Vec<String> = before.keys().filter(|k| !after.contains_key(*k)).cloned().collect();
        changes.extend(deletes.iter().map(|d| (d.clone(), "deleted")));
        let summary = |journal: Option<u64>, versions: Option<&Json>| {
            let list: Vec<Json> = changes
                .iter()
                .map(|(id, what)| {
                    let v = versions.and_then(|v| v.get(id)).cloned().unwrap_or(Json::Null);
                    Json::obj([("id", Json::from(id.as_str())), ("change", Json::from(*what)), ("version", v)])
                })
                .collect();
            let warnings: Vec<&Diagnostic> = new_diags.iter().filter(|d| !d.is_error()).collect();
            let mut out = vec![
                ("set", Json::from(e.set.as_str())),
                ("changes", Json::Arr(list)),
                ("items", Json::from(after.len())),
                ("valid", Json::Bool(errors == 0)),
                ("journal", Json::from(journal)),
            ];
            if errors > 0 {
                out.push(("errors", Json::arr(new_diags.iter().filter(|d| d.is_error()).take(50).map(Diagnostic::to_json))));
            }
            if !warnings.is_empty() {
                out.push(("warnings", Json::arr(warnings.iter().take(50).map(|d| d.to_json()))));
            }
            Json::obj(out)
        };
        if e.dry_run {
            let mut s = summary(None, None);
            s.set("dry_run", Json::Bool(true));
            return ToolOutput::text(s.to_pretty());
        }
        if changes.is_empty() && !header_changed {
            return ToolOutput::text(summary(None, None).to_pretty());
        }
        let out = store.commit(Commit { set: e.set.clone(), base, header: header_changed.then(|| header.clone()), puts, deletes });
        let reply = output_json(&out);
        if out.is_error {
            if reply.get("error").and_then(Json::as_str) == Some("set_changed") {
                continue;
            }
            let mut j = reply;
            j.set("tool", Json::from(e.tool));
            return ToolOutput::error(j.to_pretty());
        }
        return ToolOutput::text(summary(reply.get("journal").and_then(Json::as_u64), reply.get("versions")).to_pretty());
    }
    err("busy", format!("other agents kept changing set '{}' while this edit was validated ({EDIT_ATTEMPTS} tries); claim_take /lex/{} first, then edit", e.set, e.set))
}

/// Splits `a.b[2].c` into keys and indices.
fn path_parts(path: &str) -> Result<Vec<Result<String, usize>>, String> {
    let mut out = Vec::new();
    for seg in path.split('.') {
        let (name, rest) = match seg.find('[') {
            Some(i) => (&seg[..i], &seg[i..]),
            None => (seg, ""),
        };
        if !name.is_empty() {
            out.push(Ok(name.to_owned()));
        } else if rest.is_empty() {
            return Err("empty path segment (two dots?)".into());
        }
        let mut r = rest;
        while let Some(s) = r.strip_prefix('[') {
            let Some(end) = s.find(']') else { return Err("unclosed '['".into()) };
            let n: usize = s[..end].parse().map_err(|_| format!("'{}' is not an index", &s[..end]))?;
            out.push(Err(n));
            r = &s[end + 1..];
        }
        if !r.is_empty() {
            return Err(format!("unexpected '{r}'"));
        }
    }
    if out.is_empty() {
        return Err("empty path".into());
    }
    Ok(out)
}

/// Sets `path` in `root` (objects are created as needed; an index may append at the end).
fn json_set(root: &mut Json, path: &str, value: Json) -> Result<(), String> {
    let parts = path_parts(path)?;
    let mut cur = root;
    for (k, p) in parts.iter().enumerate() {
        let last = k + 1 == parts.len();
        match p {
            Ok(key) => {
                if !matches!(cur, Json::Obj(_)) {
                    return Err(format!("'{key}' is a field of something that isn't an object"));
                }
                let Json::Obj(pairs) = cur else { unreachable!() };
                let pos = match pairs.iter().position(|(n, _)| n == key) {
                    Some(i) => i,
                    None => {
                        pairs.push((key.clone(), Json::Obj(Vec::new())));
                        pairs.len() - 1
                    }
                };
                if last {
                    pairs[pos].1 = value;
                    return Ok(());
                }
                cur = &mut pairs[pos].1;
            }
            Err(i) => {
                let Json::Arr(items) = cur else { return Err(format!("[{i}] indexes something that isn't an array")) };
                if *i > items.len() {
                    return Err(format!("index {i} is past the end (length {})", items.len()));
                }
                if *i == items.len() {
                    items.push(Json::Obj(Vec::new()));
                }
                if last {
                    items[*i] = value;
                    return Ok(());
                }
                cur = &mut items[*i];
            }
        }
    }
    Ok(())
}

fn json_remove(root: &mut Json, path: &str) -> Result<(), String> {
    let parts = path_parts(path)?;
    let mut cur = root;
    for (k, p) in parts.iter().enumerate() {
        let last = k + 1 == parts.len();
        match (p, cur) {
            (Ok(key), Json::Obj(pairs)) => {
                let Some(pos) = pairs.iter().position(|(n, _)| n == key) else { return Err(format!("no field '{key}'")) };
                if last {
                    pairs.remove(pos);
                    return Ok(());
                }
                cur = &mut pairs[pos].1;
            }
            (Err(i), Json::Arr(items)) => {
                if *i >= items.len() {
                    return Err(format!("index {i} is past the end"));
                }
                if last {
                    items.remove(*i);
                    return Ok(());
                }
                cur = &mut items[*i];
            }
            _ => return Err("the path doesn't match the item's shape".into()),
        }
    }
    Ok(())
}

// ---- protocol threads ----

fn set_arg(args: &Json, required: bool) -> Result<Option<String>, String> {
    match args.get("set") {
        None | Some(Json::Null) if required => Err("'set' is required: the rule set's name (lex_list shows them)".into()),
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => {
            specula_lex::doc::check_id(s).map_err(|d| format!("'set' {}: {}", d.message, d.suggestion.unwrap_or_default()))?;
            Ok(Some(s.clone()))
        }
        Some(_) => Err("'set' must be a string".into()),
    }
}

fn u64_arg(args: &Json, key: &str, lo: u64, hi: u64) -> Result<Option<u64>, String> {
    crate::tools::int_arg(args, key, lo, hi)
}

fn bool_of(args: &Json, key: &str) -> Result<bool, String> {
    Ok(crate::tools::bool_arg(args, key)?.unwrap_or(false))
}

/// The stored set, loaded (with its load problems), from a main-thread snapshot.
fn fetch(tools: &Tools, session: &SessionInfo, set: &str) -> Result<Loaded, ToolOutput> {
    let j = EngineStore { tools, session }.snapshot(set)?;
    if j.get("exists") != Some(&Json::Bool(true)) {
        let sets = output_json(&tools.send(session, Command::Lex(LexCall::Sets { after: None, limit: 500 })));
        let names: Vec<String> = sets.get("sets").and_then(Json::as_array).unwrap_or_default().iter().filter_map(|s| s.get("set").and_then(Json::as_str).map(str::to_owned)).collect();
        let hint = did_you_mean(set, names.iter().map(String::as_str)).unwrap_or_else(|| "create it with lex_edit or lex_import".into());
        return Err(err_with("unknown_set", format!("there is no rule set '{set}'"), vec![("suggestion", Json::from(hint))]));
    }
    let mut items = BTreeMap::new();
    for it in j.get("items").and_then(Json::as_array).unwrap_or_default() {
        items.insert(it.get("id").and_then(Json::as_str).unwrap_or("").to_owned(), it.get("item").cloned().unwrap_or(Json::Null));
    }
    Ok(load(&header_of(j.get("header"), set), &items, set))
}

/// A set (by name) or an inline `document`. A stored set whose header or item ids are broken is
/// refused here (with what to fix), so nothing runs it at the wrong rate or without some items.
fn doc_arg(tools: &Tools, session: &SessionInfo, args: &Json, tool: &str) -> Result<(Document, Option<String>), ToolOutput> {
    match (args.get("set"), args.get("document")) {
        (Some(_), Some(_)) => Err(err("invalid_argument", format!("{tool}: give 'set' or 'document', not both"))),
        (None, Some(d)) => {
            let doc = Document::from_json(d).map_err(|ds| err_with("invalid_document", "the document's header or item ids are invalid", vec![("diagnostics", diag_json(&ds, 50))]))?;
            Ok((doc, None))
        }
        _ => {
            let set = set_arg(args, true).map_err(|m| err("invalid_argument", m))?.expect("required");
            let l = fetch(tools, session, &set)?;
            if !l.problems.is_empty() {
                return Err(err_with(
                    "invalid_set",
                    format!("set '{set}' has a broken header or item ids (changed outside lex_edit?); fix them with lex_edit first"),
                    vec![("diagnostics", diag_json(&l.problems, 50))],
                ));
            }
            Ok((l.doc, Some(set)))
        }
    }
}

fn params_arg(args: &Json) -> Result<Vec<(String, Json)>, String> {
    match args.get("params") {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Obj(p)) => Ok(p.clone()),
        Some(_) => Err("'params' must be an object of param id → value (in the param's unit, e.g. 7.5 or \"7.5m\")".into()),
    }
}

fn seed_arg(args: &Json) -> Result<u64, String> {
    match args.get("seed") {
        None | Some(Json::Null) => Ok(1),
        Some(Json::Str(s)) => s.parse().map_err(|_| "'seed' must be a whole number (a decimal string for values above 2^53)".into()),
        Some(v) => v.as_u64().ok_or_else(|| "'seed' must be a non-negative whole number".into()),
    }
}

fn queries(args: &Json, key: &str) -> Result<Vec<String>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Arr(a)) if a.len() <= MAX_QUERIES => a.iter().map(|q| q.as_str().map(str::to_owned).ok_or_else(|| format!("'{key}' entries are expression strings"))).collect(),
        Some(Json::Arr(_)) => Err(format!("at most {MAX_QUERIES} '{key}' expressions")),
        Some(_) => Err(format!("'{key}' must be an array of expression strings, e.g. [\"res.gold\"]")),
    }
}

/// Answers a `lex_*` tool call (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let out = match name {
        "lex_schema" => schema_tool(args),
        "lex_list" => list_tool(tools, session, args),
        "lex_get" => get_tool(tools, session, args),
        "lex_validate" => validate_tool(tools, session, args),
        "lex_edit" => edit_tool(tools, session, args),
        "lex_import" => import_tool(tools, session, args),
        "lex_export" => export_tool(tools, session, args),
        "lex_simulate" => simulate_tool(tools, session, args),
        "lex_session" => session_tool(tools, session, args),
        other => Err(format!("unknown lex tool '{other}'")),
    };
    out.unwrap_or_else(|m| ToolOutput::error(crate::tools::argument_error(&m).to_pretty()))
}

fn schema_tool(args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "lex_schema", &[&["topic"]])?;
    let topic = crate::tools::string_arg(args, "topic")?;
    Ok(ToolOutput::text(schema::to_json(topic)?.to_string()))
}

fn summary_of(id: &str, item: &Json) -> Json {
    let kind = item.get("kind").and_then(Json::as_str).unwrap_or("?");
    let mut s = vec![("id", Json::from(id)), ("kind", Json::from(kind))];
    match kind {
        "archetype" => {
            if let Some(t) = item.get("tags") {
                s.push(("tags", t.clone()));
            }
            if let Some(e) = item.get("extends") {
                s.push(("extends", e.clone()));
            }
            let types: Vec<Json> = item.get("behaviours").and_then(Json::as_array).unwrap_or_default().iter().filter_map(|b| b.get("type").cloned()).collect();
            if !types.is_empty() {
                s.push(("behaviours", Json::Arr(types)));
            }
        }
        "param" => {
            for k in ["type", "value", "min", "max"] {
                if let Some(v) = item.get(k) {
                    s.push((k, v.clone()));
                }
            }
        }
        "rule" => {
            if let Some(v) = item.get("on") {
                s.push(("on", v.clone()));
            }
        }
        "native" => {
            for k in ["hook", "reason"] {
                if let Some(v) = item.get(k) {
                    s.push((k, v.clone()));
                }
            }
        }
        _ => {}
    }
    if let Some(Json::Str(n)) = item.get("notes") {
        let short: String = n.chars().take(120).collect();
        s.push(("notes", Json::from(if short.len() < n.len() { format!("{short}...") } else { short })));
    }
    Json::obj(s)
}

fn list_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "lex_list", &[&["set", "kind", "cursor", "limit"]])?;
    let limit = u64_arg(args, "limit", 1, LIST_LIMITS.1)?.unwrap_or(LIST_LIMITS.0) as usize;
    let cursor = crate::tools::string_arg(args, "cursor")?.map(str::to_owned);
    let Some(set) = set_arg(args, false)? else {
        return Ok(tools.send(session, Command::Lex(LexCall::Sets { after: cursor, limit })));
    };
    let kind = crate::tools::string_arg(args, "kind")?.map(str::to_owned);
    if let Some(k) = &kind {
        if schema::kind(k).is_none() {
            let names: Vec<&str> = schema::KINDS.iter().map(|k| k.name).collect();
            return Err(format!("'kind' {k:?} is not an item kind; {}", did_you_mean(k, names).unwrap_or_default()));
        }
    }
    Ok(tools.send(session, Command::Lex(LexCall::List { set, kind, after: cursor, limit })))
}

fn get_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "lex_get", &[&["set", "ids"]])?;
    let set = set_arg(args, true)?.expect("required");
    let ids: Vec<String> = match args.get("ids") {
        Some(Json::Arr(a)) if !a.is_empty() && a.len() <= MAX_GET => a.iter().map(|x| x.as_str().map(str::to_owned).ok_or("'ids' are item id strings")).collect::<Result<_, _>>()?,
        Some(Json::Str(s)) => vec![s.clone()],
        _ => return Err(format!("'ids' must be an item id or an array of 1 to {MAX_GET} ids (lex_list pages through all of them)")),
    };
    let snap = tools.send(session, Command::Lex(LexCall::Snapshot { set: set.clone(), only: Some(ids.clone()) }));
    if snap.is_error {
        return Ok(snap);
    }
    let j = output_json(&snap);
    if j.get("exists") != Some(&Json::Bool(true)) {
        return Ok(err("unknown_set", format!("there is no rule set '{set}'; lex_list without 'set' lists them")));
    }
    let stored = j.get("items").and_then(Json::as_array).unwrap_or_default();
    let all: Vec<String> = j.get("all_ids").and_then(Json::as_array).unwrap_or_default().iter().filter_map(|x| x.as_str().map(str::to_owned)).collect();
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for id in &ids {
        match stored.iter().find(|it| it.get("id").and_then(Json::as_str) == Some(id)) {
            Some(it) => found.push(Json::obj([
                ("id", Json::from(id.as_str())),
                ("version", it.get("version").cloned().unwrap_or(Json::Null)),
                ("item", specula_lex::doc::canonical(it.get("item").unwrap_or(&Json::Null))),
            ])),
            None => missing.push(Json::obj([
                ("id", Json::from(id.as_str())),
                ("suggestion", Json::from(did_you_mean(id, all.iter().map(String::as_str)).unwrap_or_default())),
            ])),
        }
    }
    let mut out = vec![("set", Json::from(set)), ("items", Json::Arr(found))];
    if !missing.is_empty() {
        out.push(("missing", Json::Arr(missing)));
    }
    Ok(ToolOutput::text(Json::obj(out).to_string()))
}

/// Most params and natives `lex_validate` lists.
const MAX_LISTED: usize = 200;

fn validate_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "lex_validate", &[&["set", "document", "params", "limit"]])?;
    let params = params_arg(args)?;
    let limit = u64_arg(args, "limit", 1, specula_lex::compile::MAX_DIAGNOSTICS as u64)?.unwrap_or(50) as usize;
    // A stored set is validated even when damaged: its load problems lead the list.
    let (doc, set, mut diags) = match (args.get("set"), args.get("document")) {
        (Some(_), None) => {
            let set = set_arg(args, true)?.expect("required");
            match fetch(tools, session, &set) {
                Ok(l) => (l.doc, Some(set), l.problems),
                Err(e) => return Ok(e),
            }
        }
        _ => match doc_arg(tools, session, args, "lex_validate") {
            Ok((d, s)) => (d, s, Vec::new()),
            Err(e) => return Ok(e),
        },
    };
    let opts = Options { overrides: params, natives: Some(Vec::new()) };
    let rules = match compile(&doc, &opts) {
        Ok(c) => {
            diags.extend(c.warnings);
            Some(c.rules)
        }
        Err(d) => {
            diags.extend(d);
            None
        }
    };
    diags.sort_by_key(|d| d.severity);
    let errors = diags.iter().filter(|d| d.is_error()).count();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for item in doc.items.values() {
        *counts.entry(item.get("kind").and_then(Json::as_str).unwrap_or("?").chars().take(32).collect()).or_insert(0) += 1;
    }
    let mut out = vec![
        ("set", Json::from(set)),
        ("valid", Json::Bool(errors == 0)),
        ("errors", Json::from(errors)),
        ("warnings", Json::from(diags.len() - errors)),
        ("diagnostics", diag_json(&diags, limit)),
        ("items_by_kind", Json::Obj(counts.into_iter().take(MAX_LISTED).map(|(k, v)| (k, Json::from(v))).collect())),
    ];
    if let Some(r) = rules.filter(|_| errors == 0) {
        out.push(("rules_hash", Json::from(format!("{:016x}", r.hash))));
        out.push((
            "params",
            Json::arr(r.params.iter().take(MAX_LISTED).map(|p| {
                Json::obj([
                    ("id", Json::from(p.id.as_str())),
                    ("type", Json::from(p.unit.name())),
                    ("value", show_val(&r, specula_lex::Val::Int(p.value), specula_lex::expr::Ty::Num(p.unit))),
                    ("doc", Json::from(p.doc.chars().take(200).collect::<String>())),
                ])
            })),
        ));
        if r.params.len() > MAX_LISTED {
            out.push(("params_not_listed", Json::from(r.params.len() - MAX_LISTED)));
        }
        if !r.natives.is_empty() {
            out.push(("natives", Json::arr(r.natives.iter().take(MAX_LISTED).map(|n| Json::obj([("id", Json::from(n.id.as_str())), ("hook", Json::from(n.hook.as_str())), ("reason", Json::from(n.reason.chars().take(300).collect::<String>()))])))));
        }
        if r.natives.len() > MAX_LISTED {
            out.push(("natives_not_listed", Json::from(r.natives.len() - MAX_LISTED)));
        }
    }
    Ok(ToolOutput::text(Json::obj(out).to_string()))
}

fn expect_arg(o: &Json) -> Result<Option<u64>, String> {
    match o.get("expect_version") {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or_else(|| "'expect_version' must be a whole number (0: the item must not exist yet)".into()),
    }
}

fn parse_ops(args: &Json) -> Result<Vec<EditOp>, String> {
    let Some(list) = args.get("ops").and_then(Json::as_array) else {
        return Err("'ops' must be an array of {op: put|patch|delete, id, ...}".into());
    };
    if list.len() > MAX_OPS {
        return Err(format!("at most {MAX_OPS} ops per lex_edit"));
    }
    let mut ops = Vec::new();
    for (i, o) in list.iter().enumerate() {
        let what = |m: &str| format!("ops[{i}]: {m}");
        let op = o.get("op").and_then(Json::as_str).ok_or_else(|| what("'op' must be put, patch or delete"))?;
        let id = o.get("id").and_then(Json::as_str).ok_or_else(|| what("'id' (the item id) is required"))?.to_owned();
        let expect = expect_arg(o).map_err(|m| what(&m))?;
        let allowed: &[&str] = match op {
            "put" => &["op", "id", "item", "expect_version"],
            "patch" => &["op", "id", "set", "remove", "expect_version"],
            "delete" => &["op", "id", "expect_version"],
            other => return Err(what(&format!("'{other}' is not an op; {}", did_you_mean(other, ["put", "patch", "delete"]).unwrap_or_default()))),
        };
        check_keys(o, &format!("ops[{i}] ({op})"), &[allowed])?;
        ops.push(match op {
            "put" => {
                let item = o.get("item").cloned().ok_or_else(|| what("put needs 'item' (the whole item, with its kind)"))?;
                if !matches!(item, Json::Obj(_)) {
                    return Err(what("'item' must be an object"));
                }
                EditOp::Put { id, item, expect }
            }
            "patch" => {
                let set = match o.get("set") {
                    None => Vec::new(),
                    Some(Json::Obj(p)) => p.clone(),
                    Some(_) => return Err(what("'set' must be an object of field path → value, e.g. {\"behaviours[0].range\": \"8m\"}")),
                };
                let remove = match o.get("remove") {
                    None => Vec::new(),
                    Some(Json::Arr(a)) => a.iter().map(|x| x.as_str().map(str::to_owned).ok_or_else(|| what("'remove' entries are field paths"))).collect::<Result<_, _>>()?,
                    Some(_) => return Err(what("'remove' must be an array of field paths")),
                };
                if set.is_empty() && remove.is_empty() {
                    return Err(what("patch needs 'set' or 'remove'"));
                }
                EditOp::Patch { id, set, remove, expect }
            }
            _ => EditOp::Delete { id, expect },
        });
    }
    Ok(ops)
}

fn header_args(args: &Json) -> Result<Header, String> {
    Ok(Header {
        name: crate::tools::string_arg(args, "name")?.map(str::to_owned),
        tick_hz: u64_arg(args, "tick_hz", 1, 1000)?.map(|v| v as u32),
        notes: match args.get("notes") {
            None => None,
            Some(Json::Null) => Some(None),
            Some(Json::Str(s)) => Some(Some(s.clone())),
            Some(_) => return Err("'notes' must be a string (null removes them)".into()),
        },
    })
}

fn edit_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "lex_edit", &[&["set", "ops", "allow_errors", "dry_run", "name", "tick_hz", "notes"]])?;
    let set = set_arg(args, true)?.expect("required");
    let ops = parse_ops(args)?;
    let e = Edit { set, header: header_args(args)?, ops, allow_errors: bool_of(args, "allow_errors")?, dry_run: bool_of(args, "dry_run")?, replace: false, tool: "lex_edit" };
    Ok(locked(tools, &e.set, || edit_in(&mut EngineStore { tools, session }, &e)).unwrap_or_else(|busy| busy))
}

fn import_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "lex_import", &[&["set", "document", "replace", "allow_errors", "dry_run"]])?;
    let set = set_arg(args, true)?.expect("required");
    let d = args.get("document").ok_or("'document' is required: {\"lex\": 1, \"tick_hz\": ..., \"items\": {...}}")?;
    let doc = match Document::from_json(d) {
        Ok(doc) => doc,
        Err(ds) => return Ok(err_with("invalid_document", "the document's header or item ids are invalid; nothing was imported", vec![("diagnostics", diag_json(&ds, 50))])),
    };
    if doc.items.len() > MAX_OPS * 20 {
        return Err(format!("the document has {} items; at most {}", doc.items.len(), MAX_OPS * 20));
    }
    let ops = doc.items.iter().map(|(id, item)| EditOp::Put { id: id.clone(), item: item.clone(), expect: None }).collect();
    let header = Header { name: Some(doc.name.clone()).filter(|n| !n.is_empty()), tick_hz: Some(doc.tick_hz), notes: Some(doc.notes.clone()) };
    let replace = crate::tools::bool_arg(args, "replace")?.unwrap_or(true);
    let e = Edit { set, header, ops, allow_errors: bool_of(args, "allow_errors")?, dry_run: bool_of(args, "dry_run")?, replace, tool: "lex_import" };
    Ok(locked(tools, &e.set, || edit_in(&mut EngineStore { tools, session }, &e)).unwrap_or_else(|busy| busy))
}

fn export_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "lex_export", &[&["set", "pretty"]])?;
    if args.get("document").is_some() {
        return Err("lex_export exports a stored set: give 'set'".into());
    }
    let (doc, _) = match doc_arg(tools, session, args, "lex_export") {
        Ok(x) => x,
        Err(e) => return Ok(e),
    };
    let j = doc.to_json();
    let text = if crate::tools::bool_arg(args, "pretty")?.unwrap_or(true) { j.to_pretty() } else { j.to_string() };
    if text.len() > MAX_EXPORT_BYTES {
        return Ok(err("too_large", format!("the document is {} bytes (most {MAX_EXPORT_BYTES}); page through it with lex_list and lex_get", text.len())));
    }
    Ok(ToolOutput::text(text))
}

fn simulate_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "lex_simulate", &[&["set", "document", "seed", "ticks", "commands", "params", "expect", "watch", "sample_every", "from", "save_as"]])?;
    let params = params_arg(args)?;
    let ticks = u64_arg(args, "ticks", 1, MAX_SIM_TICKS)?.unwrap_or(DEFAULT_SIM_TICKS) as u32;
    let sample_every = u64_arg(args, "sample_every", 0, MAX_SIM_TICKS)?.unwrap_or(0) as u32;
    let from = crate::tools::string_arg(args, "from")?.map(str::to_owned);
    let save_as = crate::tools::string_arg(args, "save_as")?.map(str::to_owned);
    if let Some(n) = &save_as {
        if n.is_empty() || n.len() > 64 {
            return Err("'save_as' must be a name of 1 to 64 characters".into());
        }
    }
    let watch = queries(args, "watch")?;
    let expect = queries(args, "expect")?;
    let (doc, set) = match doc_arg(tools, session, args, "lex_simulate") {
        Ok(x) => x,
        Err(e) => return Ok(e),
    };
    let compiled = match compile(&doc, &Options { overrides: params, natives: None }) {
        Ok(c) => c,
        Err(ds) => {
            return Ok(err_with("invalid", "the rules don't compile; fix these first (each says what to change)", vec![("diagnostics", diag_json(&ds, 50))]));
        }
    };
    let rules = Arc::new(compiled.rules);
    let mut sim = match &from {
        None => match Sim::new(rules.clone(), seed_arg(args)?, &Natives::new()) {
            Ok(s) => s,
            Err(m) => return Ok(err("native_unbound", m)),
        },
        Some(name) => {
            let runs = tools.lex_runs.0.lock().unwrap_or_else(|p| p.into_inner());
            let Some(saved) = runs.iter().find(|r| &r.name == name) else {
                let names: Vec<&str> = runs.iter().map(|r| r.name.as_str()).collect();
                return Ok(err_with("unknown_session", format!("no saved run '{name}'"), vec![("suggestion", Json::from(did_you_mean(name, names).unwrap_or_else(|| "lex_session {action: list}".into())))]));
            };
            if saved.sim.rules().hash != rules.hash {
                return Ok(err_with(
                    "rules_changed",
                    format!("run '{name}' was made with other rules (hash {:016x}, now {:016x})", saved.sim.rules().hash, rules.hash),
                    vec![("suggestion", Json::from("simulate from a fresh state (drop 'from'), or pass the same params the run used"))],
                ));
            }
            if args.get("seed").is_some() {
                return Ok(err("invalid_argument", "a saved run keeps its seed; drop 'seed' when using 'from'"));
            }
            saved.sim.clone()
        }
    };
    let mut commands = Vec::new();
    if let Some(list) = args.get("commands") {
        let Some(list) = list.as_array().filter(|l| l.len() <= MAX_SIM_COMMANDS) else {
            return Err(format!("'commands' must be an array of at most {MAX_SIM_COMMANDS} {{tick, command, args}}"));
        };
        for (i, c) in list.iter().enumerate() {
            check_keys(c, &format!("commands[{i}]"), &[&["tick", "command", "args"]])?;
            let tick = c.get("tick").and_then(Json::as_u64).filter(|t| *t <= u64::from(u32::MAX / 2)).ok_or_else(|| format!("commands[{i}]: 'tick' must be a whole number"))? as u32;
            if tick < sim.tick() {
                return Err(format!("commands[{i}]: tick {tick} is in the past (the run is at tick {})", sim.tick()));
            }
            let name = c.get("command").and_then(Json::as_str).ok_or_else(|| format!("commands[{i}]: 'command' must be a command id"))?;
            let call = match sim.parse_command(name, c.get("args").unwrap_or(&Json::Null)) {
                Ok(x) => x,
                Err(r) => {
                    let mut j = r.to_json();
                    j.set("command_index", Json::from(i));
                    return Ok(ToolOutput::error(j.to_pretty()));
                }
            };
            commands.push(Timed { tick, name: name.to_owned(), call });
        }
    }
    let compile_all = |srcs: &[String]| -> Result<Vec<specula_lex::run::Query>, ToolOutput> {
        srcs.iter().map(|s| compile_query(&rules, s).map_err(|d| err_with("invalid_query", d.message.clone(), vec![("diagnostic", d.to_json())]))).collect()
    };
    let watch = match compile_all(&watch) {
        Ok(w) => w,
        Err(e) => return Ok(e),
    };
    let expect = match compile_all(&expect) {
        Ok(w) => w,
        Err(e) => return Ok(e),
    };
    let spec = RunSpec { ticks, commands, watch, expect, sample_every, deadline: Some(Instant::now() + SIM_WALL_BUDGET) };
    let started = Instant::now();
    let mut report = run(&mut sim, &spec);
    report.set("wall_ms", Json::from((started.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0));
    if let Some(s) = &set {
        report.set("set", Json::from(s.as_str()));
    }
    if !compiled.warnings.is_empty() {
        report.set("warnings", diag_json(&compiled.warnings, 20));
    }
    if let Some(name) = save_as {
        let mut runs = tools.lex_runs.0.lock().unwrap_or_else(|p| p.into_inner());
        // Another session's run is never overwritten.
        if let Some(other) = runs.iter().find(|r| r.name == name && r.session != session.id) {
            report.set("save_error", Json::from(format!("the run name '{name}' belongs to {}'s session; nothing was saved (pick another save_as)", other.owner)));
            return Ok(ToolOutput::text(report.to_string()));
        }
        runs.retain(|r| r.name != name);
        if runs.len() >= MAX_RUNS {
            runs.pop_front();
        }
        runs.push_back(SavedRun { name: name.clone(), owner: session.client_name.clone(), session: session.id.clone(), set: set.clone().unwrap_or_default(), sim });
        report.set("saved_as", Json::from(name));
    }
    Ok(ToolOutput::text(report.to_string()))
}

fn session_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "lex_session", &[&["action", "name", "archetype", "tag", "cursor", "limit"]])?;
    let action = crate::tools::string_arg(args, "action")?.unwrap_or("list");
    let mut runs = tools.lex_runs.0.lock().unwrap_or_else(|p| p.into_inner());
    let name = crate::tools::string_arg(args, "name")?;
    let find = |runs: &VecDeque<SavedRun>, n: Option<&str>| -> Result<usize, String> {
        let n = n.ok_or("'name' is required: the run's save_as name")?;
        runs.iter().position(|r| r.name == n).ok_or_else(|| {
            let names: Vec<&str> = runs.iter().map(|r| r.name.as_str()).collect();
            format!("no saved run '{n}'; {}", did_you_mean(n, names).unwrap_or_else(|| "lex_session {action: list}".into()))
        })
    };
    match action {
        "list" => Ok(ToolOutput::text(
            Json::obj([(
                "runs",
                Json::arr(runs.iter().map(|r| {
                    Json::obj([
                        ("name", Json::from(r.name.as_str())),
                        ("owner", Json::from(r.owner.as_str())),
                        ("set", Json::from(r.set.as_str())),
                        ("tick", Json::from(r.sim.tick())),
                        ("outcome", Json::from(r.sim.outcome().map(|(o, _)| o.to_owned()))),
                        ("rules_hash", Json::from(format!("{:016x}", r.sim.rules().hash))),
                        ("hash", Json::from(format!("{:016x}", r.sim.hash()))),
                    ])
                })),
            )])
            .to_string(),
        )),
        "delete" => {
            let i = find(&runs, name)?;
            if runs[i].session != session.id {
                return Err(format!("run {:?} belongs to {}'s session; only it can delete it", runs[i].name, runs[i].owner));
            }
            runs.remove(i);
            Ok(ToolOutput::text(Json::obj([("deleted", Json::from(name.unwrap_or("")))]).to_string()))
        }
        "inspect" => {
            let i = find(&runs, name)?;
            let sim = &runs[i].sim;
            let r = sim.rules();
            let arch = crate::tools::string_arg(args, "archetype")?.map(|a| r.archetype(a).ok_or_else(|| format!("no archetype '{a}'"))).transpose()?;
            let tag = crate::tools::string_arg(args, "tag")?.map(|t| r.tag(t).ok_or_else(|| format!("no tag '{t}'"))).transpose()?;
            let limit = u64_arg(args, "limit", 1, 200)?.unwrap_or(50) as usize;
            let cursor = u64_arg(args, "cursor", 0, u64::from(u32::MAX))?.unwrap_or(0) as u32;
            let mut page = Vec::new();
            let mut next = Json::Null;
            for e in sim.entities().iter().filter(|e| e.alive && e.id > cursor) {
                if arch.is_some_and(|a| e.arch != a) || tag.is_some_and(|t| !r.archetypes[usize::from(e.arch)].tags.contains(&t)) {
                    continue;
                }
                if page.len() == limit {
                    next = page.last().and_then(|p: &Json| p.get("id")).cloned().unwrap_or(Json::Null);
                    break;
                }
                let a = &r.archetypes[usize::from(e.arch)];
                let attrs: Vec<(String, Json)> = a
                    .attrs
                    .iter()
                    .map(|(id, _)| {
                        let def = &r.attrs[usize::from(*id)];
                        let v = sim.attr(e.id, &def.name).unwrap_or(0);
                        (def.name.clone(), show_val(r, specula_lex::Val::Int(v), specula_lex::expr::Ty::Num(def.unit)))
                    })
                    .collect();
                let pos = Json::arr(e.pos.iter().map(|c| Json::from(*c as f64 / 1000.0)));
                let mut j = vec![("id", Json::from(e.id)), ("archetype", Json::from(a.id.as_str())), ("pos_m", pos), ("attributes", Json::Obj(attrs))];
                if !e.effects.is_empty() {
                    j.push((
                        "effects",
                        Json::arr(e.effects.iter().map(|x| {
                            Json::obj([
                                ("effect", Json::from(r.effects[usize::from(x.effect)].id.as_str())),
                                ("magnitude", Json::from(x.magnitude)),
                                ("expires", Json::from(if x.expires == u32::MAX { None } else { Some(x.expires) })),
                            ])
                        })),
                    ));
                }
                if let (Some(m), Some((s, since))) = (a.machine, e.state) {
                    j.push(("state", Json::obj([("state", Json::from(r.machines[usize::from(m)].states[usize::from(s)].name.as_str())), ("since", Json::from(since))])));
                }
                page.push(Json::obj(j));
            }
            let resources: Vec<(String, Json)> = r.resources.iter().map(|d| (d.id.clone(), Json::from(sim.resource(&d.id).unwrap_or(0)))).collect();
            Ok(ToolOutput::text(
                Json::obj([
                    ("name", Json::from(name.unwrap_or(""))),
                    ("tick", Json::from(sim.tick())),
                    ("resources", Json::Obj(resources)),
                    ("entities", Json::Arr(page)),
                    ("next_cursor", next),
                ])
                .to_string(),
            ))
        }
        other => Err(format!("'action' {other:?} must be list, inspect or delete")),
    }
}

// ---- schemas ----

pub fn definitions() -> Vec<ToolDef> {
    let obj = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let set = || t("string", "The rule set's name ([a-z][a-z0-9_]*); it lives at /lex/<set> in the world.");
    let document = || t("object", "A whole Lex document: {\"lex\": 1, \"name\": ..., \"tick_hz\": 30, \"items\": {\"<id>\": {\"kind\": ..., ...}}}.");
    let params = || t("object", "Param overrides, param id -> value in the param's unit (a number like 7.5, or a literal like \"7.5m\"); checked against the param's range.");
    vec![
        ToolDef {
            name: "lex_schema".into(),
            description: "Lex is Specula's gameplay-as-data language: rules are typed JSON items (archetypes with behaviours, GAS-style effects, waves, timers, rules on events, state machines, player commands, outcomes, params, native hooks). This returns the language itself as data: every item kind, behaviour, action, payload, function and unit, with its fields. Start with no topic for the index, then ask for a topic.".into(),
            input_schema: obj(vec![("topic", t("string", "index (default), kinds, behaviours, actions, payloads, expressions, events, units or tick (the phase order)."))], &[]),
        },
        ToolDef {
            name: "lex_list".into(),
            description: "Without 'set': the rule sets in this world. With 'set': its items, one line each (id, kind, version, a summary), sorted by id and paged; filter by kind.".into(),
            input_schema: obj(
                vec![
                    ("set", set()),
                    ("kind", t("string", "Only items of this kind (archetype, effect, command, ...).")),
                    ("cursor", t("string", "next_cursor from the previous page.")),
                    ("limit", t("integer", "Items per page, 1..500 (default 100).")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "lex_get".into(),
            description: "Items in full with their versions (pass a version to lex_edit as expect_version so you never overwrite another agent's newer change). At most 50 per call; unknown ids come back under 'missing' with the closest id.".into(),
            input_schema: obj(vec![("set", set()), ("ids", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("string"))])), ("description", Json::from("Item ids (1..50)."))]))], &["set", "ids"]),
        },
        ToolDef {
            name: "lex_edit".into(),
            description: "Edits a rule set atomically: ops are put (the whole item), patch (field paths like \"behaviours[0].range\": \"8m\", and remove), delete. Creates the set if it doesn't exist. The whole set is validated after the edit: if it would add errors nothing changes and you get every error with the item, path, expected, got and a suggestion (allow_errors stores a draft anyway; dry_run only reports). Claims on /lex/<set> or an item path apply; each edit is journaled (world_history) and undone with world_undo. Header: name, tick_hz (default 60), notes.".into(),
            input_schema: obj(
                vec![
                    ("set", set()),
                    (
                        "ops",
                        Json::obj([
                            ("type", Json::from("array")),
                            ("description", Json::from("[{\"op\": \"put\", \"id\": \"goblin\", \"item\": {\"kind\": \"archetype\", ...}}, {\"op\": \"patch\", \"id\": \"goblin\", \"set\": {\"attributes.hp\": \"30pt\"}, \"remove\": [\"notes\"], \"expect_version\": 3}, {\"op\": \"delete\", \"id\": \"old\"}]. expect_version 0 means the item must not exist yet.")),
                            ("items", Json::obj([("type", Json::from("object"))])),
                        ]),
                    ),
                    ("allow_errors", t("boolean", "Store the edit even if it adds validation errors (a draft).")),
                    ("dry_run", t("boolean", "Validate and report the changes without applying them.")),
                    ("name", t("string", "The set's display name.")),
                    ("tick_hz", t("integer", "Simulation ticks per second, 1..1000 (durations convert with it).")),
                    ("notes", t("string", "Notes on the set (null removes them).")),
                ],
                &["set", "ops"],
            ),
        },
        ToolDef {
            name: "lex_validate".into(),
            description: "Checks a rule set (or an inline document) and returns every error and warning as data (severity, code, item, path, message, expected, got, suggestion, column for expressions), the items by kind, the params (the tunable search space) and the declared native hooks with their reasons.".into(),
            input_schema: obj(vec![("set", set()), ("document", document()), ("params", params()), ("limit", t("integer", "Most diagnostics returned, 1..200 (default 50)."))], &[]),
        },
        ToolDef {
            name: "lex_simulate".into(),
            description: "Runs a rule set headless and deterministically (same rules, seed and commands: same report and hash) for up to 'ticks' ticks or until an outcome, from a fresh state or a saved run ('from'). 'commands' are player inputs at absolute ticks: [{\"tick\": 0, \"command\": \"place\", \"args\": {\"site\": \"a\", \"stone\": \"warding_1\"}}] (refusals are reported with their code). 'watch' samples expressions every 'sample_every' ticks (at most 200 samples); 'expect' checks assertions at the end (expect_passed). The report is bounded: outcome, resources, waves, entities by archetype, event counts, refusals, run-time diagnostics, native hook calls and the state hash. 'params' overrides params (balance sweeps). save_as keeps the end state for lex_session and later 'from'. Runs on your own connection, so it never stalls other agents or the window.".into(),
            input_schema: obj(
                vec![
                    ("set", set()),
                    ("document", document()),
                    ("seed", t("integer", "The RNG seed (default 1); a decimal string above 2^53.")),
                    ("ticks", t("integer", "Most ticks to run, 1..216000 (default 36000).")),
                    ("commands", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("object"))])), ("description", Json::from("[{tick, command, args}], at most 5000."))])),
                    ("params", params()),
                    ("expect", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("string"))])), ("description", Json::from("Boolean expressions checked at the end, e.g. \"res.tombstones >= 5\" (no self, no rand)."))])),
                    ("watch", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("string"))])), ("description", Json::from("Expressions to sample, e.g. \"res.dust\", \"count(enemy)\"."))])),
                    ("sample_every", t("integer", "Sample 'watch' every N ticks (0: only at the end).")),
                    ("from", t("string", "Continue a saved run (its rules must be unchanged).")),
                    ("save_as", t("string", "Keep the end state under this name (16 runs are kept, oldest dropped).")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "lex_import".into(),
            description: "Writes a whole document into a set in one atomic, validated, journaled edit (replace: true, the default, deletes items the document doesn't have). Use it to load a *.lex.json file's contents.".into(),
            input_schema: obj(
                vec![
                    ("set", set()),
                    ("document", document()),
                    ("replace", t("boolean", "Delete items not in the document (default true).")),
                    ("allow_errors", t("boolean", "Store it even with validation errors.")),
                    ("dry_run", t("boolean", "Only validate and report.")),
                ],
                &["set", "document"],
            ),
        },
        ToolDef {
            name: "lex_export".into(),
            description: "A set as one canonical document (items sorted by id, keys sorted): the text form to save as a *.lex.json file or diff. At most 2 MB.".into(),
            input_schema: obj(vec![("set", set()), ("pretty", t("boolean", "Indented (default true)."))], &["set"]),
        },
        ToolDef {
            name: "lex_session".into(),
            description: "Saved simulation runs (lex_simulate save_as): list them, inspect one's entities (paged; filter by archetype or tag; attributes in human units, effects, machine state) or delete one.".into(),
            input_schema: obj(
                vec![
                    ("action", t("string", "list (default), inspect or delete.")),
                    ("name", t("string", "The run's name (inspect, delete).")),
                    ("archetype", t("string", "inspect: only this archetype.")),
                    ("tag", t("string", "inspect: only entities with this tag.")),
                    ("cursor", t("integer", "inspect: next_cursor from the previous page.")),
                    ("limit", t("integer", "inspect: entities per page, 1..200 (default 50).")),
                ],
                &[],
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_paths_set_and_remove() {
        let mut j = Json::parse(r#"{"a": {"b": [1, {"c": 2}]}}"#).unwrap();
        json_set(&mut j, "a.b[1].c", Json::from(3)).unwrap();
        json_set(&mut j, "a.d.e", Json::from("x")).unwrap();
        json_set(&mut j, "a.b[2]", Json::from(9)).unwrap();
        assert_eq!(j.to_string(), r#"{"a":{"b":[1,{"c":3},9],"d":{"e":"x"}}}"#);
        json_remove(&mut j, "a.b[0]").unwrap();
        json_remove(&mut j, "a.d").unwrap();
        assert_eq!(j.to_string(), r#"{"a":{"b":[{"c":3},9]}}"#);
        assert!(json_set(&mut j, "a.b[5]", Json::Null).unwrap_err().contains("past the end"));
        assert!(json_remove(&mut j, "a.zz").unwrap_err().contains("no field"));
        assert!(path_parts("a..b").is_err() && path_parts("a[x]").is_err() && path_parts("a[1").is_err());
    }

    use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
    use crate::journal::UndoRequest;
    use crate::world::Recording;

    const ANN: Writer = Writer { name: "ann", session: Some("aaaaaa11") };
    const BOB: Writer = Writer { name: "bob", session: Some("bbbbbb22") };

    fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() }
    }

    fn setup() -> (World, Swarm) {
        let (s1, s2) = (session("aaaaaa11"), session("bbbbbb22"));
        let live = vec![s1.clone(), s2.clone()];
        let mut sw = Swarm::default();
        sw.hello(&s1, "ann".into(), "builder".into(), None, &live).unwrap();
        sw.hello(&s2, "bob".into(), "builder".into(), None, &live).unwrap();
        let mut w = World::default();
        w.set_recording(Recording::Full);
        (w, sw)
    }

    fn edit_of(set: &str, ops: Vec<EditOp>) -> Edit {
        Edit { set: set.into(), header: Header { tick_hz: Some(30), ..Default::default() }, ops, allow_errors: false, dry_run: false, replace: false, tool: "lex_edit" }
    }

    fn put(id: &str, item: &str) -> EditOp {
        EditOp::Put { id: id.into(), item: Json::parse(item).unwrap(), expect: None }
    }

    /// The engine's store, minus the threads: snapshots and commits straight on the world.
    struct TestStore<'a> {
        w: &'a mut World,
        sw: &'a mut Swarm,
        who: Writer<'a>,
        /// Before the first commit, another agent changes the set (to exercise the retry).
        interfere: bool,
    }

    impl Store for TestStore<'_> {
        fn snapshot(&mut self, set: &str) -> Result<Json, ToolOutput> {
            Ok(snapshot_json(self.w, set, None))
        }

        fn commit(&mut self, c: Commit) -> ToolOutput {
            if std::mem::take(&mut self.interfere) {
                let base = versions(self.w, &c.set);
                let other = Commit { set: c.set.clone(), base, header: None, puts: vec![("intruder".into(), Json::parse(r#"{"kind": "resource", "start": 1}"#).unwrap(), true)], deletes: vec![] };
                assert!(!handle(LexCall::Commit(other), self.w, self.sw, 0, BOB, "bob").is_error);
            }
            handle(LexCall::Commit(c), self.w, self.sw, 0, self.who, self.who.name)
        }
    }

    fn go(w: &mut World, sw: &mut Swarm, who: Writer, e: Edit) -> (bool, Json) {
        let out = edit_in(&mut TestStore { w, sw, who, interfere: false }, &e);
        (out.is_error, output_json(&out))
    }

    fn snap(w: &World, set: &str) -> Json {
        snapshot_json(w, set, None)
    }

    fn item(w: &World, set: &str, id: &str) -> Option<(u64, Json)> {
        let s = snap(w, set);
        let it = s.get("items")?.as_array()?.iter().find(|i| i.get("id").and_then(Json::as_str) == Some(id))?.clone();
        Some((it.get("version")?.as_u64()?, it.get("item")?.clone()))
    }

    fn ids(w: &World, set: &str) -> Vec<String> {
        snap(w, set).get("items").and_then(Json::as_array).unwrap_or_default().iter().filter_map(|i| i.get("id").and_then(Json::as_str).map(str::to_owned)).collect()
    }

    #[test]
    fn edits_are_validated_versioned_journaled_and_undoable() {
        let (mut w, mut sw) = setup();
        let (bad, out) = go(&mut w, &mut sw, ANN, edit_of("rules", vec![put("gold", r#"{"kind": "resource", "start": 5}"#), put("win", r#"{"kind": "outcome", "result": "won", "when": "res.gold >= 10"}"#)]));
        assert!(!bad, "{out}");
        assert_eq!(out.get("valid"), Some(&Json::Bool(true)));
        assert!(out.get("journal").and_then(Json::as_u64).is_some(), "journaled: {out}");
        assert_eq!(item(&w, "rules", "gold").map(|(v, _)| v), Some(1));
        assert_eq!(snap(&w, "rules").get("header").and_then(|h| h.get("tick_hz")).and_then(Json::as_u64), Some(30));

        // An edit that adds an error changes nothing and says what to fix.
        let (bad, out) = go(&mut w, &mut sw, ANN, edit_of("rules", vec![put("win", r#"{"kind": "outcome", "result": "won", "when": "res.gld >= 10"}"#)]));
        assert!(bad);
        assert_eq!(out.get("error").and_then(Json::as_str), Some("invalid"));
        let d = &out.get("diagnostics").and_then(Json::as_array).unwrap()[0];
        assert_eq!(d.get("suggestion").and_then(Json::as_str), Some("did you mean 'gold'?"));
        assert_eq!(item(&w, "rules", "win").map(|(v, _)| v), Some(1), "unchanged");

        // A version conflict, and a patch with the right version.
        let patch = |v: u64, start: i64| EditOp::Patch { id: "gold".into(), set: vec![("start".into(), Json::from(start))], remove: vec![], expect: Some(v) };
        let (bad, out) = go(&mut w, &mut sw, BOB, edit_of("rules", vec![patch(7, 6)]));
        assert!(bad);
        assert_eq!((out.get("error").and_then(Json::as_str), out.get("version").and_then(Json::as_u64)), (Some("version_conflict"), Some(1)));
        let (bad, out) = go(&mut w, &mut sw, BOB, edit_of("rules", vec![patch(1, 6)]));
        assert!(!bad, "{out}");
        assert_eq!(item(&w, "rules", "gold").map(|(v, j)| (v, j.get("start").and_then(Json::as_i64))), Some((2, Some(6))));

        // Unknown items get the closest id.
        let (_, out) = go(&mut w, &mut sw, BOB, edit_of("rules", vec![EditOp::Delete { id: "golf".into(), expect: None }]));
        assert_eq!(out.get("suggestion").and_then(Json::as_str), Some("did you mean 'gold'?"));

        // world_undo undoes it.
        sw.undo(&mut w, 1, BOB, &UndoRequest { ids: None, count: 1, force: false }).unwrap();
        assert_eq!(item(&w, "rules", "gold").and_then(|(_, j)| j.get("start").and_then(Json::as_i64)), Some(5));

        // A draft with errors can be stored on purpose, then fixed step by step.
        let mut draft = edit_of("rules", vec![put("lose", r#"{"kind": "outcome", "result": "lost", "when": "res.lives <= 0"}"#)]);
        draft.allow_errors = true;
        let (bad, out) = go(&mut w, &mut sw, ANN, draft);
        assert!(!bad);
        assert_eq!(out.get("valid"), Some(&Json::Bool(false)));
        let (bad, out) = go(&mut w, &mut sw, ANN, edit_of("rules", vec![put("note", r#"{"kind": "resource", "start": 0}"#)]));
        assert!(!bad, "an edit that adds no new error is accepted while old ones remain: {out}");
        let (bad, out) = go(&mut w, &mut sw, ANN, edit_of("rules", vec![put("lives", r#"{"kind": "resource", "start": 3}"#)]));
        assert!(!bad);
        assert_eq!(out.get("valid"), Some(&Json::Bool(true)), "fixed: {out}");
    }

    #[test]
    fn claims_protect_rule_sets_and_imports_replace() {
        let (mut w, mut sw) = setup();
        go(&mut w, &mut sw, ANN, edit_of("rules", vec![put("gold", r#"{"kind": "resource", "start": 5}"#)]));
        let spec = ClaimSpec { scope: Scope::path("/lex/rules").unwrap(), lease: DEFAULT_LEASE, note: String::new() };
        sw.take_claim(&w, &session("aaaaaa11"), spec).unwrap();
        let (bad, out) = go(&mut w, &mut sw, BOB, edit_of("rules", vec![put("gems", r#"{"kind": "resource", "start": 0}"#)]));
        assert!(bad);
        assert_eq!(out.get("error").and_then(Json::as_str), Some("claimed"), "{out}");
        assert!(item(&w, "rules", "gems").is_none());
        // Bob can still build his own set.
        let (bad, _) = go(&mut w, &mut sw, BOB, edit_of("bobs", vec![put("gems", r#"{"kind": "resource", "start": 0}"#)]));
        assert!(!bad);
        // An import replaces the set's items.
        let mut imp = edit_of("bobs", vec![put("coins", r#"{"kind": "resource", "start": 1}"#)]);
        imp.replace = true;
        let (bad, out) = go(&mut w, &mut sw, BOB, imp);
        assert!(!bad, "{out}");
        assert_eq!(ids(&w, "bobs"), ["coins"]);
        let sets = sets_json(&w, None, 100).to_string();
        assert!(sets.contains("\"set\":\"bobs\"") && sets.contains("\"set\":\"rules\""), "{sets}");
        let page = sets_json(&w, None, 1);
        assert_eq!(page.get("next_cursor").and_then(Json::as_str), Some("bobs"), "paged");
    }

    #[test]
    fn a_commit_against_a_changed_set_is_redone() {
        let (mut w, mut sw) = setup();
        go(&mut w, &mut sw, ANN, edit_of("rules", vec![put("gold", r#"{"kind": "resource", "start": 5}"#)]));
        let stale = Commit { set: "rules".into(), base: None, header: None, puts: vec![], deletes: vec![] };
        let out = handle(LexCall::Commit(stale), &mut w, &mut sw, 0, ANN, "ann");
        assert_eq!(output_json(&out).get("error").and_then(Json::as_str), Some("set_changed"));
        // Another agent's commit lands between this edit's validation and its commit: it is
        // validated again on the new state and both survive.
        let e = edit_of("rules", vec![put("gems", r#"{"kind": "resource", "start": 2}"#)]);
        let out = edit_in(&mut TestStore { w: &mut w, sw: &mut sw, who: ANN, interfere: true }, &e);
        assert!(!out.is_error, "{}", output_json(&out));
        assert_eq!(ids(&w, "rules"), ["gems", "gold", "intruder"]);
    }

    #[test]
    fn an_item_deleted_and_put_again_is_a_change() {
        let (mut w, mut sw) = setup();
        go(&mut w, &mut sw, ANN, edit_of("rules", vec![put("x", r#"{"kind": "resource", "start": 1}"#)]));
        let stale = versions(&w, "rules");
        // Bob deletes x and puts a different x: it restarts at version 1, as a new entity.
        go(&mut w, &mut sw, BOB, edit_of("rules", vec![EditOp::Delete { id: "x".into(), expect: None }]));
        go(&mut w, &mut sw, BOB, edit_of("rules", vec![put("x", r#"{"kind": "resource", "start": 2}"#)]));
        assert_eq!(item(&w, "rules", "x").map(|(v, _)| v), Some(1));
        let late = Commit { set: "rules".into(), base: stale, header: None, puts: vec![("x".into(), Json::parse(r#"{"kind": "resource", "start": 9}"#).unwrap(), false)], deletes: vec![] };
        let out = handle(LexCall::Commit(late), &mut w, &mut sw, 0, ANN, "ann");
        assert_eq!(output_json(&out).get("error").and_then(Json::as_str), Some("set_changed"), "the id gives the old snapshot away");
        assert_eq!(item(&w, "rules", "x").and_then(|(_, j)| j.get("start").and_then(Json::as_i64)), Some(2), "Bob's x survives");
    }

    #[test]
    fn damaged_sets_are_still_validated_and_can_be_fixed() {
        let (mut w, mut sw) = setup();
        go(&mut w, &mut sw, ANN, edit_of("s", vec![put("gold", r#"{"kind": "resource", "start": 5}"#)]));
        // A badly named item slipped in through world_spawn.
        let mut bad = SpawnSpec::new(Shape::Group);
        bad.name = Some("Bad".into());
        bad.parent = Some(EntityKey::Path("/lex/s".into()));
        w.spawn(bad, "x").unwrap();
        let (refused, out) = go(&mut w, &mut sw, ANN, edit_of("s", vec![put("win", r#"{"kind": "outcome", "result": "won", "when": "res.gld >= 10 +"}"#)]));
        assert!(refused, "the new syntax error is still caught: {out}");
        assert!(out.get("diagnostics").unwrap().to_string().contains("syntax"));
        let (refused, out) = go(&mut w, &mut sw, ANN, edit_of("s", vec![EditOp::Delete { id: "Bad".into(), expect: None }]));
        assert!(!refused, "the bad item can be deleted: {out}");
        assert_eq!(out.get("valid"), Some(&Json::Bool(true)));
    }

    #[test]
    fn errors_past_the_report_cap_still_count() {
        let (mut w, mut sw) = setup();
        let mut ops = vec![put("gold", r#"{"kind": "resource", "start": 5}"#)];
        for i in 0..250 {
            ops.push(put(&format!("o{i:03}"), r#"{"kind": "outcome", "result": "won", "when": "res.gld >= 10"}"#));
        }
        let mut draft = edit_of("big", ops);
        draft.allow_errors = true;
        assert!(!go(&mut w, &mut sw, ANN, draft).0);
        let fix = edit_of("big", vec![put("o000", r#"{"kind": "outcome", "result": "won", "when": "res.gold >= 10"}"#)]);
        let (refused, out) = go(&mut w, &mut sw, ANN, fix);
        assert!(!refused, "fixing one of 250 errors adds none: {out}");
    }

    #[test]
    fn definitions_are_well_formed() {
        for d in definitions() {
            assert!(d.name.starts_with("lex_") && d.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
            assert!(d.input_schema.get("properties").is_some());
            assert!(!d.description.contains("   "));
        }
    }
}
