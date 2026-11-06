//! The task board and notes: how a swarm shares intent, progress and handoffs.
//!
//! Claims protect data; the board shares *what is being done*, so two agents don't both build
//! the tavern and a parent agent reads status instead of guessing from the change log.
//!
//! **Tasks.** `task_post` puts a task on the board (title, detail, dependencies, the paths,
//! entities and claims it is about, an optional assignee). An agent `task_take`s an open task
//! whose dependencies are all finished (done, or cancelled: a cancelled dependency won't be done,
//! so the poster replaces it if it mattered; an assigned task only by its assignee), reports with
//! `task_update` (progress, `blocked` with a blocker, questions and answers, more links) and
//! finishes with `task_done` (a summary) or cancels it. Statuses: `open` → `taken` ⇄ `blocked`
//! → `done` / `cancelled`; the owner (or the poster) can give a task back (`open`).
//!
//! - **Linked to the journal.** The task an agent took last (still taken or blocked) is its
//!   *current* task: every journal entry it makes while that holds is linked to the task, claims
//!   it takes are added to the task's links, and refused writes count against it. That is what
//!   `world_rollup {by: "task"}` reads.
//! - **Who may do what.** Introduced agents only (`agent_hello`). Owners, posters and note
//!   authors are a name *and* an identity, like claims and undo: a name taken again after its
//!   session ended adopts nothing (a resumed agent keeps its identity). The
//!   owner changes status and progress; the poster edits the task (title, detail, assignee,
//!   dependencies, priority) and may cancel or reopen it; anyone may ask or answer a question,
//!   or add links. `expect_version` refuses an update when the task changed since it was read.
//! - **Recorded.** Every board change is a [`BoardEvent`] with a sequence number, the tick and
//!   its author: the last 64 per task are kept on the task, and `task_list {changed_since}` is a
//!   feed of tasks changed after a sequence number.
//! - **Persisted with the world**: `board.json` next to `world.json` (written when the world is
//!   saved, and by autosave on a worker thread, re-rendering only tasks that changed), read back
//!   with the world. Identities are per run, so loaded owners, posters and note authors come back
//!   by name: whichever session holds the name, until the owner or poster acts on the task again
//!   and binds it. `world_load` keeps the outgoing world's board with it first, and refuses
//!   (`unsaved_changes`) when it can't.
//! - **Bounded.** At most [`MAX_TASKS`] tasks (the oldest done or cancelled ones make room),
//!   [`MAX_NOTES`] notes, and every text and list has a cap; listings are paged by bytes.
//!
//! **Notes** are handoffs left on an entity, a path or a task for whoever comes next ("the east
//! gate's hinges are placeholders"). They outlive sessions (they are saved with the board).
//!
//! **Entities** (a note's, a task's) are [`EntityRef`]s: the entity's identity, never a bare id
//! or a path, so they survive renames, moves and world replacements (a save read keeps the
//! board) and never land on another entity; a deleted or absent one shows as detached with its
//! last known path (docs/design/EDITOR.md, "Stable entity identity").
//!
//! Nothing here is simulation state: the board never changes the world, so it is not part of
//! the state hash. Ticks and sequence numbers order it; no wall-clock time is stored.

use crate::entity_ref::{EntityRef, Prefix};
use crate::journal::{path_under, Writer};
use crate::world::{World, WorldError};
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Tasks kept (open and finished); past this the oldest finished ones are dropped.
pub const MAX_TASKS: usize = 8192;
/// Notes kept; past this the oldest are dropped.
pub const MAX_NOTES: usize = 8192;
/// Events kept on each task (older ones are counted, not kept).
pub const MAX_TASK_EVENTS: usize = 64;
/// Board events kept for the feed.
const MAX_EVENTS: usize = 4096;
pub const MAX_TITLE: usize = 200;
pub const MAX_DETAIL: usize = 4000;
pub const MAX_TEXT: usize = 2000;
/// Dependencies, paths, entities, claims and tags per task.
pub const MAX_LINKS: usize = 64;
/// Questions per task.
pub const MAX_QUESTIONS: usize = 16;
/// A question's or an answer's text.
pub const MAX_QA_TEXT: usize = 1000;
/// The whole board's text (tasks and notes, roughly as saved): about 24 MiB, so saving, loading
/// and the file stay bounded whatever agents post. Past it, edits that add text are refused
/// with `board_full` (posting first drops the oldest finished tasks to make room).
pub const MAX_BOARD_BYTES: usize = 24 << 20;
/// Journal entries linked to tasks that are remembered (the journal's own bound).
const MAX_ENTRY_LINKS: usize = crate::journal::MAX_ENTRIES;
/// Listings stop adding items past this many bytes and hand back a cursor.
const MAX_LIST_BYTES: usize = 64 * 1024;
pub const FORMAT: &str = "specula.board/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Open,
    Taken,
    Blocked,
    Done,
    Cancelled,
}

impl Status {
    pub const ALL: [Status; 5] = [Status::Open, Status::Taken, Status::Blocked, Status::Done, Status::Cancelled];

    pub fn name(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Taken => "taken",
            Status::Blocked => "blocked",
            Status::Done => "done",
            Status::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|x| x.name() == s)
    }

    /// Done or cancelled.
    pub fn finished(self) -> bool {
        matches!(self, Status::Done | Status::Cancelled)
    }

    /// Someone is on it (taken or blocked).
    pub fn active(self) -> bool {
        matches!(self, Status::Taken | Status::Blocked)
    }
}

/// An agent name, and (within this engine run) the identity key that holds it. A task loaded
/// from a file has no key: whichever session holds the name now is the owner.
#[derive(Clone, Debug, PartialEq)]
pub struct Owner {
    pub name: String,
    pub key: Option<String>,
}

impl Owner {
    fn of(w: Writer) -> Owner {
        Owner { name: w.name.to_owned(), key: w.session.map(str::to_owned) }
    }

    pub fn is(&self, w: Writer) -> bool {
        self.name == w.name && self.key.as_deref().is_none_or(|k| w.session == Some(k))
    }
}

/// One change on the board.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardEvent {
    pub seq: u64,
    pub tick: u64,
    pub by: String,
    /// `post`, `take`, `update`, `progress`, `blocked`, `unblocked`, `question`, `answer`,
    /// `assign`, `reopen`, `done`, `cancel`, `link`, `note`, `note_delete`; and, for the feed,
    /// `intent`, `intent_clear`, `lease`, `lease_release`, `settings`; the review queue's
    /// `review` and `critique`.
    pub what: &'static str,
    pub task: Option<u64>,
    /// A short text (a progress line, the blocker, the question), at most 200 characters.
    pub text: String,
}

impl BoardEvent {
    pub fn to_json(&self) -> Json {
        let mut f = vec![
            ("seq", Json::from(self.seq)),
            ("tick", Json::from(self.tick)),
            ("by", Json::from(self.by.as_str())),
            ("what", Json::from(self.what)),
        ];
        if let Some(t) = self.task {
            f.push(("task", Json::from(t)));
        }
        if !self.text.is_empty() {
            f.push(("text", Json::from(self.text.as_str())));
        }
        Json::obj(f)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub id: u32,
    pub by: String,
    pub text: String,
    /// `(by, text)` once answered.
    pub answer: Option<(String, String)>,
}

/// Roll-up counters a task gathers while it is someone's current task.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TaskWork {
    pub entries: u64,
    pub changes: u64,
    pub refused: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub detail: String,
    pub status: Status,
    pub posted_by: String,
    /// The identity key that posted it (`None` for a task loaded from a file: whichever session
    /// holds the name, until the poster acts again and binds it).
    pub poster_key: Option<String>,
    pub owner: Option<Owner>,
    /// Only this agent may take it.
    pub assignee: Option<String>,
    pub parent: Option<u64>,
    pub deps: Vec<u64>,
    pub paths: Vec<String>,
    pub entities: Vec<EntityRef>,
    pub claims: Vec<u64>,
    pub tags: Vec<String>,
    /// 0 (lowest) to 9; 5 by default.
    pub priority: u8,
    /// The owner's latest progress line, and the blocker while blocked.
    pub progress: String,
    pub blocker: String,
    /// `task_done`'s summary.
    pub result: String,
    pub questions: Vec<Question>,
    pub events: VecDeque<BoardEvent>,
    pub events_dropped: u64,
    pub created_tick: u64,
    pub updated_tick: u64,
    /// The board sequence number of its last change (`task_list {changed_since}`).
    pub updated_seq: u64,
    /// Bumped by every change (`expect_version`).
    pub version: u64,
    pub work: TaskWork,
    /// Its text's size as last measured (see [`MAX_BOARD_BYTES`]); not saved.
    pub size: usize,
}

impl Task {
    /// Its text's size, roughly as saved (strings plus a little per item).
    pub fn measure(&self) -> usize {
        let strs = |v: &[String]| v.iter().map(|x| x.len() + 8).sum::<usize>();
        512 + self.title.len()
            + self.detail.len()
            + self.progress.len()
            + self.blocker.len()
            + self.result.len()
            + self.questions.iter().map(|q| 64 + q.by.len() + q.text.len() + q.answer.as_ref().map_or(0, |(b, t)| b.len() + t.len() + 32)).sum::<usize>()
            + self.events.iter().map(|e| 96 + e.by.len() + e.text.len()).sum::<usize>()
            + strs(&self.paths)
            + strs(&self.tags)
            + self.entities.iter().map(|e| e.path.len() + 32).sum::<usize>()
            + 24 * (self.deps.len() + self.claims.len())
    }

