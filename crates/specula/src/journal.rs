//! The journal: an append-only, agent-attributed, tick-stamped record of every world change.
//!
//! Every change to the world — an agent's spawn, set, delete, batch or environment edit, an undo,
//! and whatever the hosted game does — becomes one [`Entry`]: who made it (`by`, plus `via` when
//! an agent asked the game to act), the simulation tick it was applied at, and the exact
//! [`Change`]s the world recorded. Entries are numbered by `seq`, a total order that never goes
//! back (not even across `world_load`), so it doubles as the change-feed cursor.
//!
//! - **Replay (F2).** Each change carries the entity's whole state afterwards (exact bits), and
//!   each entry the id counter, so applying the entries in order to the journal's *base* (the
//!   empty world, or the world as loaded) rebuilds the live world: [`replay`] reproduces
//!   [`World::state_hash`] exactly. That is the journal-replay test in CI.
//! - **Undo.** Agents' changes also carry the state before, so [`Journal::undo`] can put it back
//!   surgically: by default the caller's own most recent changes, refused when a later change by
//!   someone else touched the same entities (unless forced). A batch undoes as a unit. Undo is
//!   itself journaled, and undoing an undo redoes it.
//! - **Bounded (F1/F3).** Entries live in memory in two rings, capped by entries and by bytes:
//!   agents' (48 MiB) and the hosted game's (16 MiB), so a busy game never pushes agents' history
//!   and undo out. The oldest entries are evicted first. Once anything has been evicted the
//!   journal can no longer replay from its base (`replayable: false`), and changes that old can't
//!   be undone. There is no spill to disk yet: world files are the durable record, and M3's input
//!   journal (commands plus seeds, replayed through a deterministic game) is the long-run replay
//!   log — with a busy game, state-based replay covers a short window only.
//! - **Game updates are cheap (F3).** The game's changes are recorded *after-only* (replayable,
//!   not undoable) and coalesced per tick: one entry per tick with at most one change per entity.
//!   The game's newest change to each entity is remembered past eviction, so undo still refuses
//!   to overwrite it.

use crate::world::{Change, Entity, Recording, World, WorldError};
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

/// Vets the changes a command just made (the engine checks claims); an error rolls them back.
pub type Check<'a> = dyn FnMut(&World, &[Change]) -> Result<(), WorldError> + 'a;

/// Agents' entries kept in memory, and their approximate memory.
pub const MAX_ENTRIES: usize = 65_536;
pub const MAX_BYTES: usize = 48 << 20;
/// The hosted game's entries (one per tick or frame) kept, and their approximate memory.
pub const MAX_GAME_ENTRIES: usize = 65_536;
pub const MAX_GAME_BYTES: usize = 16 << 20;
/// Entries one world_undo call may undo.
pub const MAX_UNDO: usize = 100;
/// Entries a history call examines before handing back a cursor (bounds its cost).
const MAX_SCAN: usize = 50_000;
/// Changes listed per entry in a detailed history item.
const DETAIL_CHANGES: usize = 50;
/// Responses stop adding items past this many bytes and hand back a cursor.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// What kind of action an entry records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Spawn,
    Set,
    Delete,
    Batch,
    Env,
    Undo,
    /// The hosted game's `start`.
    GameStart,
    /// One simulation tick of the hosted game (coalesced).
    GameTick,
    /// The game's per-frame presentation update (coalesced).
    GameFrame,
    /// A game tool (`play_*`) an agent called; `via` names the agent.
    GameTool,
}

impl Kind {
    pub const ALL: [Kind; 10] = [
        Kind::Spawn,
        Kind::Set,
        Kind::Delete,
        Kind::Batch,
        Kind::Env,
        Kind::Undo,
        Kind::GameStart,
        Kind::GameTick,
        Kind::GameFrame,
        Kind::GameTool,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Spawn => "spawn",
            Kind::Set => "set",
            Kind::Delete => "delete",
            Kind::Batch => "batch",
            Kind::Env => "env",
            Kind::Undo => "undo",
            Kind::GameStart => "game_start",
            Kind::GameTick => "game_tick",
            Kind::GameFrame => "game_frame",
            Kind::GameTool => "game_tool",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == s)
    }

    pub fn is_game(self) -> bool {
        matches!(self, Kind::GameStart | Kind::GameTick | Kind::GameFrame | Kind::GameTool)
    }
}

/// One journaled action.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Journal id: 1, 2, 3… in the order changes were applied; never reused.
    pub seq: u64,
    /// The simulation tick it was applied at ([`crate::game::Clock::tick`]).
    pub tick: u64,
    /// The agent (or the game) that made the change.
    pub by: Arc<str>,
    /// For game entries caused by an agent's game tool call: that agent.
    pub via: Option<Arc<str>>,
    pub kind: Kind,
    /// In the order they happened (coalesced per entity for game entries).
    pub changes: Vec<Change>,
    /// The world's id counter afterwards (replay restores it: failed spawns use no ids, but
    /// deleted ids are never handed out again).
    pub next_id: u64,
    /// For undo entries: the entries undone.
    pub undoes: Vec<u64>,
    /// The undo entry that undid this one (cleared again if that undo is itself undone).
    pub undone_by: Option<u64>,
    /// Entity ids touched (0 = the environment), sorted.
    ids: Vec<u64>,
    bytes: usize,
}

impl Entry {
    /// Whether the entry touched entity `id` (0 = the environment).
    pub fn touches(&self, id: u64) -> bool {
        self.ids.binary_search(&id).is_ok()
    }

    /// Entity ids touched (0 = the environment), sorted.
    pub fn ids(&self) -> &[u64] {
        &self.ids
    }

    /// Whether every change kept its previous state (game updates don't).
    pub fn undoable(&self) -> bool {
        self.changes.iter().all(|c| !matches!(c, Change::Set { before: None, .. }))
    }

    fn matches_path(&self, prefix: &str) -> bool {
        self.changes.iter().any(|c| change_under(c, prefix))
    }

    /// A compact summary; `detail` adds up to [`DETAIL_CHANGES`] changes.
    pub fn to_json(&self, detail: bool, path_prefix: Option<&str>) -> Json {
        let mut f = vec![
            ("id", Json::from(self.seq)),
            ("tick", Json::from(self.tick)),
            ("by", Json::from(&*self.by)),
            ("kind", Json::from(self.kind.name())),
            ("changes", Json::from(self.changes.len())),
        ];
        if let Some(via) = &self.via {
            f.push(("via", Json::from(&**via)));
        }
        let relevant = |c: &&Change| path_prefix.is_none_or(|p| change_under(c, p));
        if detail {
            let items: Vec<Json> = self.changes.iter().filter(relevant).take(DETAIL_CHANGES).map(change_json).collect();
            let shown = items.len();
            f.push(("items", Json::Arr(items)));
            let total = self.changes.iter().filter(relevant).count();
            if total > shown {
                f.push(("items_omitted", Json::from(total - shown)));
            }
        } else {
            let mut paths: Vec<&str> = Vec::new();
            for c in self.changes.iter().filter(relevant) {
                if paths.len() == 3 {
                    break;
                }
                if !paths.contains(&c.path()) {
                    paths.push(c.path());
                }
            }
            f.push(("paths", Json::arr(paths.into_iter().map(Json::from))));
        }
        if !self.undoes.is_empty() {
            f.push(("undoes", Json::arr(self.undoes.iter().map(|s| Json::from(*s)))));
        }
        if let Some(u) = self.undone_by {
            f.push(("undone_by", Json::from(u)));
        }
        if !self.undoable() {
            f.push(("undoable", Json::from(false)));
        }
        Json::obj(f)
    }
}