    pub fn open_questions(&self) -> usize {
        self.questions.iter().filter(|q| q.answer.is_none()).count()
    }

    /// Its entities as shown against `world` (attached ones with their id and current path:
    /// refresh them first, [`Board::refresh_task_entities`]).
    pub fn entities_json(&self, world: &World) -> Json {
        Json::arr(self.entities.iter().map(|e| e.json(world)))
    }

    /// The compact listing item.
    pub fn summary_json(&self) -> Json {
        let mut f = vec![
            ("id", Json::from(self.id)),
            ("title", Json::from(self.title.as_str())),
            ("status", Json::from(self.status.name())),
            ("priority", Json::from(u64::from(self.priority))),
            ("posted_by", Json::from(self.posted_by.as_str())),
            ("version", Json::from(self.version)),
        ];
        if let Some(o) = &self.owner {
            f.push(("owner", Json::from(o.name.as_str())));
        }
        if let Some(a) = &self.assignee {
            f.push(("assignee", Json::from(a.as_str())));
        }
        if !self.deps.is_empty() {
            f.push(("deps", Json::arr(self.deps.iter().map(|d| Json::from(*d)))));
        }
        if !self.paths.is_empty() {
            f.push(("paths", Json::arr(self.paths.iter().take(4).map(|p| Json::from(p.as_str())))));
        }
        if !self.progress.is_empty() {
            f.push(("progress", Json::from(short(&self.progress, 120))));
        }
        if self.status == Status::Blocked {
            f.push(("blocker", Json::from(short(&self.blocker, 120))));
        }
        let open = self.open_questions();
        if open > 0 {
            f.push(("open_questions", Json::from(open)));
        }
        f.push(("updated_seq", Json::from(self.updated_seq)));
        Json::obj(f)
    }

    /// Everything, with the last `events` events and the last `questions` questions (all with
    /// `None`; the file keeps everything).
    pub fn to_json(&self, events: Option<usize>) -> Json {
        self.to_json_with(events, None)
    }

    /// [`Task::to_json`] with at most `questions` questions (the newest; open ones first).
    pub fn to_json_with(&self, events: Option<usize>, questions: Option<usize>) -> Json {
        let mut j = self.summary_json();
        let ids = |v: &[u64]| Json::arr(v.iter().map(|x| Json::from(*x)));
        let strs = |v: &[String]| Json::arr(v.iter().map(|x| Json::from(x.as_str())));
        j.set("detail", Json::from(self.detail.as_str()));
        if let Some(p) = self.parent {
            j.set("parent", Json::from(p));
        }
        j.set("paths", strs(&self.paths));
        // As saved; task_get shows them against the world (`Task::entities_json`).
        j.set("entities", Json::arr(self.entities.iter().map(EntityRef::saved_json)));
        j.set("claims", ids(&self.claims));
        j.set("tags", strs(&self.tags));
        j.set("progress", Json::from(self.progress.as_str()));
        if !self.blocker.is_empty() {
            j.set("blocker", Json::from(self.blocker.as_str()));
        }
        if !self.result.is_empty() {
            j.set("result", Json::from(self.result.as_str()));
        }
        let shown: Vec<&Question> = match questions {
            None => self.questions.iter().collect(),
            Some(n) => {
                let mut v: Vec<&Question> = self.questions.iter().filter(|q| q.answer.is_none()).collect();
                v.extend(self.questions.iter().rev().filter(|q| q.answer.is_some()));
                v.truncate(n);
                v.sort_by_key(|q| q.id);
                v
            }
        };
        if shown.len() < self.questions.len() {
            j.set("questions_total", Json::from(self.questions.len()));
        }
        j.set(
            "questions",
            Json::arr(shown.into_iter().map(|q| {
                let mut f = vec![("id", Json::from(u64::from(q.id))), ("by", Json::from(q.by.as_str())), ("text", Json::from(q.text.as_str()))];
                if let Some((by, text)) = &q.answer {
                    f.push(("answer", Json::obj([("by", Json::from(by.as_str())), ("text", Json::from(text.as_str()))])));
                }
                Json::obj(f)
            })),
        );
        j.set("created_tick", Json::from(self.created_tick));
        j.set("updated_tick", Json::from(self.updated_tick));
        j.set(
            "work",
            Json::obj([
                ("journal_entries", Json::from(self.work.entries)),
                ("changes", Json::from(self.work.changes)),
                ("refused", Json::from(self.work.refused)),
            ]),
        );
        let n = events.unwrap_or(usize::MAX).min(self.events.len());
        j.set("events", Json::arr(self.events.iter().skip(self.events.len() - n).map(BoardEvent::to_json)));
        if self.events_dropped > 0 {
            j.set("events_dropped", Json::from(self.events_dropped));
        }
        j
    }
}

/// What a note is attached to.
#[derive(Clone, Debug, PartialEq)]
pub enum NoteTarget {
    /// An entity, by identity (see [`EntityRef`]).
    Entity(EntityRef),
    /// A path (whatever is, or will be, there).
    Path(String),
    Task(u64),
}

impl NoteTarget {
    /// As saved (`None`), or as shown against that world.
    fn json(&self, world: Option<&World>) -> (&'static str, Json) {
        match self {
            NoteTarget::Entity(r) => ("entity", world.map_or_else(|| r.saved_json(), |w| r.json(w))),
            NoteTarget::Path(p) => ("path", Json::from(p.as_str())),
            NoteTarget::Task(t) => ("task", Json::from(*t)),
        }
    }

    fn under(&self, world: &World, prefix: &Prefix<'_>) -> bool {
        match self {
            NoteTarget::Entity(r) => r.under(world, prefix),
            NoteTarget::Path(path) => path_under(path, prefix.path),
            NoteTarget::Task(_) => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: u64,
    pub by: String,
    /// The author's identity key (`None` when loaded from a file).
    pub by_key: Option<String>,
    pub target: NoteTarget,
    pub text: String,
    pub tick: u64,
    pub seq: u64,
}

impl Note {
    pub fn measure(&self) -> usize {
        let target = match &self.target {
            NoteTarget::Entity(EntityRef { path, .. }) | NoteTarget::Path(path) => path.len(),
            NoteTarget::Task(_) => 0,
        };
        160 + self.by.len() + self.text.len() + target
    }

    /// As saved in `board.json` (an entity as `{id, born, path}`).
    pub fn to_json(&self) -> Json {
        self.json_with(None)
    }

    /// As agents see it: an entity attached (`{id, path}`, its path as last refreshed) or
    /// detached (`{path, detached: true}`) in `world`.
    pub fn shown(&self, world: &World) -> Json {
        self.json_with(Some(world))
    }

    fn json_with(&self, world: Option<&World>) -> Json {
        Json::obj([
            ("id", Json::from(self.id)),
            ("by", Json::from(self.by.as_str())),
            self.target.json(world),
            ("text", Json::from(self.text.as_str())),
            ("tick", Json::from(self.tick)),
            ("seq", Json::from(self.seq)),
        ])
    }
}

/// What `task_post` asks for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskSpec {
    pub title: String,
    pub detail: String,
    pub deps: Vec<u64>,
    pub paths: Vec<String>,
    /// By identity: the handler makes them from the call's ids ([`EntityRef::of_ids`]).
    pub entities: Vec<EntityRef>,
    pub claims: Vec<u64>,
    pub tags: Vec<String>,
    pub assignee: Option<String>,
    pub parent: Option<u64>,
    pub priority: Option<u8>,
}

/// What `task_update` asks for; every given field applies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskPatch {
    /// `open` (give back), `taken` (unblock) or `blocked` (with `blocker`).
    pub status: Option<Status>,
    pub blocker: Option<String>,
    pub progress: Option<String>,
    pub question: Option<String>,
    /// `(question id, text)`.
    pub answer: Option<(u32, String)>,
    /// Poster only.
    pub title: Option<String>,
    pub detail: Option<String>,
    /// `Some(None)` un-assigns.
    pub assignee: Option<Option<String>>,
    pub priority: Option<u8>,
    pub add_deps: Vec<u64>,
    pub remove_deps: Vec<u64>,
    /// Anyone.
    pub add_paths: Vec<String>,
    pub add_entities: Vec<EntityRef>,
    pub add_claims: Vec<u64>,
    pub add_tags: Vec<String>,
    pub expect_version: Option<u64>,
}

/// `task_list`'s filters.
#[derive(Clone, Debug, Default)]
pub struct TaskQuery {
    pub statuses: Vec<Status>,
    pub owner: Option<String>,
    pub posted_by: Option<String>,
    pub assignee: Option<String>,
    /// Open, all dependencies done, and takeable by `ready_for` (if given).
    pub ready: bool,
    pub ready_for: Option<String>,
    pub path_prefix: Option<String>,
    pub tag: Option<String>,
    pub parent: Option<u64>,
    /// Only tasks changed after this board sequence number.
    pub changed_since: Option<u64>,
    /// Ids after this one (paging).
    pub after: Option<u64>,
    pub limit: usize,
}