/// Whether a change happened at or below `prefix` (`/` matches everything).
fn change_under(c: &Change, prefix: &str) -> bool {
    let prev = match c {
        Change::Set { prev_path: Some(p), .. } => Some(p.as_str()),
        _ => None,
    };
    path_under(c.path(), prefix) || prev.is_some_and(|p| path_under(p, prefix))
}

/// `path` is `prefix` or below it.
pub fn path_under(path: &str, prefix: &str) -> bool {
    prefix == "/" || (path.starts_with(prefix) && matches!(path.as_bytes().get(prefix.len()), None | Some(b'/')))
}

/// One change as a compact feed item.
fn change_json(c: &Change) -> Json {
    let mut f = vec![("op", Json::from(c.op())), ("id", Json::from(c.id())), ("path", Json::from(c.path()))];
    if let Change::Set { prev_path: Some(p), .. } = c {
        f.push(("from", Json::from(p.as_str())));
    }
    if let Some(v) = c.version() {
        f.push(("version", Json::from(v)));
    }
    Json::obj(f)
}

/// Filters for [`Journal::history`]; every given filter must match.
#[derive(Clone, Debug, Default)]
pub struct HistoryQuery {
    pub agent: Option<String>,
    pub path_prefix: Option<String>,
    pub entity: Option<u64>,
    pub tick_from: Option<u64>,
    pub tick_to: Option<u64>,
    pub kinds: Vec<Kind>,
    /// Only entries older than this journal id (paging, newest first).
    pub before: Option<u64>,
    pub limit: usize,
    pub detail: bool,
}

/// A position in the change feed: entries before `seq` are consumed, and the first `index`
/// changes of entry `seq`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub seq: u64,
    pub index: usize,
}

impl Cursor {
    /// `"N"` (everything up to journal id N consumed; `"0"` = from the start) or `"N:I"` (the
    /// first I changes of entry N consumed).
    pub fn parse(s: &str) -> Option<Cursor> {
        match s.split_once(':') {
            None => s.parse::<u64>().ok().and_then(|n| n.checked_add(1)).map(|seq| Cursor { seq, index: 0 }),
            Some((a, b)) => Some(Cursor { seq: a.parse().ok()?, index: b.parse().ok()? }),
        }
    }

    pub fn token(self) -> String {
        if self.index == 0 {
            (self.seq - 1).to_string()
        } else {
            format!("{}:{}", self.seq, self.index)
        }
    }
}

/// [`Journal::changes`]' filters.
#[derive(Clone, Debug)]
pub struct ChangesQuery {
    pub since: Cursor,
    pub limit: usize,
    pub path_prefix: Option<String>,
    /// Include the hosted game's changes.
    pub game: bool,
}

/// What [`Journal::undo`] should undo.
#[derive(Clone, Debug, Default)]
pub struct UndoRequest {
    /// Specific journal ids; `None` = the caller's own most recent `count` undoable entries.
    pub ids: Option<Vec<u64>>,
    pub count: usize,
    /// Undo even when someone else changed the same entities later, or the entries aren't yours.
    pub force: bool,
}

/// What an undo did.
#[derive(Clone, Debug)]
pub struct Undone {
    /// The undo's own journal entry.
    pub seq: u64,
    /// Entries undone, newest first.
    pub undone: Vec<u64>,
    pub changes: usize,
}

/// Entries of one class (agents' or the game's), oldest first, bounded.
struct Ring {
    entries: VecDeque<Entry>,
    bytes: usize,
    max_entries: usize,
    max_bytes: usize,
    /// Entries evicted since the base.
    evicted: u64,
    /// The newest id lost from this ring (evicted, or dropped by a rebase); 0 = none.
    lost_upto: u64,
}

impl Ring {
    fn new(max_entries: usize, max_bytes: usize) -> Ring {
        Ring { entries: VecDeque::new(), bytes: 0, max_entries: max_entries.max(1), max_bytes, evicted: 0, lost_upto: 0 }
    }

    fn push(&mut self, entry: Entry) {
        self.bytes += entry.bytes;
        self.entries.push_back(entry);
        while self.entries.len() > 1 && (self.entries.len() > self.max_entries || self.bytes > self.max_bytes) {
            let old = self.entries.pop_front().expect("more than one entry");
            self.bytes -= old.bytes;
            self.evicted += 1;
            self.lost_upto = old.seq;
        }
    }

    fn get(&self, seq: u64) -> Option<&Entry> {
        self.entries.binary_search_by_key(&seq, |e| e.seq).ok().map(|i| &self.entries[i])
    }

    fn get_mut(&mut self, seq: u64) -> Option<&mut Entry> {
        let i = self.entries.binary_search_by_key(&seq, |e| e.seq).ok()?;
        self.entries.get_mut(i)
    }

    /// Entries with ids at or after `seq`, oldest first.
    fn from(&self, seq: u64) -> std::collections::vec_deque::Iter<'_, Entry> {
        let i = self.entries.partition_point(|e| e.seq < seq);
        self.entries.range(i..)
    }

    /// Entries with ids at or before `seq`, newest first.
    fn upto_rev(&self, seq: u64) -> std::iter::Rev<std::collections::vec_deque::Iter<'_, Entry>> {
        let i = self.entries.partition_point(|e| e.seq <= seq);
        self.entries.range(..i).rev()
    }
}

/// Two id-ordered entry streams as one (ascending, or descending with `newest_first`).
fn merged<'a>(
    a: impl Iterator<Item = &'a Entry>,
    b: impl Iterator<Item = &'a Entry>,
    newest_first: bool,
) -> impl Iterator<Item = &'a Entry> {
    let (mut a, mut b) = (a.peekable(), b.peekable());
    std::iter::from_fn(move || {
        let take_a = match (a.peek(), b.peek()) {
            (Some(x), Some(y)) => (x.seq < y.seq) != newest_first,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => return None,
        };
        if take_a {
            a.next()
        } else {
            b.next()
        }
    })
}

/// The newest game change to an entity (kept after the entry itself is evicted, so undo still
/// sees the conflict).
#[derive(Clone, Debug)]
struct GameTouch {
    seq: u64,
    tick: u64,
    by: Arc<str>,
    path: String,
}

/// See the module docs.
///
/// Agents' entries and the game's live in separate rings with their own bounds, so a busy game
/// (hundreds of updates every tick) never pushes agents' history — and their undo — out.
pub struct Journal {
    agents: Ring,
    game: Ring,
    /// Entity id → the game's newest change to it.
    game_touch: BTreeMap<u64, GameTouch>,
    /// The next entry's id.
    next_seq: u64,
    /// The world the entries apply to, and its hash.
    base: World,
    base_hash: u64,
    /// What the base is: `"empty"` or `"world:<name>"`.
    base_label: String,
    names: BTreeSet<Arc<str>>,
}

impl Default for Journal {
    fn default() -> Self {
        Journal::new(&World::default(), "empty")
    }
}

impl Journal {
    /// A journal of changes made to `base` from now on.
    pub fn new(base: &World, label: &str) -> Journal {
        let mut j = Journal::with_limits(base, label, MAX_ENTRIES, MAX_BYTES);
        j.game = Ring::new(MAX_GAME_ENTRIES, MAX_GAME_BYTES);
        j
    }

    /// Like [`Journal::new`], with the same bounds for both rings.
    pub fn with_limits(base: &World, label: &str, max_entries: usize, max_bytes: usize) -> Journal {
        let mut j = Journal {
            agents: Ring::new(max_entries, max_bytes),
            game: Ring::new(max_entries, max_bytes),
            game_touch: BTreeMap::new(),
            next_seq: 1,
            base: World::default(),
            base_hash: 0,
            base_label: String::new(),
            names: BTreeSet::new(),
        };
        j.rebase(base, label);
        j
    }

    /// Starts over from `base` (after `world_load`): entries are dropped, ids keep counting, so
    /// older feed cursors are reported as a gap.
    pub fn rebase(&mut self, base: &World, label: &str) {
        let last = self.last_seq();
        for ring in [&mut self.agents, &mut self.game] {
            *ring = Ring { lost_upto: last, ..Ring::new(ring.max_entries, ring.max_bytes) };
        }
        self.game_touch.clear();
        let mut copy = base.clone();
        copy.set_recording(Recording::Off);
        copy.take_changes();
        self.base_hash = copy.state_hash();
        self.base = copy;
        self.base_label = label.to_owned();
    }

    fn intern(&mut self, name: &str) -> Arc<str> {
        if let Some(a) = self.names.get(name) {
            return Arc::clone(a);
        }
        let a: Arc<str> = Arc::from(name);
        self.names.insert(Arc::clone(&a));
        a
    }