/// `note_list`'s filters.
#[derive(Clone, Debug, Default)]
pub struct NoteQuery {
    pub path_prefix: Option<String>,
    pub entity: Option<u64>,
    pub task: Option<u64>,
    pub by: Option<String>,
    pub after: Option<u64>,
    pub limit: usize,
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn err(code: &'static str, message: impl Into<String>) -> WorldError {
    WorldError::new(code, message)
}

fn not_found(id: u64) -> WorldError {
    err("task_not_found", format!("there is no task {id} (see task_list)")).with("task", id)
}

/// Checks a text field: `what` names it in the error.
pub fn check_text(what: &str, s: &str, max: usize, required: bool) -> Result<(), String> {
    if required && s.trim().is_empty() {
        return Err(format!("'{what}' must not be empty"));
    }
    if s.chars().count() > max {
        return Err(format!("'{what}' must be at most {max} characters"));
    }
    if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(format!("'{what}' must not contain control characters other than newlines and tabs"));
    }
    Ok(())
}

/// See the module docs.
#[derive(Default)]
pub struct Board {
    tasks: BTreeMap<u64, Task>,
    notes: BTreeMap<u64, Note>,
    next_task: u64,
    next_note: u64,
    /// The last board event's sequence number.
    seq: u64,
    events: VecDeque<BoardEvent>,
    /// Identity key → the task it works on now (see the module docs).
    current: BTreeMap<String, u64>,
    /// Journal entry → the task it was made for.
    entry_task: BTreeMap<u64, u64>,
    /// Bumped by every change that is saved (the engine saves the board when it moved).
    revision: u64,
    /// The board's text size (tasks' `size` plus notes; see [`MAX_BOARD_BYTES`]).
    bytes: usize,
    /// Rendered `board.json` lines of tasks, by id: (version, line). Only tasks that changed
    /// since the last save are rendered again (F3). The key is the version and the work
    /// counters (journal links change those without a new version).
    rendered: BTreeMap<u64, ((u64, TaskWork), String)>,
    /// Rendered note lines, by id (a note's text never changes once left; a refreshed last
    /// known path of its entity drops the line).
    rendered_notes: BTreeMap<u64, String>,
}

impl Board {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&Task> {
        self.tasks.get(&id)
    }