    /// Records `changes` as one entry; returns its id.
    pub fn append(&mut self, tick: u64, by: &str, via: Option<&str>, kind: Kind, changes: Vec<Change>, next_id: u64) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let mut ids: Vec<u64> = changes.iter().map(Change::id).collect();
        ids.sort_unstable();
        ids.dedup();
        let bytes = std::mem::size_of::<Entry>() + ids.len() * 8 + changes.iter().map(Change::approx_bytes).sum::<usize>();
        let by = self.intern(by);
        if kind.is_game() {
            for c in &changes {
                if let Change::Delete { .. } = c {
                    self.game_touch.remove(&c.id());
                } else {
                    let touch = GameTouch { seq, tick, by: Arc::clone(&by), path: c.path().to_owned() };
                    self.game_touch.insert(c.id(), touch);
                }
            }
        }
        let entry = Entry {
            seq,
            tick,
            by,
            via: via.map(|v| self.intern(v)),
            kind,
            changes,
            next_id,
            undoes: Vec::new(),
            undone_by: None,
            ids,
            bytes,
        };
        if kind.is_game() {
            self.game.push(entry);
        } else {
            self.agents.push(entry);
        }
        seq
    }

    /// Records a game's changes, coalesced to one change per entity (see [`coalesce`]); nothing
    /// is recorded when they cancel out and leave the id counter alone. Returns the entry's id.
    pub fn append_game(
        &mut self,
        tick: u64,
        game: &str,
        via: Option<&str>,
        kind: Kind,
        changes: Vec<Change>,
        next_id: u64,
    ) -> Option<u64> {
        if changes.is_empty() {
            return None;
        }
        let changes = coalesce(changes);
        let previous = self.entries_rev().next().map_or(self.base.next_id(), |e| e.next_id);
        if changes.is_empty() && previous == next_id {
            return None;
        }
        Some(self.append(tick, game, via, kind, changes, next_id))
    }

    pub fn get(&self, seq: u64) -> Option<&Entry> {
        self.agents.get(seq).or_else(|| self.game.get(seq))
    }

    fn get_mut(&mut self, seq: u64) -> Option<&mut Entry> {
        match self.agents.get_mut(seq) {
            Some(e) => Some(e),
            None => self.game.get_mut(seq),
        }
    }

    /// Every entry kept, oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> + '_ {
        merged(self.agents.entries.iter(), self.game.entries.iter(), false)
    }

    /// Every entry kept, newest first.
    fn entries_rev(&self) -> impl Iterator<Item = &Entry> + '_ {
        merged(self.agents.entries.iter().rev(), self.game.entries.iter().rev(), true)
    }

    pub fn len(&self) -> usize {
        self.agents.entries.len() + self.game.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The newest entry's id (0 before the first).
    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }

    /// The oldest entry kept.
    fn oldest_kept(&self) -> Option<u64> {
        self.entries().next().map(|e| e.seq)
    }

    /// Whether every entry since the base is still here, so [`Journal::replay`] can rebuild the
    /// world from the base.
    pub fn replayable(&self) -> bool {
        self.agents.evicted == 0 && self.game.evicted == 0
    }

    pub fn base_hash(&self) -> u64 {
        self.base_hash
    }

    /// The world rebuilt by applying every entry to the base (`None` once entries were evicted).
    pub fn replay(&self) -> Option<World> {
        self.replayable().then(|| replay(self.base.clone(), self.entries()))
    }

    /// Sizes and bounds, for engine_info and world_hash.
    pub fn stats_json(&self) -> Json {
        Json::obj([
            ("first_id", Json::from(self.oldest_kept())),
            ("last_id", Json::from(self.last_seq())),
            ("agent_entries", Json::from(self.agents.entries.len())),
            ("game_entries", Json::from(self.game.entries.len())),
            ("approx_bytes", Json::from(self.agents.bytes + self.game.bytes)),
            ("evicted", Json::from(self.agents.evicted + self.game.evicted)),
            ("replayable", Json::from(self.replayable())),
            ("base", Json::from(self.base_label.as_str())),
            ("base_hash", Json::from(hex(self.base_hash))),
        ])
    }

    /// Entries matching `q`, newest first, one page at a time.
    pub fn history(&self, q: &HistoryQuery) -> Json {
        let mut items = Vec::new();
        let (mut bytes, mut scanned, mut next, mut more) = (0, 0, None, false);
        let newest = q.before.map_or(u64::MAX, |b| b.saturating_sub(1));
        let only_game = !q.kinds.is_empty() && q.kinds.iter().all(|k| k.is_game());
        let no_game = !q.kinds.is_empty() && q.kinds.iter().all(|k| !k.is_game());
        let agents = self.agents.upto_rev(newest).take(if only_game { 0 } else { usize::MAX });
        let game = self.game.upto_rev(newest).take(if no_game { 0 } else { usize::MAX });
        let mut last = None;
        for e in merged(agents, game, true) {
            if scanned == MAX_SCAN {
                // Stop examining; resume from here next time.
                more = true;
                next = last;
                break;
            }
            scanned += 1;
            last = Some(e.seq);
            let matches = q.agent.as_deref().is_none_or(|a| &*e.by == a || e.via.as_deref() == Some(a))
                && q.entity.is_none_or(|id| e.touches(id))
                && q.tick_from.is_none_or(|t| e.tick >= t)
                && q.tick_to.is_none_or(|t| e.tick <= t)
                && (q.kinds.is_empty() || q.kinds.contains(&e.kind))
                && q.path_prefix.as_deref().is_none_or(|p| e.matches_path(p));
            if !matches {
                continue;
            }
            let shown = items.last().and_then(|i: &Json| i.get("id")).and_then(Json::as_u64);
            if items.len() == q.limit {
                more = true;
                next = shown;
                break;
            }
            let item = e.to_json(q.detail, q.path_prefix.as_deref());
            let size = item.to_string().len();
            if !items.is_empty() && bytes + size > MAX_RESPONSE_BYTES {
                more = true;
                next = shown;
                break;
            }
            bytes += size;
            items.push(item);
        }
        Json::obj([
            ("entries", Json::Arr(items)),
            ("next_cursor", Json::from(if more { next } else { None })),
            ("oldest_kept", Json::from(self.oldest_kept())),
            ("evicted", Json::from(self.agents.evicted + self.game.evicted)),
        ])
    }

    /// Changes after `q.since`, oldest first, flattened (one item per change), with the cursor to
    /// pass next time. When changes after the cursor were dropped (journal bounds, or a
    /// world_load) the answer says `gap: true`: re-read the world (world_query) and carry on.
    pub fn changes(&self, q: &ChangesQuery) -> Json {
        Json::parse(&self.changes_text(q)).expect("valid JSON")
    }

    /// [`Journal::changes`] as compact JSON text, written directly: agents poll the feed often,
    /// so each item is serialized once.
    pub fn changes_text(&self, q: &ChangesQuery) -> String {
        let mut at = q.since;
        let gap = at.seq <= self.agents.lost_upto || (q.game && at.seq <= self.game.lost_upto);
        if at.seq > self.next_seq {
            at = Cursor { seq: self.next_seq, index: 0 };
        }
        let mut out = String::from("{\"changes\":[");
        let mut count = 0;
        let mut more = false;
        let game = self.game.from(at.seq).take(if q.game { usize::MAX } else { 0 });
        'outer: for e in merged(self.agents.from(at.seq), game, false) {
            if e.seq > at.seq {
                at = Cursor { seq: e.seq, index: 0 };
            }
            // The entry's shared fields, written once.
            let tail = format!(",\"seq\":{},\"tick\":{},\"by\":{}}}", e.seq, e.tick, Json::from(&*e.by));
            while let Some(c) = e.changes.get(at.index) {
                if count == q.limit {
                    more = true;
                    break 'outer;
                }
                if q.path_prefix.as_deref().is_none_or(|p| change_under(c, p)) {
                    let item = change_json(c).to_string();
                    if count > 0 && out.len() + item.len() + tail.len() > MAX_RESPONSE_BYTES {
                        more = true;
                        break 'outer;
                    }
                    if count > 0 {
                        out.push(',');
                    }
                    out.push_str(&item[..item.len() - 1]); // without its closing brace
                    out.push_str(&tail);
                    count += 1;
                }
                at.index += 1;
            }
            at = Cursor { seq: e.seq + 1, index: 0 };
        }
        if !more {
            at = Cursor { seq: self.next_seq.max(at.seq), index: 0 };
        }
        out.push_str(&format!(
            "],\"next\":{},\"more\":{more},\"gap\":{gap},\"last_id\":{}}}",
            Json::from(at.token()),
            self.last_seq()
        ));
        out
    }

    /// Undoes entries (see [`UndoRequest`]) on `world`, newest first and all or nothing; the
    /// undo is journaled as one entry by `by` at `tick`. `check` vets the resulting changes
    /// before they are kept (the engine checks claims there); an error rolls everything back.
    ///
    /// `world` must have no changes pending.
    pub fn undo(
        &mut self,
        world: &mut World,
        tick: u64,
        by: &str,
        req: &UndoRequest,
        check: &mut Check,
    ) -> Result<Undone, WorldError> {
        let targets = self.select(by, req)?;
        if !req.force {
            self.check_conflicts(world, by, &targets)?;
        }
        let mark = world.mark();
        let previous = world.recording();
        world.set_recording(Recording::Full);
        let mut result = Ok(());
        for &seq in &targets {
            let entry = self.get(seq).expect("selected above");
            if let Err(e) = undo_entry(world, entry, by, req.force) {
                result = Err(e.with("entry", seq));
                break;
            }
        }
        if result.is_ok() {
            result = check(world, &world.pending_changes()[world.mark_changes(mark)..]);
        }
        if let Err(e) = result {
            world.rollback_to(mark);
            world.set_recording(previous);
            return Err(e);
        }
        world.set_recording(previous);
        let changes = world.take_changes();
        let count = changes.len();
        // Mark the targets first: appending may evict old entries.
        let seq = self.next_seq;
        for &t in &targets {
            let redone = self.get(t).map(|e| e.undoes.clone()).unwrap_or_default();
            if let Some(e) = self.get_mut(t) {
                e.undone_by = Some(seq);
            }
            // Undoing an undo redoes what it undid.
            for r in redone {
                if let Some(e) = self.get_mut(r) {
                    e.undone_by = None;
                }
            }
        }
        let appended = self.append(tick, by, None, Kind::Undo, changes, world.next_id());
        debug_assert_eq!(appended, seq);
        if let Some(e) = self.get_mut(seq) {
            e.undoes = targets.clone();
        }
        Ok(Undone { seq, undone: targets, changes: count })
    }

    /// The entries an undo request names, newest first.
    fn select(&self, by: &str, req: &UndoRequest) -> Result<Vec<u64>, WorldError> {
        let mut targets = Vec::new();
        match &req.ids {
            Some(ids) => {
                for &seq in ids {
                    let Some(e) = self.get(seq) else {
                        let err = if seq > self.last_seq() || seq == 0 {
                            WorldError::new("not_found", format!("there is no journal entry {seq}"))
                        } else {
                            WorldError::new(
                                "too_old",
                                format!("journal entry {seq} is no longer kept (the journal keeps the newest entries only)"),
                            )
                        };
                        return Err(err.with("entry", seq).with("oldest_kept", Json::from(self.oldest_kept())));
                    };
                    if &*e.by != by && !req.force {
                        return Err(WorldError::new(
                            "not_yours",
                            format!("journal entry {seq} was made by {}; pass force: true to undo someone else's change", e.by),
                        )
                        .with("entry", seq)
                        .with("by", &*e.by));
                    }
                    if let Some(u) = e.undone_by {
                        return Err(WorldError::new("already_undone", format!("journal entry {seq} was already undone by entry {u}"))
                            .with("entry", seq)
                            .with("undone_by", u));
                    }
                    if !e.undoable() {
                        return Err(WorldError::new(
                            "not_undoable",
                            format!("journal entry {seq} ({}) kept no previous state: game updates are replay-only", e.kind.name()),
                        )
                        .with("entry", seq));
                    }
                    if !targets.contains(&seq) {
                        targets.push(seq);
                    }
                }
                targets.sort_unstable_by(|a, b| b.cmp(a));
            }
            None => {
                targets = self
                    .agents
                    .entries
                    .iter()
                    .rev()
                    .filter(|e| &*e.by == by && e.undone_by.is_none() && e.kind != Kind::Undo && e.undoable())
                    .take(req.count)
                    .map(|e| e.seq)
                    .collect();
            }
        }
        if targets.is_empty() {
            return Err(WorldError::new("nothing_to_undo", format!("{by} has no undoable changes in the journal"))
                .with("agent", by));
        }
        Ok(targets)
    }

    /// Refuses when someone else (another agent, or the game) changed what an entry touched
    /// after it, or something now below an entity it spawned.
    fn check_conflicts(&self, world: &World, by: &str, targets: &[u64]) -> Result<(), WorldError> {
        let conflict = |seq: u64, id: u64, path: &str, later: u64, later_by: &str, later_tick: u64| {
            WorldError::new(
                "undo_conflict",
                format!(
                    "{later_by} changed {} later (journal entry {later}, tick {later_tick}); undoing entry {seq} would \
                     overwrite it. Pass force: true to undo anyway",
                    if id == 0 { "the environment" } else { path },
                ),
            )
            .with("entry", seq)
            .with("entity", id)
            .with("path", path)
            .with("later_entry", later)
            .with("later_by", later_by)
            .with("later_tick", later_tick)
        };
        for &seq in targets {
            let e = self.get(seq).expect("selected");
            let mut ids: BTreeSet<u64> = e.ids.iter().copied().collect();
            for c in &e.changes {
                if let Change::Spawn { after, .. } = c {
                    ids.extend(world.subtree_children_first(after.id));
                }
            }
            for &id in &ids {
                if let Some(g) = self.game_touch.get(&id).filter(|g| g.seq > seq && &*g.by != by) {
                    return Err(conflict(seq, id, &g.path, g.seq, &g.by, g.tick));
                }
            }
            for later in self.agents.from(seq + 1) {
                if &*later.by == by || targets.contains(&later.seq) {
                    continue;
                }
                let hit = if later.ids.len() < ids.len() {
                    later.ids.iter().find(|id| ids.contains(id)).copied()
                } else {
                    ids.iter().find(|id| later.touches(**id)).copied()
                };
                if let Some(id) = hit {
                    let path = later.changes.iter().find(|c| c.id() == id).map_or("", |c| c.path());
                    return Err(conflict(seq, id, path, later.seq, &later.by, later.tick));
                }
            }
        }
        Ok(())
    }
}

/// Puts back the state before each of `entry`'s changes, newest change first.
fn undo_entry(world: &mut World, entry: &Entry, by: &str, force: bool) -> Result<(), WorldError> {
    let by: Arc<str> = Arc::from(by);
    // A restored entity is a new version of whatever is there now.
    let restore = |world: &World, before: &Entity| -> Entity {
        let mut e = before.clone();
        let current = world.get(e.id).map_or(0, |c| c.version);
        e.version = current.max(before.version) + 1;
        e.modified_by = Arc::clone(&by);
        e
    };
    for change in entry.changes.iter().rev() {
        match change {
            Change::Spawn { after, path } => {
                if world.get(after.id).is_none() {
                    continue; // already gone
                }
                if world.has_children(after.id) && !force {
                    return Err(WorldError::new(
                        "has_children",
                        format!("{path} has children added since; undo those first, or pass force: true to remove them too"),
                    )
                    .with("path", path.as_str())
                    .with("id", after.id));
                }
                for id in world.subtree_children_first(after.id) {
                    world.remove_raw(id);
                }
            }
            Change::Set { before: Some(before), path, .. } => {
                if world.get(before.id).is_none() && !force {
                    return Err(WorldError::new(
                        "gone",
                        format!("{path} was deleted since; pass force: true to bring back its earlier state"),
                    )
                    .with("path", path.as_str())
                    .with("id", before.id));
                }
                let e = restore(world, before);
                world.check_put(&e)?;
                world.put_raw(e);
            }
            Change::Set { before: None, .. } => {
                return Err(WorldError::new("not_undoable", "this entry kept no previous state"));
            }
            Change::Delete { before, path } => {
                if world.get(before.id).is_some() {
                    continue; // already back
                }
                let e = restore(world, before);
                world.check_put(&e).map_err(|err| err.with("restoring", path.as_str()))?;
                world.put_raw(e);
            }
            Change::Env { before, .. } => world.replace_env(*before),
        }
    }
    Ok(())
}

/// Folds a game's changes to at most one per entity, in the order each entity was first
/// touched: spawn + sets → one spawn of the final state; sets → one set (first `before`, last
/// `after`); anything + delete → one delete (a spawn + delete leaves nothing); environment edits
/// → one. Replaying the result gives the same world as replaying the originals.
pub fn coalesce(changes: Vec<Change>) -> Vec<Change> {
    if changes.len() < 2 {
        return changes;
    }
    // id → slot in `out` (ordered map: no hash iteration order anywhere).
    let mut slot: BTreeMap<u64, usize> = BTreeMap::new();
    let mut out: Vec<Option<Change>> = Vec::with_capacity(changes.len());
    for c in changes {
        let id = c.id();
        let Some(&i) = slot.get(&id) else {
            slot.insert(id, out.len());
            out.push(Some(c));
            continue;
        };
        let prev = out[i].take();
        out[i] = match (prev, c) {
            (None, c) => Some(c),
            (Some(Change::Spawn { .. }), Change::Set { path, after, .. }) => Some(Change::Spawn { path, after }),
            (Some(Change::Spawn { .. }), Change::Delete { .. }) => None,
            (Some(Change::Set { before, prev_path, .. }), Change::Set { path, prev_path: p2, after, .. }) => {
                let prev_path = prev_path.or(p2).filter(|p| *p != path);
                Some(Change::Set { path, prev_path, before, after })
            }
            (Some(Change::Set { before: Some(before), .. }), Change::Delete { path, .. }) => {
                Some(Change::Delete { path, before })
            }
            (Some(Change::Set { .. }), d @ Change::Delete { .. }) => Some(d),
            (Some(Change::Env { before, .. }), Change::Env { after, .. }) => Some(Change::Env { before, after }),
            // Anything after a delete (an id coming back) or other unusual sequences: keep the
            // latest state, which is what replay needs.
            (Some(_), c) => Some(c),
        };
    }
    out.into_iter().flatten().collect()
}

/// `base` with every entry's changes applied in order: their states afterwards and the id
/// counter. Applying an engine's journal to its base reproduces its world exactly.
pub fn replay<'a>(mut base: World, entries: impl IntoIterator<Item = &'a Entry>) -> World {
    base.set_recording(Recording::Off);
    for e in entries {
        for c in &e.changes {
            match c {
                Change::Spawn { after, .. } | Change::Set { after, .. } => base.put_raw((**after).clone()),
                Change::Delete { before, .. } => {
                    base.remove_raw(before.id);
                }
                Change::Env { after, .. } => base.replace_env(*after),
            }
        }
        base.replay_next_id(e.next_id);
    }
    base
}