    pub fn tasks(&self) -> impl Iterator<Item = &Task> + '_ {
        self.tasks.values()
    }

    pub fn notes_len(&self) -> usize {
        self.notes.len()
    }

    /// The task `w` works on now: the one it took last, while still taken or blocked by it.
    pub fn current(&self, w: Writer) -> Option<&Task> {
        let key = w.session?;
        let t = self.tasks.get(self.current.get(key)?)?;
        (t.status.active() && t.owner.as_ref().is_some_and(|o| o.is(w))).then_some(t)
    }

    /// The task journal entry `seq` was made for, if any.
    pub fn task_of_entry(&self, seq: u64) -> Option<u64> {
        self.entry_task.get(&seq).copied()
    }

    /// Records that `w` made journal entry `seq` (of `changes` changes): linked to its current
    /// task.
    pub fn note_entry(&mut self, w: Writer, seq: u64, changes: usize) {
        let Some(id) = self.current(w).map(|t| t.id) else { return };
        if self.entry_task.len() >= MAX_ENTRY_LINKS {
            self.entry_task.pop_first();
        }
        self.entry_task.insert(seq, id);
        if let Some(t) = self.tasks.get_mut(&id) {
            t.work.entries += 1;
            self.revision += 1;
            t.work.changes += changes as u64;
        }
    }

    /// A write by `w` was refused: counted against its current task.
    pub fn note_refused(&mut self, w: Writer) {
        let Some(id) = self.current(w).map(|t| t.id) else { return };
        if let Some(t) = self.tasks.get_mut(&id) {
            t.work.refused += 1;
            self.revision += 1;
        }
    }

    /// `w` took claim `claim`: linked to its current task. Returns the task's id.
    pub fn note_claim(&mut self, w: Writer, claim: u64, tick: u64) -> Option<u64> {
        let id = self.current(w)?.id;
        let t = self.tasks.get_mut(&id)?;
        if t.claims.contains(&claim) || t.claims.len() >= MAX_LINKS {
            return Some(id);
        }
        t.claims.push(claim);
        self.event(w.name, tick, "link", Some(id), format!("claim {claim}"));
        Some(id)
    }

    /// Appends a board event (to the feed and to its task) and returns its sequence number.
    fn event(&mut self, by: &str, tick: u64, what: &'static str, task: Option<u64>, text: String) -> u64 {
        self.seq += 1;
        self.revision += 1;
        let e = BoardEvent { seq: self.seq, tick, by: by.to_owned(), what, task, text: short(&text, 200) };
        if let Some(t) = task.and_then(|id| self.tasks.get_mut(&id)) {
            if t.events.len() >= MAX_TASK_EVENTS {
                t.events.pop_front();
                t.events_dropped += 1;
            }
            t.events.push_back(e.clone());
            t.updated_seq = e.seq;
            t.updated_tick = tick;
            t.version += 1;
            let size = t.measure();
            self.bytes = (self.bytes + size).saturating_sub(t.size);
            t.size = size;
        }
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(e);
        self.seq
    }

    /// Records a board event that isn't a task change (intents, leases, settings).
    pub fn log(&mut self, by: &str, tick: u64, what: &'static str, task: Option<u64>, text: String) {
        let task = task.filter(|t| self.tasks.contains_key(t));
        let revision = self.revision;
        self.event(by, tick, what, task, text);
        if task.is_none() {
            // Feed only: nothing saved changed.
            self.revision = revision;
        }
    }

    /// The board's text size now (see [`MAX_BOARD_BYTES`]).
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Refuses adding about `more` bytes of text past [`MAX_BOARD_BYTES`].
    fn check_budget(&self, more: usize) -> Result<(), WorldError> {
        if self.bytes + more <= MAX_BOARD_BYTES {
            return Ok(());
        }
        Err(err(
            "board_full",
            format!(
                "the board holds about {} MiB of text, its limit is {} MiB: finish or cancel tasks (posting drops the oldest \
                 finished ones), delete notes (note_delete), or keep texts shorter",
                self.bytes >> 20,
                MAX_BOARD_BYTES >> 20
            ),
        )
        .with("bytes", self.bytes)
        .with("adding", more)
        .with("budget", MAX_BOARD_BYTES))
    }

    fn remove_task(&mut self, id: u64) {
        if let Some(t) = self.tasks.remove(&id) {
            self.bytes = self.bytes.saturating_sub(t.size);
        }
    }

    /// Makes room for a task of about `size` bytes: drops the oldest finished tasks while the board
    /// is at its count or byte limit. Refused when every kept task is still open or in progress.
    fn make_room(&mut self, size: usize) -> Result<(), WorldError> {
        while self.bytes + size > MAX_BOARD_BYTES {
            let Some(old) = self.tasks.values().find(|t| t.status.finished()).map(|t| t.id) else { break };
            self.remove_task(old);
        }
        self.check_budget(size)?;
        if self.tasks.len() < MAX_TASKS {
            return Ok(());
        }
        let Some(old) = self.tasks.values().find(|t| t.status.finished()).map(|t| t.id) else {
            return Err(err(
                "too_many_tasks",
                format!("the board holds {MAX_TASKS} unfinished tasks; finish or cancel some first"),
            )
            .with("tasks", self.tasks.len()));
        };
        self.remove_task(old);
        Ok(())
    }

    fn check_deps(&self, deps: &[u64]) -> Result<(), WorldError> {
        if let Some(d) = deps.iter().find(|d| !self.tasks.contains_key(d)) {
            return Err(err("task_not_found", format!("dependency {d} is not a task on the board")).with("task", *d));
        }
        Ok(())
    }

    /// `task_post`.
    pub fn post(&mut self, w: Writer, tick: u64, spec: TaskSpec) -> Result<&Task, WorldError> {
        self.check_deps(&spec.deps)?;
        if let Some(p) = spec.parent.filter(|p| !self.tasks.contains_key(p)) {
            return Err(err("task_not_found", format!("parent {p} is not a task on the board")).with("task", p));
        }
        let size = 1024
            + spec.title.len()
            + spec.detail.len()
            + spec.paths.iter().chain(&spec.tags).map(|x| x.len() + 8).sum::<usize>()
            + spec.entities.iter().map(|e| e.path.len() + 32).sum::<usize>();
        self.make_room(size)?;
        self.next_task += 1;
        let id = self.next_task;
        let task = Task {
            id,
            title: spec.title,
            detail: spec.detail,
            status: Status::Open,
            posted_by: w.name.to_owned(),
            poster_key: w.session.map(str::to_owned),
            owner: None,
            assignee: spec.assignee,
            parent: spec.parent,
            deps: dedup(spec.deps),
            paths: dedup(spec.paths),
            entities: dedup(spec.entities),
            claims: dedup(spec.claims),
            tags: dedup(spec.tags),
            priority: spec.priority.unwrap_or(5),
            progress: String::new(),
            blocker: String::new(),
            result: String::new(),
            questions: Vec::new(),
            events: VecDeque::new(),
            events_dropped: 0,
            created_tick: tick,
            updated_tick: tick,
            updated_seq: 0,
            version: 0,
            work: TaskWork::default(),
            size: 0,
        };
        let title = task.title.clone();
        self.tasks.insert(id, task);
        self.event(w.name, tick, "post", Some(id), title);
        Ok(&self.tasks[&id])
    }

    fn check_version(t: &Task, expect: Option<u64>) -> Result<(), WorldError> {
        match expect {
            Some(v) if v != t.version => Err(err(
                "stale_task",
                format!("task {} is at version {}, not {v}: it changed since you read it; re-read it (task_get) and retry", t.id, t.version),
            )
            .with("task", t.id)
            .with("expected_version", v)
            .with("current_version", t.version)
            .with("current", t.summary_json())),
            _ => Ok(()),
        }
    }

    /// `task_take`: `w` becomes the owner and this is its current task.
    pub fn take(&mut self, w: Writer, tick: u64, id: u64, expect: Option<u64>) -> Result<&Task, WorldError> {
        let t = self.tasks.get(&id).ok_or_else(|| not_found(id))?;
        Self::check_version(t, expect)?;
        match (t.status, &t.owner) {
            (s, Some(o)) if s.active() && o.is(w) => {
                // Taking your own task again makes it current (and binds a loaded owner).
                if let Some(k) = w.session {
                    self.current.insert(k.to_owned(), id);
                }
                self.bind(w, id);
                return Ok(&self.tasks[&id]);
            }
            (s, Some(o)) if s.active() => {
                return Err(err("task_taken", format!("task {id} is already {} by {}; pick another (task_list {{ready: true}})", s.name(), o.name))
                    .with("task", id)
                    .with("owner", o.name.as_str()));
            }
            (s, _) if s.finished() => {
                return Err(err("task_finished", format!("task {id} is {}; post a new task to do more", s.name())).with("task", id));
            }
            _ => {}
        }
        if let Some(a) = t.assignee.as_ref().filter(|a| *a != w.name) {
            return Err(err("assigned_elsewhere", format!("task {id} is assigned to {a}; only they may take it"))
                .with("task", id)
                .with("assignee", a.as_str()));
        }
        let pending: Vec<Json> = t
            .deps
            .iter()
            .filter_map(|d| self.tasks.get(d))
            .filter(|d| !d.status.finished())
            .map(|d| Json::obj([("id", Json::from(d.id)), ("title", Json::from(d.title.as_str())), ("status", Json::from(d.status.name()))]))
            .collect();
        if !pending.is_empty() {
            return Err(err(
                "deps_not_done",
                format!("task {id} depends on {} task(s) that aren't finished yet; take one of those, or wait", pending.len()),
            )
            .with("task", id)
            .with("pending", Json::Arr(pending)));
        }
        let t = self.tasks.get_mut(&id).expect("checked");
        t.status = Status::Taken;
        t.owner = Some(Owner::of(w));
        t.blocker.clear();
        if let Some(k) = w.session {
            self.current.insert(k.to_owned(), id);
        }
        self.event(w.name, tick, "take", Some(id), String::new());
        Ok(&self.tasks[&id])
    }

    /// Whether `w` owns task `t` (taken or blocked by it).
    fn owns(t: &Task, w: Writer) -> bool {
        t.owner.as_ref().is_some_and(|o| o.is(w)) && t.status.active()
    }

    /// Whether `w` posted task `t`: the same name *and* identity (a name taken again after its
    /// session ended adopts nothing), or the name alone for a task loaded from a file.
    fn posted(t: &Task, w: Writer) -> bool {
        t.posted_by == w.name && t.poster_key.as_deref().is_none_or(|k| w.session == Some(k))
    }

    /// After `w` acted on task `id` as its owner or poster: a loaded task's name-only owner or
    /// poster is bound to `w`'s identity from now on.
    fn bind(&mut self, w: Writer, id: u64) {
        let Some(t) = self.tasks.get_mut(&id) else { return };
        if let Some(o) = t.owner.as_mut().filter(|o| o.key.is_none() && o.name == w.name) {
            o.key = w.session.map(str::to_owned);
        }
        if t.poster_key.is_none() && t.posted_by == w.name {
            t.poster_key = w.session.map(str::to_owned);
        }
    }

    fn would_cycle(&self, id: u64, add: &[u64]) -> Option<u64> {
        // A cycle exists when `id` is reachable from one of the new dependencies.
        let mut stack: Vec<u64> = add.to_vec();
        let mut seen = BTreeSet::new();
        while let Some(d) = stack.pop() {
            if d == id {
                return Some(d);
            }
            if seen.insert(d) && seen.len() <= MAX_TASKS {
                if let Some(t) = self.tasks.get(&d) {
                    stack.extend(t.deps.iter().copied());
                }
            }
        }
        None
    }

    /// `task_update`: see [`TaskPatch`] for who may change what. All or nothing.
    pub fn update(&mut self, w: Writer, tick: u64, id: u64, p: TaskPatch) -> Result<&Task, WorldError> {
        let t = self.tasks.get(&id).ok_or_else(|| not_found(id))?;
        Self::check_version(t, p.expect_version)?;
        let owner = Self::owns(t, w);
        let poster = Self::posted(t, w);
        let poster_only = p.title.is_some() || p.detail.is_some() || p.assignee.is_some() || p.priority.is_some() || !p.add_deps.is_empty() || !p.remove_deps.is_empty();
        if poster_only && !poster {
            return Err(err(
                "not_poster",
                format!("only {} (who posted task {id}) may change its title, detail, assignee, priority or dependencies", t.posted_by),
            )
            .with("task", id)
            .with("posted_by", t.posted_by.as_str()));
        }
        if p.progress.is_some() && !owner {
            return Err(not_owner(t, "report progress on"));
        }
        if let Some(s) = p.status {
            let ok = match s {
                Status::Blocked | Status::Taken => owner && t.status.active(),
                Status::Open => t.status.active() && (owner || poster),
                Status::Done | Status::Cancelled => false,
            };
            if !ok {
                if matches!(s, Status::Done | Status::Cancelled) {
                    return Err(err("bad_status_change", "finish a task with task_done (cancel: true to cancel it)").with("task", id));
                }
                if !t.status.active() {
                    return Err(err(
                        "bad_status_change",
                        format!("task {id} is {}; only a taken or blocked task can become {}", t.status.name(), s.name()),
                    )
                    .with("task", id)
                    .with("status", t.status.name()));
                }
                return Err(not_owner(t, "change the status of"));
            }
            if s == Status::Blocked && p.blocker.as_deref().is_none_or(|b| b.trim().is_empty()) {
                return Err(err("blocker_required", "say what blocks the task: give 'blocker' with status 'blocked'").with("task", id));
            }
        }
        if let Some((q, _)) = &p.answer {
            match t.questions.iter().find(|x| x.id == *q) {
                None => return Err(err("question_not_found", format!("task {id} has no question {q}")).with("task", id).with("question", u64::from(*q))),
                Some(x) if x.answer.is_some() => {
                    return Err(err("already_answered", format!("question {q} on task {id} was already answered")).with("task", id))
                }
                _ => {}
            }
        }
        let adding = 256
            + [&p.progress, &p.blocker, &p.question, &p.title, &p.detail].iter().map(|x| x.as_ref().map_or(0, String::len)).sum::<usize>()
            + p.answer.as_ref().map_or(0, |(_, a)| a.len() + 64)
            + p.add_paths.iter().chain(&p.add_tags).map(|x| x.len() + 8).sum::<usize>()
            + p.add_entities.iter().map(|e| e.path.len() + 32).sum::<usize>();
        self.check_budget(adding).map_err(|e| e.with("task", id))?;
        if p.question.is_some() && t.questions.len() >= MAX_QUESTIONS {
            return Err(err("too_many_questions", format!("a task holds at most {MAX_QUESTIONS} questions")).with("task", id));
        }
        self.check_deps(&p.add_deps)?;
        if let Some(c) = self.would_cycle(id, &p.add_deps) {
            return Err(err("dep_cycle", format!("task {id} can't depend on {c}: that would make a cycle")).with("task", id));
        }
        let links = |cur: usize, add: usize| cur + add > MAX_LINKS;
        if links(t.deps.len(), p.add_deps.len())
            || links(t.paths.len(), p.add_paths.len())
            || links(t.entities.len(), p.add_entities.len())
            || links(t.claims.len(), p.add_claims.len())
            || links(t.tags.len(), p.add_tags.len())
        {
            return Err(err("too_many_links", format!("a task holds at most {MAX_LINKS} of each kind of link")).with("task", id));
        }
        // Everything is checked: apply.
        let name = w.name;
        let mut events: Vec<(&'static str, String)> = Vec::new();
        let t = self.tasks.get_mut(&id).expect("checked");
        if let Some(text) = p.progress {
            events.push(("progress", text.clone()));
            t.progress = text;
        }
        if let Some(s) = p.status {
            match s {
                Status::Blocked => {
                    t.blocker = p.blocker.unwrap_or_default();
                    events.push(("blocked", t.blocker.clone()));
                }
                Status::Taken => {
                    t.blocker.clear();
                    events.push(("unblocked", String::new()));
                }
                _ => {
                    t.owner = None;
                    t.blocker.clear();
                    events.push(("reopen", String::new()));
                }
            }
            t.status = s;
        }
        if let Some(text) = p.question {
            let qid = t.questions.iter().map(|q| q.id).max().unwrap_or(0) + 1;
            events.push(("question", format!("q{qid}: {text}")));
            t.questions.push(Question { id: qid, by: name.to_owned(), text, answer: None });
        }
        if let Some((q, text)) = p.answer {
            events.push(("answer", format!("q{q}: {text}")));
            if let Some(x) = t.questions.iter_mut().find(|x| x.id == q) {
                x.answer = Some((name.to_owned(), text));
            }
        }
        if let Some(title) = p.title {
            t.title = title;
            events.push(("update", "title".into()));
        }
        if let Some(detail) = p.detail {
            t.detail = detail;
            events.push(("update", "detail".into()));
        }
        if let Some(a) = p.assignee {
            events.push(("assign", a.clone().unwrap_or_else(|| "nobody".into())));
            t.assignee = a;
        }
        if let Some(pr) = p.priority {
            t.priority = pr;
            events.push(("update", format!("priority {pr}")));
        }
        let mut linked = Vec::new();
        for d in p.add_deps {
            if !t.deps.contains(&d) {
                t.deps.push(d);
                linked.push(format!("dep {d}"));
            }
        }
        if !p.remove_deps.is_empty() {
            t.deps.retain(|d| !p.remove_deps.contains(d));
            linked.push("deps removed".into());
        }
        push_new(&mut t.paths, p.add_paths, &mut linked, "path");
        push_new(&mut t.entities, p.add_entities, &mut linked, "entity");
        push_new(&mut t.claims, p.add_claims, &mut linked, "claim");
        push_new(&mut t.tags, p.add_tags, &mut linked, "tag");
        if !linked.is_empty() {
            events.push(("link", linked.join(", ")));
        }
        if events.is_empty() {
            return Err(err("nothing_to_change", "task_update needs at least one change").with("task", id));
        }
        for (what, text) in events {
            self.event(name, tick, what, Some(id), text);
        }
        if owner || poster {
            self.bind(w, id);
        }
        Ok(&self.tasks[&id])
    }

    /// `task_done`: the owner finishes it (or the owner or poster cancels it).
    pub fn done(&mut self, w: Writer, tick: u64, id: u64, summary: String, cancel: bool, expect: Option<u64>) -> Result<&Task, WorldError> {
        let t = self.tasks.get(&id).ok_or_else(|| not_found(id))?;
        Self::check_version(t, expect)?;
        if t.status.finished() {
            return Err(err("task_finished", format!("task {id} is already {}", t.status.name())).with("task", id));
        }
        let owner = Self::owns(t, w);
        if cancel {
            if !owner && !Self::posted(t, w) {
                return Err(err("not_poster", format!("only the owner or {} (who posted it) may cancel task {id}", t.posted_by)).with("task", id));
            }
        } else if !owner {
            return Err(not_owner(t, "finish"));
        }
        self.bind(w, id);
        let t = self.tasks.get_mut(&id).expect("checked");
        t.status = if cancel { Status::Cancelled } else { Status::Done };
        t.result = summary.clone();
        t.blocker.clear();
        self.event(w.name, tick, if cancel { "cancel" } else { "done" }, Some(id), summary);
        Ok(&self.tasks[&id])
    }

    /// `task_list`: one page, in id order.
    pub fn list(&self, q: &TaskQuery) -> Json {
        let ready = |t: &Task| {
            t.status == Status::Open
                && t.deps.iter().all(|d| self.tasks.get(d).is_none_or(|d| d.status.finished()))
                && q.ready_for.as_deref().is_none_or(|me| t.assignee.as_deref().is_none_or(|a| a == me))
        };
        let matching = self.tasks.values().filter(|t| {
            (q.statuses.is_empty() || q.statuses.contains(&t.status))
                && q.owner.as_deref().is_none_or(|o| t.owner.as_ref().is_some_and(|x| x.name == o))
                && q.posted_by.as_deref().is_none_or(|p| t.posted_by == p)
                && q.assignee.as_deref().is_none_or(|a| t.assignee.as_deref() == Some(a))
                && (!q.ready || ready(t))
                && q.path_prefix.as_deref().is_none_or(|p| t.paths.iter().any(|tp| path_under(tp, p) || path_under(p, tp)))
                && q.tag.as_deref().is_none_or(|g| t.tags.iter().any(|x| x == g))
                && q.parent.is_none_or(|p| t.parent == Some(p))
                && q.changed_since.is_none_or(|s| t.updated_seq > s)
        });
        let total = matching.clone().count();
        let (items, next) = page(matching.filter(|t| q.after.is_none_or(|a| t.id > a)).map(|t| (t.id, t.summary_json())), q.limit);
        let mut counts: Vec<(&str, Json)> = Vec::new();
        for s in Status::ALL {
            counts.push((s.name(), Json::from(self.tasks.values().filter(|t| t.status == s).count())));
        }
        Json::obj([
            ("total", Json::from(total)),
            ("tasks", Json::Arr(items)),
            ("next_cursor", Json::from(next)),
            ("board_seq", Json::from(self.seq)),
            ("counts", Json::obj(counts)),
        ])
    }

    /// Board events after `since`, oldest first, at most `limit`.
    pub fn events_since(&self, since: u64, limit: usize) -> impl Iterator<Item = &BoardEvent> + '_ {
        let start = self.events.partition_point(|e| e.seq <= since);
        self.events.range(start..).take(limit)
    }

    /// Board events by `agent` (all kept, newest last).
    pub fn events_by<'a>(&'a self, agent: &'a str) -> impl Iterator<Item = &'a BoardEvent> + 'a {
        self.events.iter().filter(move |e| e.by == agent)
    }

    // ---- notes ----

    /// `note_add`.
    pub fn add_note(&mut self, w: Writer, tick: u64, target: NoteTarget, text: String) -> Result<&Note, WorldError> {
        if let NoteTarget::Task(t) = &target {
            if !self.tasks.contains_key(t) {
                return Err(not_found(*t));
            }
        }
        if self.notes.len() >= MAX_NOTES {
            if let Some((_, old)) = self.notes.pop_first() {
                self.bytes = self.bytes.saturating_sub(old.measure());
            }
        }
        self.check_budget(160 + text.len())?;
        self.next_note += 1;
        let id = self.next_note;
        let task = match &target {
            NoteTarget::Task(t) => Some(*t),
            _ => None,
        };
        let seq = self.event(w.name, tick, "note", task, short(&text, 80));
        let note = Note { id, by: w.name.to_owned(), by_key: w.session.map(str::to_owned), target, text, tick, seq };
        self.bytes += note.measure();
        self.notes.insert(id, note);
        Ok(&self.notes[&id])
    }

    /// `note_delete`: the author's own, or another's with `force` and lead authority.
    pub fn delete_note(&mut self, w: Writer, authority: crate::authority::Authority, tick: u64, id: u64, force: bool) -> Result<Note, WorldError> {
        let n = self.notes.get(&id).ok_or_else(|| err("note_not_found", format!("there is no note {id}")).with("note", id))?;
        let own = n.by == w.name && n.by_key.as_deref().is_none_or(|k| w.session == Some(k));
        if !own && !force {
            return Err(err("not_yours", format!("note {id} was left by {} (in another session, or another agent); a lead may delete it with force: true", n.by))
                .with("note", id)
                .with("by", n.by.as_str()));
        }
        crate::authority::may_force(authority, own, "note_delete").map_err(|e| e.with("note", id).with("by", n.by.as_str()))?;
        let n = self.notes.remove(&id).expect("checked");
        self.bytes = self.bytes.saturating_sub(n.measure());
        let task = match &n.target {
            NoteTarget::Task(t) => Some(*t),
            _ => None,
        };
        self.event(w.name, tick, "note_delete", task, format!("note {id}"));
        Ok(n)
    }

    /// `note_list`: one page, in id order. A note on an entity shows the entity's current path,
    /// looked up for the page's notes only (which refreshes their last known path), or that it
    /// is detached. `path_prefix` resolves the prefix once and asks each attached note's entity
    /// whether it is below it (no allocation per note); `entity` matches attached notes only.
    pub fn list_notes(&mut self, q: &NoteQuery, world: &World) -> Json {
        let prefix = q.path_prefix.as_deref().map(|p| Prefix::new(world, p));
        let matches = |n: &Note| {
            q.by.as_deref().is_none_or(|b| n.by == b)
                && prefix.as_ref().is_none_or(|p| n.target.under(world, p))
                && q.entity.is_none_or(|e| matches!(&n.target, NoteTarget::Entity(r) if r.attached(world) == Some(e)))
                && q.task.is_none_or(|t| n.target == NoteTarget::Task(t))
        };
        let total = self.notes.values().filter(|n| matches(n)).count();
        // The page's notes, and one more (it tells whether more follow).
        let shown: Vec<u64> =
            self.notes.values().filter(|n| q.after.is_none_or(|a| n.id > a) && matches(n)).take(q.limit.saturating_add(1)).map(|n| n.id).collect();
        for &id in &shown {
            self.refresh_note(id, world);
        }
        let (items, next) = page(shown.iter().map(|id| (*id, self.notes[id].shown(world))), q.limit);
        Json::obj([("total", Json::from(total)), ("notes", Json::Arr(items)), ("next_cursor", Json::from(next))])
    }

    /// Brings note `id`'s entity's last known path up to date (while attached); a change is
    /// saved with the board. Whether it changed.
    fn refresh_note(&mut self, id: u64, world: &World) -> bool {
        let Some(Note { target: NoteTarget::Entity(r), .. }) = self.notes.get_mut(&id) else { return false };
        let old = r.path.len();
        if !r.refresh(world) {
            return false;
        }
        self.bytes = (self.bytes + r.path.len()).saturating_sub(old);
        self.rendered_notes.remove(&id);
        self.revision += 1;
        true
    }

    /// Brings task `id`'s entities' last known paths up to date (`task_get` shows them); a
    /// change is saved with the board. Whether any changed.
    pub fn refresh_task_entities(&mut self, id: u64, world: &World) -> bool {
        let Some(t) = self.tasks.get_mut(&id) else { return false };
        let mut changed = false;
        for e in &mut t.entities {
            changed |= e.refresh(world);
        }
        if changed {
            let size = t.measure();
            self.bytes = (self.bytes + size).saturating_sub(t.size);
            t.size = size;
            self.rendered.remove(&id);
            self.revision += 1;
        }
        changed
    }

    /// Before the world is replaced (a save read keeps the board): every note's and task's
    /// entity records its path as of now, so a reference the new world doesn't have shows where
    /// its entity last was. O(references x (log n + depth)), once per replacement. Returns how
    /// many changed.
    pub fn remember_entity_paths(&mut self, world: &World) -> usize {
        let notes: Vec<u64> = self.notes.values().filter(|n| matches!(n.target, NoteTarget::Entity(_))).map(|n| n.id).collect();
        let tasks: Vec<u64> = self.tasks.values().filter(|t| !t.entities.is_empty()).map(|t| t.id).collect();
        notes.into_iter().filter(|id| self.refresh_note(*id, world)).count() + tasks.into_iter().filter(|id| self.refresh_task_entities(*id, world)).count()
    }

    /// Notes on `task`, newest last (at most `limit`).
    pub fn notes_on_task(&self, task: u64, limit: usize) -> Vec<&Note> {
        let all: Vec<&Note> = self.notes.values().filter(|n| n.target == NoteTarget::Task(task)).collect();
        all[all.len().saturating_sub(limit)..].to_vec()
    }

    // ---- persistence ----

    /// The board as `board.json` text, re-rendering only tasks that changed since the last call.
    pub fn render_cached(&mut self) -> String {
        let tasks = &self.tasks;
        self.rendered.retain(|id, _| tasks.contains_key(id));
        for t in self.tasks.values() {
            let key = (t.version, t.work);
            if self.rendered.get(&t.id).is_none_or(|(k, _)| *k != key) {
                self.rendered.insert(t.id, (key, Self::task_line(t)));
            }
        }
        let notes = &self.notes;
        self.rendered_notes.retain(|id, _| notes.contains_key(id));
        for n in self.notes.values() {
            self.rendered_notes.entry(n.id).or_insert_with(|| n.to_json().to_string());
        }
        let size = self.rendered.values().map(|(_, l)| l.len() + 2).sum::<usize>() + self.rendered_notes.values().map(|l| l.len() + 2).sum::<usize>();
        let mut out = String::with_capacity(size + 256);
        self.write_into(&mut out, |id| &self.rendered[&id].1, |id| &self.rendered_notes[&id]);
        out
    }

    fn task_line(t: &Task) -> String {
        let mut j = t.to_json(None);
        // Owners and posters come back by name (keys are this run's).
        j.set("owner", t.owner.as_ref().map_or(Json::Null, |o| Json::from(o.name.as_str())));
        j.set("assignee", t.assignee.as_deref().map_or(Json::Null, Json::from));
        j.set("events_dropped", Json::from(t.events_dropped));
        j.to_string()
    }

    fn write_into<'a>(&'a self, out: &mut String, line: impl Fn(u64) -> &'a str, note: impl Fn(u64) -> &'a str) {
        out.push_str(&format!(
            "{{\"format\": \"{FORMAT}\", \"next_task\": {}, \"next_note\": {}, \"seq\": {},\n\"tasks\": [",
            self.next_task, self.next_note, self.seq
        ));
        for (i, id) in self.tasks.keys().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            out.push_str(line(*id));
        }
        out.push_str("\n],\n\"notes\": [");
        for (i, id) in self.notes.keys().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            out.push_str(note(*id));
        }
        out.push_str("\n]}\n");
    }

    /// The board as `board.json` text.
    pub fn render(&self) -> String {
        let lines: BTreeMap<u64, String> = self.tasks.values().map(|t| (t.id, Self::task_line(t))).collect();
        let notes: BTreeMap<u64, String> = self.notes.values().map(|n| (n.id, n.to_json().to_string())).collect();
        let mut out = String::new();
        self.write_into(&mut out, |id| &lines[&id], |id| &notes[&id]);
        out
    }

    /// Reads `board.json` text (see [`Board::render`]).
    pub fn parse(text: &str) -> Result<Board, String> {
        let doc = Json::parse(text).map_err(|e| e.to_string())?;
        if doc.get("format").and_then(Json::as_str) != Some(FORMAT) {
            return Err(format!("not a board file (format must be \"{FORMAT}\")"));
        }
        let num = |j: &Json, k: &str| j.get(k).and_then(Json::as_u64).unwrap_or(0);
        let text = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
        let ids = |j: &Json, k: &str| -> Vec<u64> { j.get(k).and_then(Json::as_array).unwrap_or_default().iter().filter_map(Json::as_u64).take(MAX_LINKS).collect() };
        let strs = |j: &Json, k: &str| -> Vec<String> {
            j.get(k).and_then(Json::as_array).unwrap_or_default().iter().filter_map(Json::as_str).take(MAX_LINKS).map(str::to_owned).collect()
        };
        let mut b = Board { next_task: num(&doc, "next_task"), next_note: num(&doc, "next_note"), seq: num(&doc, "seq"), ..Board::default() };
        for t in doc.get("tasks").and_then(Json::as_array).unwrap_or_default().iter().take(MAX_TASKS) {
            let id = t.get("id").and_then(Json::as_u64).ok_or("a task has no id")?;
            let status = t.get("status").and_then(Json::as_str).and_then(Status::parse).ok_or(format!("task {id}: bad status"))?;
            let questions = t
                .get("questions")
                .and_then(Json::as_array)
                .unwrap_or_default()
                .iter()
                .take(MAX_QUESTIONS)
                .map(|q| Question {
                    id: num(q, "id") as u32,
                    by: text(q, "by"),
                    text: text(q, "text"),
                    answer: q.get("answer").filter(|a| !a.is_null()).map(|a| (text(a, "by"), text(a, "text"))),
                })
                .collect();
            let events = t
                .get("events")
                .and_then(Json::as_array)
                .unwrap_or_default()
                .iter()
                .rev()
                .take(MAX_TASK_EVENTS)
                .rev()
                .map(|e| BoardEvent {
                    seq: num(e, "seq"),
                    tick: num(e, "tick"),
                    by: text(e, "by"),
                    what: event_name(e.get("what").and_then(Json::as_str).unwrap_or("")),
                    task: Some(id),
                    text: text(e, "text"),
                })
                .collect();
            let work = t.get("work").cloned().unwrap_or(Json::Null);
            let owner = t.get("owner").and_then(Json::as_str).map(|n| Owner { name: n.to_owned(), key: None });
            b.tasks.insert(
                id,
                Task {
                    id,
                    title: text(t, "title"),
                    detail: text(t, "detail"),
                    status,
                    posted_by: text(t, "posted_by"),
                    poster_key: None,
                    owner: if status.active() { owner } else { owner.filter(|_| status.finished()) },
                    assignee: t.get("assignee").and_then(Json::as_str).map(str::to_owned),
                    parent: t.get("parent").and_then(Json::as_u64),
                    deps: ids(t, "deps"),
                    paths: strs(t, "paths"),
                    // Older files kept bare ids: no identity, so they are detached.
                    entities: t.get("entities").and_then(Json::as_array).unwrap_or_default().iter().filter_map(EntityRef::from_saved).take(MAX_LINKS).collect(),
                    claims: Vec::new(), // claims are live coordination: not carried across runs
                    tags: strs(t, "tags"),
                    priority: num(t, "priority").min(9) as u8,
                    progress: text(t, "progress"),
                    blocker: text(t, "blocker"),
                    result: text(t, "result"),
                    questions,
                    events,
                    events_dropped: num(t, "events_dropped"),
                    created_tick: num(t, "created_tick"),
                    updated_tick: num(t, "updated_tick"),
                    updated_seq: num(t, "updated_seq"),
                    version: num(t, "version"),
                    work: TaskWork { entries: num(&work, "journal_entries"), changes: num(&work, "changes"), refused: num(&work, "refused") },
                    size: 0,
                },
            );
            b.next_task = b.next_task.max(id);
        }
        for n in doc.get("notes").and_then(Json::as_array).unwrap_or_default().iter().take(MAX_NOTES) {
            let id = n.get("id").and_then(Json::as_u64).ok_or("a note has no id")?;
            let target = if let Some(t) = n.get("task").and_then(Json::as_u64) {
                NoteTarget::Task(t)
            } else if let Some(r) = n.get("entity").and_then(EntityRef::from_saved) {
                // Older files kept `{id, path}`: no identity, so the note is detached.
                NoteTarget::Entity(r)
            } else {
                NoteTarget::Path(text(n, "path"))
            };
            b.notes.insert(id, Note { id, by: text(n, "by"), by_key: None, target, text: text(n, "text"), tick: num(n, "tick"), seq: num(n, "seq") });
            b.next_note = b.next_note.max(id);
        }
        for t in b.tasks.values_mut() {
            t.size = t.measure();
        }
        b.bytes = b.tasks.values().map(|t| t.size).sum::<usize>() + b.notes.values().map(Note::measure).sum::<usize>();
        Ok(b)
    }
}