pub fn hex(h: u64) -> String {
    format!("{h:016x}")
}

/// Applies an agent's command to `world` with `check` vetting what it changed, then journals it.
/// On a failed command or check nothing changes (the world is rolled back). Returns the
/// command's result and the journal id (`None` when nothing changed).
pub fn commit<T>(
    world: &mut World,
    journal: &mut Journal,
    tick: u64,
    by: &str,
    kind: Kind,
    check: &mut Check,
    command: impl FnOnce(&mut World) -> Result<T, WorldError>,
) -> Result<(T, Option<u64>), WorldError> {
    let mark = world.mark();
    let out = command(world);
    let out = match out {
        Ok(v) => {
            let pending = &world.pending_changes()[world.mark_changes(mark)..];
            match check(world, pending) {
                Ok(()) => Ok(v),
                Err(e) => {
                    world.rollback_to(mark);
                    Err(e)
                }
            }
        }
        Err(e) => {
            // Commands change nothing when they fail (batches roll themselves back); this is a
            // safety net.
            world.rollback_to(mark);
            Err(e)
        }
    }?;
    let changes = world.take_changes();
    let seq = (!changes.is_empty()).then(|| journal.append(tick, by, None, kind, changes, world.next_id()));
    Ok((out, seq))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{EntityKey, EnvPatch, Op, Shape, SpawnSpec, UpdateSpec};
    use specula_core::math::Vec3;

    fn no_check(_: &World, _: &[Change]) -> Result<(), WorldError> {
        Ok(())
    }

    fn live() -> (World, Journal) {
        let mut w = World::default();
        w.set_recording(Recording::Full);
        (w, Journal::default())
    }

    fn spec(path: &str) -> SpawnSpec {
        let mut s = SpawnSpec::new(Shape::Cube);
        let (parent, name) = path.rsplit_once('/').unwrap();
        s.name = Some(name.into());
        s.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.into()));
        s
    }

    fn spawn(w: &mut World, j: &mut Journal, by: &str, path: &str) -> u64 {
        commit(w, j, 0, by, Kind::Spawn, &mut no_check, |w| w.spawn(spec(path), by).map(|e| e.id)).unwrap().0
    }

    fn set(w: &mut World, j: &mut Journal, by: &str, path: &str, change: UpdateSpec) -> Option<u64> {
        commit(w, j, 0, by, Kind::Set, &mut no_check, |w| w.update(&EntityKey::Path(path.into()), change, by).map(|_| ()))
            .unwrap()
            .1
    }

    fn moved(x: f32) -> UpdateSpec {
        UpdateSpec { position: Some(Vec3::new(x, 0.0, 0.0)), ..Default::default() }
    }

    fn undo(w: &mut World, j: &mut Journal, by: &str, req: UndoRequest) -> Result<Undone, WorldError> {
        j.undo(w, 0, by, &req, &mut no_check)
    }

    fn last(n: usize) -> UndoRequest {
        UndoRequest { ids: None, count: n, force: false }
    }

    fn batch(w: &mut World, j: &mut Journal, by: &str, ops: Vec<Op>) -> Result<Option<u64>, WorldError> {
        commit(w, j, 1, by, Kind::Batch, &mut no_check, |w| w.apply_batch(ops, by).map_err(|(_, e)| e)).map(|(_, s)| s)
    }

    /// A tiny deterministic generator for the scripted scenarios.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: u64) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) % n
        }
    }

    #[test]
    fn replay_reproduces_every_kind_of_change() {
        let (mut w, mut j) = live();
        spawn(&mut w, &mut j, "ann", "/d1");
        spawn(&mut w, &mut j, "ann", "/d1/a");
        spawn(&mut w, &mut j, "bob", "/d2");
        set(&mut w, &mut j, "bob", "/d1/a", UpdateSpec { name: Some("b".into()), ..moved(3.0) });
        let reparent = UpdateSpec { parent: Some(Some(EntityKey::Path("/d2".into()))), ..Default::default() };
        set(&mut w, &mut j, "bob", "/d1/b", reparent);
        let ops = vec![Op::Spawn(spec("/d3")), Op::Spawn(spec("/d3/x")), Op::Set(EntityKey::Path("/d3/x".into()), moved(1.0))];
        batch(&mut w, &mut j, "carl", ops).unwrap();
        // A failing batch leaves no trace in the world or the journal.
        let bad = vec![Op::Spawn(spec("/d4")), Op::Delete(EntityKey::Path("/nope".into()), None)];
        assert!(batch(&mut w, &mut j, "carl", bad).is_err());
        let env = EnvPatch { exposure: Some(2.0), ..Default::default() };
        commit(&mut w, &mut j, 2, "ann", Kind::Env, &mut no_check, |w| w.set_env(&env)).unwrap();
        let del = |w: &mut World| w.delete(&EntityKey::Path("/d3".into()), None).map(|_| ());
        commit(&mut w, &mut j, 2, "ann", Kind::Delete, &mut no_check, del).unwrap();
        undo(&mut w, &mut j, "ann", last(1)).unwrap(); // the delete comes back
        assert!(w.find_path("/d3/x").is_ok());
        let replayed = j.replay().unwrap();
        assert_eq!(replayed.state_hash(), w.state_hash());
        assert_eq!(replayed.len(), w.len());
        assert_eq!(replayed.next_id(), w.next_id());
        assert_eq!(replayed.find_path("/d2/b"), w.find_path("/d2/b"));
    }

    #[test]
    fn coalesced_game_ticks_replay_like_the_originals() {
        // Random game-style edits: spawns, moves, renames (including swaps), reparents, deletes.
        let mut rng = Lcg(7);
        let (mut w, mut j) = live();
        let mut plain = World::default();
        for tick in 0..80u64 {
            w.set_recording(Recording::AfterOnly);
            for _ in 0..rng.next(12) {
                let ids: Vec<u64> = w.entities().map(|e| e.id).collect();
                let pick = |rng: &mut Lcg| ids[rng.next(ids.len() as u64) as usize];
                match (rng.next(8), ids.is_empty()) {
                    (0..=2, _) | (_, true) => {
                        let mut s = SpawnSpec::new(Shape::Cube);
                        if !ids.is_empty() && rng.next(2) == 0 {
                            s.parent = Some(EntityKey::Id(pick(&mut rng)));
                        }
                        let _ = w.spawn(s, "game");
                    }
                    (3 | 4, _) => {
                        let _ = w.update(&EntityKey::Id(pick(&mut rng)), moved(rng.next(100) as f32), "game");
                    }
                    (5, _) => {
                        let name = Some(format!("n{}", rng.next(4)));
                        let _ = w.update(&EntityKey::Id(pick(&mut rng)), UpdateSpec { name, ..Default::default() }, "game");
                    }
                    (6, _) => {
                        let parent = if rng.next(3) == 0 { None } else { Some(EntityKey::Id(pick(&mut rng))) };
                        let change = UpdateSpec { parent: Some(parent), ..Default::default() };
                        let _ = w.update(&EntityKey::Id(pick(&mut rng)), change, "game");
                    }
                    _ => {
                        let _ = w.delete(&EntityKey::Id(pick(&mut rng)), None);
                    }
                }
            }
            let changes = w.take_changes();
            // The uncoalesced changes, replayed one by one, as a reference.
            let mut one = Journal::default();
            one.append(tick, "game", None, Kind::GameTick, changes.clone(), w.next_id());
            plain = replay(plain, one.entries());
            j.append_game(tick, "game", None, Kind::GameTick, changes, w.next_id());
            assert_eq!(plain.state_hash(), w.state_hash(), "tick {tick}: uncoalesced");
            assert_eq!(j.replay().unwrap().state_hash(), w.state_hash(), "tick {tick}: coalesced");
        }
        assert!(w.len() > 5, "the scenario built something ({} entities)", w.len());
        assert!(j.entries().all(|e| e.changes.len() == e.ids().len()), "one change per entity per tick");
        assert!(j.entries().all(|e| !e.undoable() || e.changes.iter().all(|c| !matches!(c, Change::Set { .. }))));
    }

    #[test]
    fn undo_is_own_newest_first_and_journaled() {
        let (mut w, mut j) = live();
        spawn(&mut w, &mut j, "ann", "/a");
        set(&mut w, &mut j, "ann", "/a", moved(1.0));
        spawn(&mut w, &mut j, "bob", "/b");
        set(&mut w, &mut j, "ann", "/a", moved(2.0));
        let u = undo(&mut w, &mut j, "ann", last(1)).unwrap();
        assert_eq!(u.undone, [4]);
        let a = w.get(w.find_path("/a").unwrap()).unwrap();
        assert_eq!((a.position.x, a.version), (1.0, 4), "restored state, newer version");
        assert_eq!(&*a.modified_by, "ann");
        // The next undo goes further back, skipping the undo itself and bob's entry.
        let u2 = undo(&mut w, &mut j, "ann", last(1)).unwrap();
        assert_eq!(u2.undone, [2]);
        assert_eq!(w.get(w.find_path("/a").unwrap()).unwrap().position.x, 0.0);
        // Undoing the undo redoes it.
        let redo = undo(&mut w, &mut j, "ann", UndoRequest { ids: Some(vec![u2.seq]), ..last(1) }).unwrap();
        assert_eq!(w.get(w.find_path("/a").unwrap()).unwrap().position.x, 1.0);
        assert_eq!(j.get(2).unwrap().undone_by, None, "redone");
        assert_eq!(j.get(redo.seq).unwrap().kind, Kind::Undo);
        let named = |ids: Vec<u64>| UndoRequest { ids: Some(ids), ..last(1) };
        assert_eq!(undo(&mut w, &mut j, "ann", named(vec![3])).unwrap_err().code, "not_yours");
        assert_eq!(undo(&mut w, &mut j, "ann", named(vec![99])).unwrap_err().code, "not_found");
        assert_eq!(undo(&mut w, &mut j, "ann", named(vec![u.seq - 1])).unwrap_err().code, "already_undone");
        assert_eq!(undo(&mut w, &mut j, "nobody", last(1)).unwrap_err().code, "nothing_to_undo");
        assert_eq!(j.replay().unwrap().state_hash(), w.state_hash());
    }

    #[test]
    fn undo_refuses_over_someone_elses_later_change_unless_forced() {
        let (mut w, mut j) = live();
        spawn(&mut w, &mut j, "ann", "/a");
        set(&mut w, &mut j, "ann", "/a", moved(1.0));
        set(&mut w, &mut j, "bob", "/a", moved(5.0));
        let err = undo(&mut w, &mut j, "ann", last(1)).unwrap_err();
        assert_eq!((err.code, err.detail("later_by")), ("undo_conflict", Some(&Json::from("bob"))));
        assert_eq!(w.get(1).unwrap().position.x, 5.0, "nothing changed");
        undo(&mut w, &mut j, "ann", UndoRequest { force: true, ..last(1) }).unwrap();
        assert_eq!(w.get(1).unwrap().position.x, 0.0);
        // Children someone else added under my spawn are a conflict too.
        spawn(&mut w, &mut j, "ann", "/p");
        spawn(&mut w, &mut j, "bob", "/p/kid");
        let err = undo(&mut w, &mut j, "ann", last(1)).unwrap_err();
        assert_eq!(err.code, "undo_conflict");
        undo(&mut w, &mut j, "ann", UndoRequest { force: true, ..last(1) }).unwrap();
        assert!(w.find_path("/p").is_err() && w.find_path("/p/kid").is_err(), "forced: the subtree goes");
        assert_eq!(j.replay().unwrap().state_hash(), w.state_hash());
    }

    #[test]
    fn a_batch_undoes_as_a_unit_and_failures_roll_back() {
        let (mut w, mut j) = live();
        let ops = vec![Op::Spawn(spec("/g")), Op::Spawn(spec("/g/a")), Op::Spawn(spec("/g/b"))];
        batch(&mut w, &mut j, "ann", ops).unwrap();
        let del = |w: &mut World| w.delete(&EntityKey::Path("/g".into()), None).map(|_| ());
        commit(&mut w, &mut j, 0, "ann", Kind::Delete, &mut no_check, del).unwrap();
        // Something else takes the name /g meanwhile: bringing the deleted /g back must fail cleanly.
        spawn(&mut w, &mut j, "ann", "/g");
        let before = w.state_hash();
        let err = undo(&mut w, &mut j, "ann", UndoRequest { ids: Some(vec![2]), ..last(1) }).unwrap_err();
        assert_eq!(err.code, "name_taken");
        assert_eq!(w.state_hash(), before, "rolled back");
        assert!(w.pending_changes().is_empty());
        undo(&mut w, &mut j, "ann", last(1)).unwrap(); // the new /g
        undo(&mut w, &mut j, "ann", last(1)).unwrap(); // the delete: the whole subtree is back
        assert_eq!(w.len(), 3);
        undo(&mut w, &mut j, "ann", last(1)).unwrap(); // the batch: all three go
        assert!(w.is_empty());
        assert_eq!(j.replay().unwrap().state_hash(), w.state_hash());
    }

    #[test]
    fn a_rejecting_check_rolls_the_command_back() {
        let (mut w, mut j) = live();
        spawn(&mut w, &mut j, "ann", "/a");
        let before = (w.state_hash(), w.revision(), w.next_id());
        let mut refuse = |_: &World, _: &[Change]| Err(WorldError::new("claimed", "no"));
        let ops = vec![
            Op::Spawn(spec("/b")),
            Op::Set(EntityKey::Path("/a".into()), moved(1.0)),
            Op::Delete(EntityKey::Path("/a".into()), None),
        ];
        let err = commit(&mut w, &mut j, 0, "bob", Kind::Batch, &mut refuse, |w| w.apply_batch(ops, "bob").map_err(|(_, e)| e));
        assert_eq!(err.unwrap_err().code, "claimed");
        assert_eq!((w.state_hash(), w.revision(), w.next_id()), before);
        assert_eq!(w.get(1).unwrap().version, 1);
        assert_eq!(w.find_path("/a"), Ok(1));
        assert_eq!(j.len(), 1, "nothing journaled");
    }

    #[test]
    fn the_ring_is_bounded_and_says_so() {
        let mut w = World::default();
        w.set_recording(Recording::Full);
        let mut j = Journal::with_limits(&w, "empty", 4, usize::MAX);
        for i in 0..6 {
            spawn(&mut w, &mut j, "ann", &format!("/e{i}"));
        }
        assert_eq!((j.len(), j.last_seq(), j.replayable()), (4, 6, false));
        assert!(j.replay().is_none());
        let err = undo(&mut w, &mut j, "ann", UndoRequest { ids: Some(vec![1]), ..last(1) }).unwrap_err();
        assert_eq!(err.code, "too_old");
        let feed = j.changes(&ChangesQuery { since: Cursor::parse("0").unwrap(), limit: 10, path_prefix: None, game: true });
        assert_eq!(feed.get("gap"), Some(&Json::from(true)));
        assert_eq!(feed.get("changes").unwrap().as_array().unwrap().len(), 4);
        let mut j = Journal::with_limits(&w, "empty", 1000, 1);
        spawn(&mut w, &mut j, "ann", "/x");
        assert_eq!(j.len(), 1, "the newest entry is always kept");
    }

    #[test]
    fn a_busy_game_never_pushes_agents_history_out() {
        let (mut w, _) = live();
        let mut j = Journal::with_limits(&w, "empty", 8, usize::MAX);
        spawn(&mut w, &mut j, "ann", "/a");
        set(&mut w, &mut j, "ann", "/a", moved(1.0));
        for t in 0..50 {
            w.set_recording(Recording::AfterOnly);
            w.update(&EntityKey::Path("/a".into()), moved(t as f32), "game").unwrap();
            w.set_recording(Recording::Full);
            let changes = w.take_changes();
            j.append_game(t, "game", None, Kind::GameTick, changes, w.next_id());
        }
        assert_eq!(j.get(1).map(|e| e.kind), Some(Kind::Spawn), "agents' entries stay");
        assert!(!j.replayable(), "game entries were evicted");
        // The game's evicted change still blocks an unforced undo.
        let err = undo(&mut w, &mut j, "ann", last(1)).unwrap_err();
        assert_eq!((err.code, err.detail("later_by")), ("undo_conflict", Some(&Json::from("game"))));
        // History can skip the game; the feed reports the game's gap only to those following it.
        let h = j.history(&HistoryQuery { limit: 10, kinds: vec![Kind::Spawn, Kind::Set], ..Default::default() });
        assert_eq!(h.get("entries").unwrap().as_array().unwrap().len(), 2);
        let feed = |game| j.changes(&ChangesQuery { since: Cursor::parse("0").unwrap(), limit: 100, path_prefix: None, game });
        assert_eq!(feed(false).get("gap"), Some(&Json::from(false)));
        assert_eq!(feed(false).get("changes").unwrap().as_array().unwrap().len(), 2);
        assert_eq!(feed(true).get("gap"), Some(&Json::from(true)));
        assert_eq!(feed(false).get("next"), Some(&Json::from(j.last_seq().to_string())));
        // A load starts over: cursors from before it see a gap.
        let mut rebased = j;
        rebased.rebase(&w, "world:test");
        assert_eq!(rebased.len(), 0);
        let after = rebased.changes(&ChangesQuery { since: Cursor::parse("2").unwrap(), limit: 10, path_prefix: None, game: false });
        assert_eq!(after.get("gap"), Some(&Json::from(true)));
        assert_eq!(rebased.replay().unwrap().state_hash(), w.state_hash(), "replays from the loaded base");
    }

    #[test]
    fn change_feed_pages_inside_entries() {
        let (mut w, mut j) = live();
        let ops: Vec<Op> = (0..5).map(|i| Op::Spawn(spec(&format!("/b{i}")))).collect();
        batch(&mut w, &mut j, "ann", ops).unwrap();
        spawn(&mut w, &mut j, "bob", "/c");
        let q = |since: &str| ChangesQuery { since: Cursor::parse(since).unwrap(), limit: 2, path_prefix: None, game: true };
        let p1 = j.changes(&q("0"));
        assert_eq!(p1.get("next"), Some(&Json::from("1:2")));
        assert_eq!(p1.get("more"), Some(&Json::from(true)));
        assert_eq!(p1.get("gap"), Some(&Json::from(false)));
        let item = &p1.get("changes").unwrap().as_array().unwrap()[0];
        assert_eq!(item.to_string(), r#"{"op":"spawn","id":1,"path":"/b0","version":1,"seq":1,"tick":1,"by":"ann"}"#);
        let mut seen = 2;
        let mut next = "1:2".to_string();
        loop {
            let page = j.changes(&q(&next));
            seen += page.get("changes").unwrap().as_array().unwrap().len();
            next = page.get("next").unwrap().as_str().unwrap().to_owned();
            if page.get("more") != Some(&Json::from(true)) {
                break;
            }
        }
        assert_eq!((seen, next.as_str()), (6, "2"));
        let quiet = j.changes(&q("2"));
        assert_eq!((quiet.get("changes").unwrap().as_array().unwrap().len(), quiet.get("next")), (0, Some(&Json::from("2"))));
        let only = j.changes(&ChangesQuery { path_prefix: Some("/c".into()), ..q("0") });
        assert_eq!(only.get("changes").unwrap().as_array().unwrap().len(), 1);
        assert_eq!(Cursor::parse("x"), None);
    }

    #[test]
    fn history_filters_and_pages_newest_first() {
        let (mut w, mut j) = live();
        for i in 0..5 {
            spawn(&mut w, &mut j, if i % 2 == 0 { "ann" } else { "bob" }, &format!("/e{i}"));
        }
        set(&mut w, &mut j, "bob", "/e0", moved(1.0));
        let ids = |h: &Json| -> Vec<u64> {
            h.get("entries").unwrap().as_array().unwrap().iter().map(|e| e.get("id").unwrap().as_u64().unwrap()).collect()
        };
        let q = |limit: usize| HistoryQuery { limit, ..Default::default() };
        let all = j.history(&q(4));
        assert_eq!(ids(&all), [6, 5, 4, 3]);
        assert_eq!(all.get("next_cursor"), Some(&Json::from(3)));
        assert_eq!(ids(&j.history(&HistoryQuery { before: Some(3), ..q(4) })), [2, 1]);
        assert_eq!(ids(&j.history(&HistoryQuery { agent: Some("ann".into()), ..q(9) })), [5, 3, 1]);
        assert_eq!(ids(&j.history(&HistoryQuery { entity: Some(1), ..q(9) })), [6, 1]);
        assert_eq!(ids(&j.history(&HistoryQuery { path_prefix: Some("/e0".into()), ..q(9) })), [6, 1]);
        assert_eq!(ids(&j.history(&HistoryQuery { kinds: vec![Kind::Set], ..q(9) })), [6]);
        assert_eq!(j.history(&q(9)).get("next_cursor"), Some(&Json::Null));
        let detail = j.history(&HistoryQuery { detail: true, ..q(1) });
        let item = &detail.get("entries").unwrap().as_array().unwrap()[0];
        assert_eq!(item.get("items").unwrap().to_string(), r#"[{"op":"set","id":1,"path":"/e0","version":2}]"#);
    }
}