/// The static name of a saved event kind.
fn event_name(s: &str) -> &'static str {
    const NAMES: [&str; 23] = [
        "post", "take", "update", "progress", "blocked", "unblocked", "question", "answer", "assign", "reopen", "done", "cancel", "link",
        "note", "note_delete", "intent", "intent_clear", "lease", "lease_release", "settings", "claim", "review", "critique",
    ];
    NAMES.into_iter().find(|n| *n == s).unwrap_or("update")
}

fn not_owner(t: &Task, what: &str) -> WorldError {
    let message = match &t.owner {
        Some(o) if t.status.active() => format!("only {} (who has task {}) may {what} it", o.name, t.id),
        _ => format!("task {} is {}: take it first (task_take) to {what} it", t.id, t.status.name()),
    };
    err("not_task_owner", message).with("task", t.id).with("status", t.status.name())
}

fn dedup<T: PartialEq>(v: Vec<T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(v.len());
    for x in v {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

fn push_new<T: PartialEq + std::fmt::Display>(into: &mut Vec<T>, add: Vec<T>, linked: &mut Vec<String>, what: &str) {
    for x in add {
        if !into.contains(&x) {
            linked.push(format!("{what} {x}"));
            into.push(x);
        }
    }
}

/// Items in order until `limit` or the byte cap; returns them and the cursor (the last id shown)
/// when more follow.
pub fn page(items: impl Iterator<Item = (u64, Json)>, limit: usize) -> (Vec<Json>, Option<u64>) {
    let (mut out, mut bytes, mut last) = (Vec::new(), 0, None);
    for (id, item) in items {
        let size = item.to_string().len();
        if out.len() == limit || (!out.is_empty() && bytes + size > MAX_LIST_BYTES) {
            return (out, last);
        }
        bytes += size;
        last = Some(id);
        out.push(item);
    }
    (out, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT: Writer = Writer { name: "lead", session: Some("k0"), via: None };
    const ANN: Writer = Writer { name: "ann", session: Some("k1"), via: None };
    const BOB: Writer = Writer { name: "bob", session: Some("k2"), via: None };

    fn spec(title: &str) -> TaskSpec {
        TaskSpec { title: title.into(), ..TaskSpec::default() }
    }

    #[test]
    fn tasks_flow_from_open_to_done_with_dependencies_and_assignees() {
        let mut b = Board::default();
        let walls = b.post(PARENT, 1, TaskSpec { paths: vec!["/walls".into()], ..spec("walls") }).unwrap().id;
        let gate = b.post(PARENT, 1, TaskSpec { deps: vec![walls], assignee: Some("bob".into()), ..spec("gate") }).unwrap().id;
        // Dependencies first; an assigned task only by its assignee.
        let e = b.take(BOB, 2, gate, None).unwrap_err();
        assert_eq!(e.code, "deps_not_done");
        assert_eq!(e.detail("pending").and_then(Json::as_array).map(<[Json]>::len), Some(1));
        b.take(ANN, 2, walls, None).unwrap();
        assert_eq!(b.take(BOB, 2, walls, None).unwrap_err().code, "task_taken");
        assert_eq!(b.current(ANN).map(|t| t.id), Some(walls));
        assert_eq!(b.done(BOB, 3, walls, String::new(), false, None).unwrap_err().code, "not_task_owner");
        b.update(ANN, 3, walls, TaskPatch { progress: Some("north side up".into()), ..TaskPatch::default() }).unwrap();
        b.done(ANN, 4, walls, "42 stones".into(), false, None).unwrap();
        assert!(b.current(ANN).is_none(), "a finished task is no longer current");
        assert_eq!(b.take(ANN, 5, gate, None).unwrap_err().code, "assigned_elsewhere");
        b.take(BOB, 5, gate, None).unwrap();
        // Ready list: nothing open is left.
        let ready = b.list(&TaskQuery { ready: true, limit: 50, ..TaskQuery::default() });
        assert_eq!(ready.get("total"), Some(&Json::from(0)));
        let t = b.get(walls).unwrap();
        assert_eq!((t.status, t.result.as_str()), (Status::Done, "42 stones"));
        assert_eq!(t.events.iter().map(|e| e.what).collect::<Vec<_>>(), ["post", "take", "progress", "done"]);
    }

    #[test]
    fn updates_check_who_may_change_what() {
        let mut b = Board::default();
        let id = b.post(PARENT, 0, spec("tavern")).unwrap().id;
        assert_eq!(b.update(ANN, 0, id, TaskPatch { title: Some("x".into()), ..TaskPatch::default() }).unwrap_err().code, "not_poster");
        assert_eq!(b.update(ANN, 0, id, TaskPatch { progress: Some("x".into()), ..TaskPatch::default() }).unwrap_err().code, "not_task_owner");
        b.take(ANN, 1, id, None).unwrap();
        let blocked = TaskPatch { status: Some(Status::Blocked), ..TaskPatch::default() };
        assert_eq!(b.update(ANN, 1, id, blocked.clone()).unwrap_err().code, "blocker_required");
        b.update(ANN, 1, id, TaskPatch { blocker: Some("no roof recipe".into()), ..blocked }).unwrap();
        assert_eq!(b.get(id).unwrap().status, Status::Blocked);
        // Questions and answers from anyone; a stale version is refused.
        let v = b.get(id).unwrap().version;
        b.update(BOB, 2, id, TaskPatch { question: Some("thatch or slate?".into()), ..TaskPatch::default() }).unwrap();
        let stale = b.update(PARENT, 2, id, TaskPatch { answer: Some((1, "slate".into())), expect_version: Some(v), ..TaskPatch::default() });
        assert_eq!(stale.unwrap_err().code, "stale_task");
        b.update(PARENT, 2, id, TaskPatch { answer: Some((1, "slate".into())), ..TaskPatch::default() }).unwrap();
        assert_eq!(b.get(id).unwrap().open_questions(), 0);
        // The poster takes it back and reassigns.
        b.update(PARENT, 3, id, TaskPatch { status: Some(Status::Open), assignee: Some(Some("bob".into())), ..TaskPatch::default() }).unwrap();
        let t = b.get(id).unwrap();
        assert_eq!((t.status, t.owner.is_none(), t.assignee.as_deref()), (Status::Open, true, Some("bob")));
        // Dependency cycles are refused.
        let other = b.post(PARENT, 3, TaskSpec { deps: vec![id], ..spec("roof") }).unwrap().id;
        let cycle = b.update(PARENT, 3, id, TaskPatch { add_deps: vec![other], ..TaskPatch::default() });
        assert_eq!(cycle.unwrap_err().code, "dep_cycle");
        assert_eq!(b.update(PARENT, 3, id, TaskPatch::default()).unwrap_err().code, "nothing_to_change");
        // Cancel: owner or poster only.
        assert_eq!(b.done(ANN, 4, other, String::new(), true, None).unwrap_err().code, "not_poster");
        b.done(PARENT, 4, other, "not needed".into(), true, None).unwrap();
        assert_eq!(b.take(ANN, 4, other, None).unwrap_err().code, "task_finished");
    }

    #[test]
    fn journal_entries_claims_and_refusals_count_toward_the_current_task() {
        let mut b = Board::default();
        let id = b.post(PARENT, 0, spec("walls")).unwrap().id;
        b.note_entry(ANN, 1, 3);
        assert_eq!(b.task_of_entry(1), None, "no current task yet");
        b.take(ANN, 0, id, None).unwrap();
        b.note_entry(ANN, 2, 3);
        b.note_entry(ANN, 3, 1);
        b.note_refused(ANN);
        b.note_claim(ANN, 7, 0);
        b.note_entry(BOB, 4, 1);
        let t = b.get(id).unwrap();
        assert_eq!(t.work, TaskWork { entries: 2, changes: 4, refused: 1 });
        assert_eq!((b.task_of_entry(2), b.task_of_entry(4)), (Some(id), None));
        assert_eq!(t.claims, [7]);
        // Another session using the name adopts nothing.
        assert!(b.current(Writer::new("ann", Some("k9"))).is_none());
    }

    #[test]
    fn notes_attach_to_entities_paths_and_tasks() {
        use crate::world::{EntityKey, Shape, SpawnSpec};
        let mut w = World::default();
        let mut put = |name: &str, parent: Option<u64>| {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some(name.into());
            s.parent = parent.map(EntityKey::Id);
            w.spawn(s, "t").unwrap().id
        };
        let east = put("east", None);
        let gate = put("gate", Some(east));
        let mut b = Board::default();
        let id = b.post(PARENT, 0, spec("gate")).unwrap().id;
        let r = EntityRef::of(&w, gate).unwrap();
        b.add_note(ANN, 1, NoteTarget::Entity(r), "hinges are placeholders".into()).unwrap();
        b.add_note(ANN, 1, NoteTarget::Path("/east".into()), "east side is mine until noon".into()).unwrap();
        b.add_note(BOB, 1, NoteTarget::Task(id), "start from the west".into()).unwrap();
        assert_eq!(b.add_note(BOB, 1, NoteTarget::Task(99), "x".into()).unwrap_err().code, "task_not_found");
        let listed = b.list_notes(&NoteQuery { path_prefix: Some("/east".into()), limit: 10, ..NoteQuery::default() }, &w);
        assert_eq!(listed.get("total"), Some(&Json::from(2)));
        let on_gate = b.list_notes(&NoteQuery { entity: Some(gate), limit: 10, ..NoteQuery::default() }, &w);
        assert_eq!(on_gate.get("total"), Some(&Json::from(1)));
        let shown = on_gate.get("notes").and_then(|n| n.as_array()).unwrap()[0].get("entity").unwrap().to_string();
        assert_eq!(shown, format!(r#"{{"id":{gate},"path":"/east/gate"}}"#));
        // Saved with its identity.
        assert!(b.render().contains(&format!(r#""entity":{{"id":{gate},"born":"{:016x}","path":"/east/gate"}}"#, w.lineage())), "{}", b.render());
        assert_eq!(b.notes_on_task(id, 5).len(), 1);
        assert_eq!(b.delete_note(BOB, crate::authority::Authority::Agent, 2, 1, false).unwrap_err().code, "not_yours");
        b.delete_note(ANN, crate::authority::Authority::Agent, 2, 1, false).unwrap();
        assert_eq!(b.notes_len(), 2);
    }

    #[test]
    fn the_board_round_trips_through_its_file() {
        let mut b = Board::default();
        let a = b.post(PARENT, 0, TaskSpec { tags: vec!["art".into()], paths: vec!["/a".into()], ..spec("a") }).unwrap().id;
        let c = b.post(PARENT, 0, TaskSpec { deps: vec![a], ..spec("c \"quoted\"\nline") }).unwrap().id;
        b.take(ANN, 1, a, None).unwrap();
        b.update(BOB, 2, a, TaskPatch { question: Some("why?".into()), ..TaskPatch::default() }).unwrap();
        b.add_note(ANN, 2, NoteTarget::Task(c), "note".into()).unwrap();
        let text = b.render();
        let back = Board::parse(&text).unwrap();
        assert_eq!(back.render(), text, "render(parse(x)) == x");
        let t = back.get(a).unwrap();
        assert_eq!((t.status, t.owner.as_ref().map(|o| o.name.as_str()), t.owner.as_ref().and_then(|o| o.key.as_deref())), (Status::Taken, Some("ann"), None));
        // Loaded owners come back by name: any session with that name may finish it.
        let mut back = back;
        back.done(Writer::new("ann", Some("new-key")), 3, a, "ok".into(), false, None).unwrap();
        // Ids keep counting.
        assert_eq!(back.post(PARENT, 3, spec("d")).unwrap().id, c + 1);
        assert!(Board::parse("{\"format\": \"other\"}").is_err());
    }

    /// Review finding (major): a later session that took a gone poster's name could reopen and
    /// retitle tasks, and delete the poster's notes; loaded owners stayed name-only for good.
    #[test]
    fn a_name_taken_again_adopts_no_tasks_or_notes() {
        let mut b = Board::default();
        let id = b.post(PARENT, 0, spec("gate")).unwrap().id;
        b.take(ANN, 0, id, None).unwrap();
        b.add_note(PARENT, 0, NoteTarget::Task(id), "mind the hinges".into()).unwrap();
        let heir = Writer::new("lead", Some("k-new"));
        let reopen = TaskPatch { status: Some(Status::Open), ..TaskPatch::default() };
        assert_eq!(b.update(heir, 1, id, reopen.clone()).unwrap_err().code, "not_task_owner");
        assert_eq!(b.update(heir, 1, id, TaskPatch { title: Some("hijacked".into()), ..TaskPatch::default() }).unwrap_err().code, "not_poster");
        assert_eq!(b.done(heir, 1, id, String::new(), true, None).unwrap_err().code, "not_poster");
        assert_eq!(b.delete_note(heir, crate::authority::Authority::Agent, 1, 1, false).unwrap_err().code, "not_yours");
        // The real poster still can.
        b.update(PARENT, 1, id, reopen).unwrap();
        // Loaded: name-only until they act, then bound.
        let mut back = Board::parse(&b.render()).unwrap();
        let ann_now = Writer::new("ann", Some("k-ann-2"));
        back.take(ann_now, 2, id, None).unwrap();
        back.update(Writer::new("lead", Some("k-lead-2")), 2, id, TaskPatch { priority: Some(1), ..TaskPatch::default() }).unwrap();
        assert_eq!(back.update(Writer::new("lead", Some("k-other")), 2, id, TaskPatch { priority: Some(2), ..TaskPatch::default() }).unwrap_err().code, "not_poster");
        assert_eq!(back.done(Writer::new("ann", Some("k-other")), 2, id, String::new(), false, None).unwrap_err().code, "not_task_owner");
        back.done(ann_now, 2, id, String::new(), false, None).unwrap();
    }

    #[test]
    fn feed_only_events_leave_the_board_saved_and_renders_are_cached() {
        let mut b = Board::default();
        b.post(PARENT, 0, spec("a")).unwrap();
        let r = b.revision();
        b.log("ann", 0, "lease", None, "port lease 1".into());
        assert_eq!(b.revision(), r, "nothing saved changed");
        assert_eq!(b.events_since(0, 10).count(), 2, "but the feed has it");
        let first = b.render_cached();
        assert_eq!(first, b.render());
        b.take(ANN, 1, 1, None).unwrap();
        assert_eq!(b.render_cached(), b.render(), "a changed task is rendered again");
        // Review round 2: journal links change the work counters without a new version.
        let r = b.revision();
        b.note_entry(ANN, 9, 3);
        assert!(b.revision() > r, "work counters are saved too");
        assert_eq!(b.render_cached(), b.render(), "and rendered again");
        b.add_note(ANN, 2, NoteTarget::Task(1), "n".into()).unwrap();
        assert_eq!(b.render_cached(), b.render(), "notes are cached by id");
    }

    #[test]
    fn a_finished_dependency_no_longer_blocks() {
        let mut b = Board::default();
        let a = b.post(PARENT, 0, spec("a")).unwrap().id;
        let c = b.post(PARENT, 0, TaskSpec { deps: vec![a], ..spec("c") }).unwrap().id;
        b.done(PARENT, 0, a, "not needed".into(), true, None).unwrap();
        assert_eq!(b.list(&TaskQuery { ready: true, limit: 10, ..TaskQuery::default() }).get("total"), Some(&Json::from(1)));
        b.take(ANN, 0, c, None).unwrap();
    }

    /// Review round 3: the board's text is bounded as a whole (about 1.7 G characters were
    /// possible), and a looping agent can't fill it with questions.
    #[test]
    fn the_board_has_a_byte_budget() {
        let mut b = Board::default();
        let big = "x".repeat(MAX_DETAIL);
        let mut posted = 0;
        let full = loop {
            match b.post(PARENT, 0, TaskSpec { detail: big.clone(), ..spec("t") }) {
                Ok(_) => posted += 1,
                Err(e) => break e,
            }
            assert!(posted < MAX_TASKS, "the count limit came first");
        };
        assert_eq!(full.code, "board_full", "{full:?}");
        assert!(b.bytes() <= MAX_BOARD_BYTES && b.bytes() > MAX_BOARD_BYTES - 64 * 1024);
        // Finished tasks make room again.
        b.take(ANN, 0, 1, None).unwrap();
        b.done(ANN, 0, 1, String::new(), false, None).unwrap();
        b.post(PARENT, 0, TaskSpec { detail: big.clone(), ..spec("again") }).unwrap();
        assert!(b.get(1).is_none());
        // A full board refuses more text on a task, but not a status change.
        let more = TaskPatch { detail: Some(big.clone()), ..TaskPatch::default() };
        assert_eq!(b.update(PARENT, 0, 2, more).unwrap_err().code, "board_full");
        b.take(ANN, 0, 2, None).unwrap();
        // The measure survives a save and load.
        let back = Board::parse(&b.render()).unwrap();
        assert_eq!(back.bytes(), b.bytes());
        // Questions are capped per task.
        let mut b = Board::default();
        b.post(PARENT, 0, spec("t")).unwrap();
        for i in 0..MAX_QUESTIONS {
            b.update(BOB, 0, 1, TaskPatch { question: Some(format!("q{i}")), ..TaskPatch::default() }).unwrap();
        }
        let more = b.update(BOB, 0, 1, TaskPatch { question: Some("again?".into()), ..TaskPatch::default() });
        assert_eq!(more.unwrap_err().code, "too_many_questions");
    }

    #[test]
    fn the_board_is_bounded() {
        let mut b = Board::default();
        for i in 0..MAX_TASKS {
            b.post(PARENT, 0, spec(&format!("t{i}"))).unwrap();
        }
        assert_eq!(b.post(PARENT, 0, spec("one more")).unwrap_err().code, "too_many_tasks");
        b.take(ANN, 0, 1, None).unwrap();
        b.done(ANN, 0, 1, String::new(), false, None).unwrap();
        b.post(PARENT, 0, spec("one more")).unwrap();
        assert!(b.get(1).is_none(), "the oldest finished task made room");
        let page = b.list(&TaskQuery { limit: 500, ..TaskQuery::default() });
        assert!(page.to_string().len() <= MAX_LIST_BYTES + 1024);
        assert!(page.get("next_cursor").and_then(Json::as_u64).is_some());
        for i in 0..MAX_TASK_EVENTS + 5 {
            b.update(BOB, 0, 2, TaskPatch { question: (i < MAX_QUESTIONS).then(|| "q".into()), add_tags: vec![format!("x{i}")], ..TaskPatch::default() }).ok();
        }
        let t = b.get(2).unwrap();
        assert!(t.events.len() <= MAX_TASK_EVENTS && t.events_dropped > 0);
    }
}
