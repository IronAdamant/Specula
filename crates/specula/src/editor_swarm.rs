//! The editor's swarm panels (docs/design/EDITOR.md, "Swarm panels"): what the swarm is doing,
//! in the window, beside the world it is doing it to.
//!
//! - **Journal timeline:** journal entries, newest first, paged back through the journal and
//!   filtered by agent, by the selected entity (its identity) or the selected entity's path.
//!   Selecting an entry highlights what it touched in the viewport, with the agent's name:
//!   "which agent built this".
//! - **Claims:** every claim and lease with its holder and time left; the claim map (a toolbar
//!   toggle) draws each claim's scope in the viewport with the holder's name.
//! - **Board:** tasks, intents and notes, live.
//! - **Review:** the pending submissions; one opens in the review screen (F9) to decide.
//! - **Cost:** per agent, the calls the engine handled, the commits charged to them and their
//!   refused writes (tokens only if the harness reports them: none does yet); per task, its
//!   progress and work.
//!
//! Every panel is read-only: an agent's data is never changed from here (decisions are the review
//! screen's; edits are the editor's protocol commands). Panels are filled on change, at most four
//! times a second, a page at a time, each from a bounded read of the engine's own stores; no world
//! walk. Agents see the same panels through `ui_tree`/`ui_act` on the `editor` surface and
//! `editor_state {panel}`.

use crate::entity_ref::EntityRef;
use crate::journal::Entry;
use crate::Engine;
use specula_core::Json;
use specula_ui::StateMap;
use std::time::{Duration, Instant};

/// Rows on one page (the screen's `repeat.max`).
pub const ROWS: usize = 10;
/// Journal entries one refresh examines while filtering (it says so when it stops there), and
/// the changes in them it may look at (a path filter reads each change's path).
pub const MAX_SCAN: usize = 20_000;
pub const MAX_SCAN_CHANGES: usize = 100_000;
/// Entities one selected row highlights in the viewport.
pub const MAX_HIGHLIGHT: usize = 64;
/// The longest row label (characters: about a line and a half of the 24 em dock in the UI's
/// font; a label wraps inside its row, so wide letters never stick out; the rest is in the
/// selected row's detail) and detail text. (Review R1 of m5-edswarm, minor 3: at 96 characters,
/// unwrapped, a row's text stuck out of its button by 787 px at 1280x800. At 34 the cost view's
/// rows lost their refused counts: the capture check of the fix.)
const ROW_CHARS: usize = 56;
const DETAIL_CHARS: usize = 900;
/// Panels are rebuilt at most this often while their data keeps changing.
const REFRESH_EVERY: Duration = Duration::from_millis(250);
/// Agents the cost view names (the busiest first).
const MAX_AGENT_ROWS: usize = 64;
/// Claims the claim map draws per frame (each a path lookup and a box): past this, the first ones.
pub const MAX_CLAIM_MAP: usize = 256;
/// Names the claim map writes per frame.
const MAX_CLAIM_NAMES: usize = 48;
/// Leases and intents one refresh lists (a page is cut from these).
const LIST_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Journal,
    Claims,
    Board,
    Review,
    Cost,
}

impl View {
    pub fn name(self) -> &'static str {
        match self {
            View::Journal => "journal",
            View::Claims => "claims",
            View::Board => "board",
            View::Review => "review",
            View::Cost => "cost",
        }
    }

    pub fn parse(s: &str) -> Option<View> {
        [View::Journal, View::Claims, View::Board, View::Review, View::Cost].into_iter().find(|v| v.name() == s)
    }
}

/// What the board view lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoardShow {
    #[default]
    Tasks,
    Intents,
    Notes,
}

impl BoardShow {
    pub fn name(self) -> &'static str {
        match self {
            BoardShow::Tasks => "tasks",
            BoardShow::Intents => "intents",
            BoardShow::Notes => "notes",
        }
    }
}

/// The journal timeline's place filter.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Place {
    #[default]
    All,
    /// Entries that touched this entity (by identity: the selection when it was chosen).
    Entity(EntityRef),
    /// Entries with a change at or under this path.
    Path(String),
}

/// What a row stands for (what selecting it shows).
#[derive(Clone, Debug, PartialEq)]
pub enum RowKey {
    Entry(u64),
    Claim(u64),
    Lease(u64),
    Task(u64),
    Intent(u64),
    Note(u64),
    Submission(u64),
    Agent(String),
}

/// One page of a swarm view: its rows, whether more follow, a note for the status line, and
/// (the journal's pages) the agents its entries name and where an older page goes on from.
type PageOf = (Vec<Row>, bool, String, Option<(Vec<String>, Option<u64>)>);

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: RowKey,
    pub label: String,
    pub badge: String,
}

/// What a press on the swarm panel asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum SwAct {
    /// The right dock: the swarm panels (`true`) or the inspector.
    Shown(bool),
    View(View),
    Select(usize),
    Newest,
    Older,
    Newer,
    /// Cycles the journal's agent filter through the agents in the journal page.
    CycleAgent,
    /// Cycles the journal's place filter: all, the selected entity, its path.
    CyclePlace,
    ToggleGame,
    CycleBoard,
    ClaimMap,
    OpenReview,
}

/// The panels' state: what they show (filled by the engine) and how (the human's choices).
#[derive(Default)]
pub struct Panel {
    /// The right dock shows the swarm panels instead of the inspector.
    pub shown: bool,
    pub view: View,
    /// Journal paging: the newest entry of each page so far (the first page starts at the
    /// newest entry: `u64::MAX`).
    pub pages: Vec<u64>,
    /// Other views' page.
    pub page: usize,
    pub agent: Option<String>,
    pub place: Place,
    /// The hosted game's own entries too (ticks and frames: many).
    pub game: bool,
    pub board: BoardShow,
    pub rows: Vec<Row>,
    pub has_next: bool,
    pub selected: Option<RowKey>,
    pub detail: String,
    pub note: String,
    /// The claim map: every claim drawn in the viewport with its holder's name.
    pub claim_map: bool,
    /// Entities the selected row highlights in the viewport, and the label they get.
    pub highlight: Vec<u64>,
    pub highlight_label: String,
    /// Agents seen on the last journal page (the agent filter cycles through them).
    pub agents_seen: Vec<String>,
    /// Where the next journal page starts (the entry the last one stopped at).
    pub resume_at: Option<u64>,
    pub(crate) built: Option<(Vec<u64>, Instant)>,
}

impl Panel {
    pub(crate) fn page_label(&self) -> String {
        match self.view {
            // `pages` holds where each page after the first starts.
            View::Journal => format!("page {}", self.pages.len() + 1),
            _ => format!("page {}", self.page + 1),
        }
    }

    pub fn has_prev(&self) -> bool {
        match self.view {
            View::Journal => !self.pages.is_empty(),
            _ => self.page > 0,
        }
    }

    /// The panels' state for the screen (`EditorUi::fill`).
    pub fn fill(&self, s: &mut StateMap) {
        s.set_bool("tab_swarm", self.shown);
        s.set_bool("tab_inspector", !self.shown);
        for v in [View::Journal, View::Claims, View::Board, View::Review, View::Cost] {
            s.set_bool(&format!("sw_is_{}", v.name()), self.view == v);
        }
        s.set_bool("claim_map", self.claim_map);
        s.set_str(
            "sw_title",
            match self.view {
                View::Journal => "Journal: who changed what, newest first",
                View::Claims => "Claims and leases",
                View::Board => "Task board",
                View::Review => "Review queue (pending)",
                View::Cost => "Cost and progress",
            },
        );
        let agent = format!("Agent: {}", self.agent.as_deref().unwrap_or("all"));
        s.set_str("sw_agent", &agent);
        let place = match &self.place {
            Place::All => "Where: everywhere".to_owned(),
            Place::Entity(r) => format!("Where: entity {}", short(&r.path, 28)),
            Place::Path(p) => format!("Where: under {}", short(p, 28)),
        };
        s.set_str("sw_place", &place);
        s.set_str("sw_game", if self.game { "Game: shown" } else { "Game: hidden" });
        s.set_str("sw_board", &format!("Show: {}", self.board.name()));
        s.set_str("sw_page", &self.page_label());
        s.set_bool("sw_has_prev", self.has_prev());
        s.set_bool("sw_has_next", self.has_next);
        s.set_str("sw_note", &self.note);
        s.set_str("sw_detail", &self.detail);
        s.set_bool("sw_can_open", matches!(self.selected, Some(RowKey::Submission(_))));
        let rows = s.list_mut("sw_rows");
        rows.clear();
        for r in self.rows.iter().take(ROWS) {
            let item = rows.push();
            item.set_str("label", &r.label);
            item.set_str("badge", &r.badge);
            item.set_bool("selected", self.selected.as_ref() == Some(&r.key));
        }
    }

    /// The view or its filters changed: back to the first page, nothing selected.
    pub fn restart(&mut self) {
        self.pages.clear();
        self.resume_at = None;
        self.page = 0;
        self.selected = None;
        self.detail.clear();
        self.highlight.clear();
        self.built = None;
    }

    /// The world was replaced: ids (highlights, the entity filter's id) are the old world's;
    /// the journal starts over too.
    pub fn world_replaced(&mut self) {
        self.restart();
        self.rows.clear();
        if matches!(self.place, Place::Entity(_)) {
            self.place = Place::All;
        }
    }

    /// For `editor_state` (bounded: one page).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("shown", Json::from(self.shown)),
            ("view", Json::from(self.view.name())),
            ("page", Json::from(self.page_label())),
            ("has_next", Json::from(self.has_next)),
            ("agent", self.agent.as_deref().map_or(Json::Null, Json::from)),
            (
                "place",
                match &self.place {
                    Place::All => Json::Null,
                    Place::Entity(r) => Json::obj([("entity_path", Json::from(r.path.as_str()))]),
                    Place::Path(p) => Json::obj([("path", Json::from(p.as_str()))]),
                },
            ),
            ("game", Json::from(self.game)),
            ("board", Json::from(self.board.name())),
            ("claim_map", Json::from(self.claim_map)),
            ("rows", Json::arr(self.rows.iter().map(|r| Json::obj([("label", Json::from(r.label.as_str())), ("badge", Json::from(r.badge.as_str()))])))),
            ("selected", self.rows.iter().position(|r| Some(&r.key) == self.selected.as_ref()).map_or(Json::Null, Json::from)),
            ("detail", Json::from(self.detail.as_str())),
            ("highlight", Json::arr(self.highlight.iter().map(|i| Json::from(*i)))),
            ("note", Json::from(self.note.as_str())),
        ])
    }
}

/// `s` cut to `n` characters (an ellipsis when cut), on one line.
pub fn short(s: &str, n: usize) -> String {
    let one = s.chars().map(|c| if c.is_control() { ' ' } else { c });
    if s.chars().count() > n {
        one.take(n.saturating_sub(1)).chain(['\u{2026}']).collect()
    } else {
        one.collect()
    }
}

/// `s` cut to `n` characters (an ellipsis when cut), its lines kept.
pub fn short_keep_lines(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        s.chars().take(n.saturating_sub(1)).chain(['\u{2026}']).collect()
    } else {
        s.to_owned()
    }
}

fn secs_left(t: Instant, now: Instant) -> u64 {
    t.saturating_duration_since(now).as_secs()
}

/// A journal entry as a row: `#seq by: kind path (+n)`, badges for via, undo and its task.
fn entry_row(e: &Entry, task: Option<u64>) -> Row {
    let first = e.changes.first().map(|c| c.path()).unwrap_or("");
    let more = if e.changes.len() > 1 { format!(" (+{})", e.changes.len() - 1) } else { String::new() };
    let label = short(&format!("#{} {}: {} {first}{more}", e.seq, e.by, e.kind.name()), ROW_CHARS);
    let mut badges: Vec<String> = Vec::new();
    if let Some(v) = &e.via {
        badges.push(format!("via {v}"));
    }
    if let Some(t) = task {
        badges.push(format!("task {t}"));
    }
    if e.undone_by.is_some() {
        badges.push("undone".into());
    }
    Row { key: RowKey::Entry(e.seq), label, badge: badges.join(", ") }
}

impl Engine {
    /// The swarm panel's press (the editor's input, as the human or an agent driving it).
    pub(crate) fn editor_swarm_act(&mut self, a: SwAct) {
        let p = &mut self.editor.ui.swarm;
        match a {
            SwAct::Shown(on) => p.shown = on,
            SwAct::View(v) => {
                if p.view != v {
                    p.view = v;
                    p.restart();
                }
            }
            SwAct::Select(i) => {
                let key = p.rows.get(i).map(|r| r.key.clone());
                if let Some(k) = key {
                    p.selected = if p.selected.as_ref() == Some(&k) { None } else { Some(k) };
                    self.editor_swarm_detail();
                }
            }
            SwAct::Newest => p.restart(),
            SwAct::Older => {
                if p.has_next {
                    match p.view {
                        View::Journal => {
                            if let Some(at) = p.resume_at {
                                p.pages.push(at);
                            }
                        }
                        _ => p.page += 1,
                    }
                    p.built = None;
                }
            }
            SwAct::Newer => {
                match p.view {
                    View::Journal => {
                        p.pages.pop();
                    }
                    _ => p.page = p.page.saturating_sub(1),
                }
                p.built = None;
            }
            SwAct::CycleAgent => {
                let next = match &p.agent {
                    None => p.agents_seen.first().cloned(),
                    Some(a) => p.agents_seen.iter().skip_while(|x| *x != a).nth(1).cloned(),
                };
                p.agent = next;
                p.restart();
            }
            SwAct::CyclePlace => {
                let sel = self.editor.ui.selection.last().copied();
                let p = &mut self.editor.ui.swarm;
                p.place = match (&p.place, sel) {
                    (Place::All, Some(id)) => EntityRef::of(&self.world, id).map_or(Place::All, Place::Entity),
                    (Place::Entity(r), _) => Place::Path(r.path.clone()),
                    _ => Place::All,
                };
                p.restart();
            }
            SwAct::ToggleGame => {
                p.game = !p.game;
                p.restart();
            }
            SwAct::CycleBoard => {
                p.board = match p.board {
                    BoardShow::Tasks => BoardShow::Intents,
                    BoardShow::Intents => BoardShow::Notes,
                    BoardShow::Notes => BoardShow::Tasks,
                };
                p.restart();
            }
            SwAct::ClaimMap => {
                p.claim_map = !p.claim_map;
                let (on, live) = (p.claim_map, self.swarm.claims.live(Instant::now()).count());
                if on && live > MAX_CLAIM_MAP {
                    self.editor.ui.set_status(format!("The claim map draws the first {MAX_CLAIM_MAP} of {live} claims; the Claims view lists them all."), false);
                }
            }
            SwAct::OpenReview => {
                if let Some(RowKey::Submission(id)) = p.selected.clone() {
                    if self.swarm.review.get(id).is_some() {
                        // The review screen takes the window (the editor gets no input until it
                        // closes). The input monitor sees it first (docs/design/SECURITY.md): the
                        // human's own press at the window, or the editor acting on the frames an
                        // owner's agent injected (that call was seen as editor input already).
                        let actor = if self.editor.input_is_injected() { crate::inputguard::Actor::Engine("editor_input") } else { crate::inputguard::Actor::Human };
                        // (The record names the owner's agent whose frames pressed it.)
                        let what = match self.editor.ui.driven_by.as_deref().filter(|_| self.editor.input_is_injected()) {
                            Some(by) => format!("submission {id}, pressed by {by}'s injected frames"),
                            None => format!("submission {id}"),
                        };
                        let seen = crate::inputguard::done(crate::inputguard::Channel::Screen, "editor.open_review", &actor, &what);
                        self.review_ui.open(Some(id), &self.swarm.review, seen);
                        if self.editor.input_is_injected() {
                            // An owner's agent pressed it with injected frames: an agent's
                            // open, so it starts on Close like any other.
                            self.review_ui.defuse();
                        }
                        self.editor.ui.set_status(format!("Submission {id} is open in the review screen: approve or reject it there (F9 closes it)."), true);
                    }
                }
            }
        }
        self.editor.ui.changed();
    }

    /// The panels' rows, rebuilt when their data changed (at most every [`REFRESH_EVERY`]).
    pub(crate) fn editor_swarm_refresh(&mut self) {
        let p = &self.editor.ui.swarm;
        // Only for the tab being looked at: the claim map reads the claims itself, each frame.
        // (Review R1 of m5-edswarm, minor 9: with only the claim map on, the hidden tab's view
        // was still rebuilt, a journal scan of up to MAX_SCAN entries.)
        if !p.shown {
            return;
        }
        let now = Instant::now();
        // What the page depends on: the journal, the board, the queue, claims and leases (and
        // time left on them: once a second).
        let key = vec![
            p.view as u64,
            p.pages.last().copied().unwrap_or(u64::MAX),
            p.page as u64,
            // The journal moves with every tick of a running game: a reason to rebuild only
            // the views that show it.
            if matches!(p.view, View::Journal | View::Cost) { self.swarm.journal().last_seq() } else { 0 },
            self.swarm.board.seq(),
            self.swarm.board.revision(),
            self.swarm.review.seq(),
            self.swarm.claims.len() as u64,
            self.swarm.leases.len() as u64,
            self.swarm.intents.len() as u64,
            u64::from(p.game),
            p.board as u64,
        ];
        if let Some((k, at)) = &p.built {
            let second = at.elapsed() >= Duration::from_secs(1);
            if (*k == key && !second) || at.elapsed() < REFRESH_EVERY {
                return;
            }
        }
        let (rows, has_next, note, seen) = match p.view {
            View::Journal => self.editor_swarm_journal(),
            View::Claims => self.editor_swarm_claims(now),
            View::Board => self.editor_swarm_board(now),
            View::Review => self.editor_swarm_review(),
            View::Cost => self.editor_swarm_cost(),
        };
        let p = &mut self.editor.ui.swarm;
        p.rows = rows;
        p.has_next = has_next;
        p.note = note;
        if let Some((seen, resume)) = seen {
            p.agents_seen = seen;
            p.resume_at = resume;
        }
        p.built = Some((key, now));
        // A selected row that left the page (decided, released, done) is let go of.
        if p.selected.as_ref().is_some_and(|k| !p.rows.iter().any(|r| r.key == *k)) {
            if let Some(RowKey::Submission(id)) = p.selected.take() {
                // The status line said it was open for a decision: now it says how it went.
                if let Some(s) = self.swarm.review.get(id).filter(|s| s.status != crate::review::Status::Pending) {
                    let by = s.decision.as_ref().map_or_else(String::new, |d| format!(" by {}", d.by));
                    let line = format!("Submission {id} was {}{by}.", s.status.name());
                    self.editor.ui.set_status(line, true);
                }
            }
        }
        self.editor_swarm_detail();
        self.editor.ui.changed();
    }

    /// One journal page: newest first from the page's start, filtered, examining at most
    /// [`MAX_SCAN`] entries.
    fn editor_swarm_journal(&self) -> PageOf {
        let p = &self.editor.ui.swarm;
        let from = p.pages.last().copied().unwrap_or(u64::MAX);
        let place_id = match &p.place {
            Place::Entity(r) => Some(r.attached(&self.world)),
            _ => None,
        };
        let journal = self.swarm.journal();
        let mut rows = Vec::with_capacity(ROWS);
        let mut seen: Vec<String> = Vec::new();
        let mut examined = 0usize;
        let mut changes = 0usize;
        let mut more = false;
        let mut resume = None;
        for e in journal.entries_upto_rev(from, p.game) {
            examined += 1;
            changes += e.changes.len();
            if examined > MAX_SCAN || changes > MAX_SCAN_CHANGES {
                examined = MAX_SCAN + 1;
                more = true;
                resume = Some(e.seq);
                break;
            }
            if seen.len() < 32 && !seen.iter().any(|s| **s == *e.by) {
                seen.push(e.by.to_string());
            }
            if p.agent.as_deref().is_some_and(|a| *e.by != *a && e.via.as_deref() != Some(a)) {
                continue;
            }
            let place_ok = match (&p.place, place_id) {
                (Place::Entity(_), Some(Some(id))) => e.touches(id),
                // A deleted entity keeps its history, and the entry that deleted it: the id it
                // had in this world (the filter is dropped when the world is replaced, so the id
                // never names another world's entity). (Review R1, minor 2: the timeline said
                // "No entries match" the moment the filtered entity was deleted.)
                (Place::Entity(r), Some(None)) => e.touches(r.uid.id),
                (Place::Entity(_), None) => false,
                (Place::Path(prefix), _) => e.touches_path(prefix),
                (Place::All, _) => true,
            };
            if !place_ok {
                continue;
            }
            if rows.len() == ROWS {
                more = true;
                resume = Some(e.seq);
                break;
            }
            rows.push(entry_row(e, self.swarm.board.task_of_entry(e.seq)));
        }
        seen.sort();
        let note = if examined > MAX_SCAN {
            "Looked at as many entries as one refresh may; Older goes on from the last row.".to_owned()
        } else if rows.is_empty() {
            "No entries match.".into()
        } else {
            String::new()
        };
        (rows, more, note, Some((seen, resume)))
    }

    fn editor_swarm_claims(&self, now: Instant) -> PageOf {
        let p = &self.editor.ui.swarm;
        let skip = p.page * ROWS;
        let mut all: Vec<Row> = self
            .swarm
            .claims
            .live(now)
            .map(|c| {
                let shared = if c.shared.is_empty() { String::new() } else { format!(", shared with {}", c.shared.len()) };
                Row { key: RowKey::Claim(c.id), label: short(&format!("{}: {}", c.owner, c.scope.describe()), ROW_CHARS), badge: format!("{} s{shared}", secs_left(c.expires, now)) }
            })
            .skip(skip)
            .take(ROWS + 1)
            .collect();
        let mut capped = false;
        if all.len() <= ROWS {
            // Leases follow the claims (ports, GPU time, scratch folders: no place in the world).
            let claims = self.swarm.claims.live(now).count();
            let lease_skip = skip.saturating_sub(claims);
            let leases = self.swarm.leases.list(None, None, None, LIST_LIMIT, now);
            capped = leases.get("leases").and_then(Json::as_array).is_some_and(|a| a.len() >= LIST_LIMIT);
            for l in leases.get("leases").and_then(Json::as_array).unwrap_or_default().iter().skip(lease_skip).take(ROWS + 1 - all.len()) {
                let id = l.get("id").and_then(Json::as_u64).unwrap_or(0);
                let what = match l.get("kind").and_then(Json::as_str).unwrap_or("?") {
                    "port" => format!("ports {}", l.get("ports").map(Json::to_string).unwrap_or_default()),
                    "gpu" => format!("GPU {} ms/s", l.get("gpu_ms_per_s").and_then(Json::as_u64).unwrap_or(0)),
                    other => other.to_owned(),
                };
                let owner = l.get("owner").and_then(Json::as_str).unwrap_or("?");
                let left = l.get("expires_in_s").and_then(Json::as_f64).unwrap_or(0.0) as u64;
                all.push(Row { key: RowKey::Lease(id), label: short(&format!("{owner}: {what}"), ROW_CHARS), badge: format!("lease, {left} s") });
            }
        }
        let more = all.len() > ROWS;
        all.truncate(ROWS);
        let note = if all.is_empty() && p.page == 0 {
            "No claims or leases.".to_owned()
        } else if capped {
            format!("Leases: the first {LIST_LIMIT} are listed here (lease_list has them all).")
        } else {
            String::new()
        };
        (all, more, note, None)
    }

    fn editor_swarm_board(&self, now: Instant) -> PageOf {
        let p = &self.editor.ui.swarm;
        let skip = p.page * ROWS;
        let mut rows: Vec<Row> = match p.board {
            BoardShow::Tasks => {
                // Newest first: what was just asked comes to the top.
                self.swarm
                    .board
                    .tasks()
                    .rev()
                    .skip(skip)
                    .take(ROWS + 1)
                    .map(|t| {
                        let who = t.owner.as_ref().map_or_else(|| format!("posted by {}", t.posted_by), |o| format!("taken by {}", o.name));
                        let ask = if t.tags.iter().any(|x| x == crate::ask::TAG) { ", asked" } else { "" };
                        Row { key: RowKey::Task(t.id), label: short(&format!("#{} {}", t.id, t.title), ROW_CHARS), badge: format!("{}, {who}{ask}", t.status.name()) }
                    })
                    .collect()
            }
            BoardShow::Intents => {
                let j = self.swarm.intents.list(None, None, None, LIST_LIMIT, now);
                j.get("intents")
                    .and_then(Json::as_array)
                    .unwrap_or_default()
                    .iter()
                    .skip(skip)
                    .take(ROWS + 1)
                    .map(|i| {
                        let id = i.get("id").and_then(Json::as_u64).unwrap_or(0);
                        let owner = i.get("owner").and_then(Json::as_str).unwrap_or("?");
                        let text = i.get("text").and_then(Json::as_str).unwrap_or("");
                        Row { key: RowKey::Intent(id), label: short(&format!("{owner}: {text}"), ROW_CHARS), badge: "intent".into() }
                    })
                    .collect()
            }
            BoardShow::Notes => self
                .swarm
                .board
                .notes()
                .rev()
                .skip(skip)
                .take(ROWS + 1)
                .map(|n| {
                    // A note on an entity shows its last known path (refreshed when listed or deleted).
                    let on = match &n.target {
                        crate::board::NoteTarget::Entity(r) => r.path.clone(),
                        crate::board::NoteTarget::Path(p) => p.clone(),
                        crate::board::NoteTarget::Task(t) => format!("task {t}"),
                    };
                    Row { key: RowKey::Note(n.id), label: short(&format!("{}: {}", n.by, n.text), ROW_CHARS), badge: short(&on, 32) }
                })
                .collect(),
        };
        let more = rows.len() > ROWS;
        rows.truncate(ROWS);
        let note = if rows.is_empty() && skip == 0 {
            format!("No {}.", p.board.name())
        } else if matches!(p.board, BoardShow::Intents) && self.swarm.intents.len() > LIST_LIMIT {
            format!("Intents: the first {LIST_LIMIT} of {} are listed here (intent_list has them all).", self.swarm.intents.len())
        } else {
            String::new()
        };
        (rows, more, note, None)
    }

    fn editor_swarm_review(&self) -> PageOf {
        let p = &self.editor.ui.swarm;
        let mut rows: Vec<Row> = self
            .swarm
            .review
            .pending()
            .skip(p.page * ROWS)
            .take(ROWS + 1)
            .map(|s| {
                let task = s.task.map_or(String::new(), |t| format!(", task {t}"));
                Row { key: RowKey::Submission(s.id), label: short(&format!("#{} {}", s.id, s.title), ROW_CHARS), badge: format!("by {}{task}", s.by.name) }
            })
            .collect();
        let more = rows.len() > ROWS;
        rows.truncate(ROWS);
        let note = if rows.is_empty() && p.page == 0 { "Nothing waits for review.".to_owned() } else { "Select one, then Open in review to decide it.".to_owned() };
        (rows, more, note, None)
    }

    fn editor_swarm_cost(&self) -> PageOf {
        let p = &self.editor.ui.swarm;
        // Agents, the busiest first, then tasks in progress.
        let mut agents: Vec<(&str, &crate::swarm::Usage)> = self.swarm.usage().iter().map(|(n, u)| (n.as_str(), u)).collect();
        agents.sort_by(|a, b| b.1.calls.cmp(&a.1.calls).then(a.0.cmp(b.0)));
        agents.truncate(MAX_AGENT_ROWS);
        let agent_rows = agents.into_iter().map(|(name, u)| {
            let refused: u64 = self.swarm.refused_of(name).map_or(0, |m| m.values().sum());
            let tokens = match u.tokens {
                Some(t) => format!(", {t} tokens (reported)"),
                None => String::new(),
            };
            Row {
                key: RowKey::Agent(name.to_owned()),
                label: short(&format!("{name}: {} calls, {} commits, {refused} refused{tokens}", u.calls, u.commits), ROW_CHARS),
                badge: if u.tokens.is_none() { "tokens not reported".into() } else { String::new() },
            }
        });
        let task_rows = self.swarm.board.tasks().rev().filter(|t| t.status.active()).map(|t| {
            let who = t.owner.as_ref().map_or("nobody", |o| o.name.as_str());
            let progress = if t.progress.is_empty() { String::new() } else { format!(": {}", t.progress) };
            Row {
                key: RowKey::Task(t.id),
                label: short(&format!("#{} {} ({who}){progress}", t.id, t.title), ROW_CHARS),
                badge: format!("{}, {} entries, {} refused", t.status.name(), t.work.entries, t.work.refused),
            }
        });
        let mut rows: Vec<Row> = agent_rows.chain(task_rows).skip(p.page * ROWS).take(ROWS + 1).collect();
        let more = rows.len() > ROWS;
        rows.truncate(ROWS);
        let note = "Calls the engine handled and commits charged, per agent; tokens show only if the harness reports them (none does yet).".to_owned();
        (rows, more, note, None)
    }

    /// The selected row's detail and what it highlights in the viewport.
    pub(crate) fn editor_swarm_detail(&mut self) {
        let Some(key) = self.editor.ui.swarm.selected.clone() else {
            let p = &mut self.editor.ui.swarm;
            p.detail.clear();
            p.highlight.clear();
            return;
        };
        let now = Instant::now();
        let (detail, highlight, label): (String, Vec<u64>, String) = match key {
            RowKey::Entry(seq) => match self.swarm.journal().get(seq) {
                Some(e) => {
                    let mut lines = vec![format!(
                        "Journal {} by {}{} ({}), tick {}, call {}",
                        e.seq,
                        e.by,
                        e.via.as_deref().map_or(String::new(), |v| format!(" via {v}")),
                        e.kind.name(),
                        e.tick,
                        e.call
                    )];
                    if let Some(t) = self.swarm.board.task_of_entry(seq) {
                        let title = self.swarm.board.get(t).map_or(String::new(), |t| format!(" \"{}\"", short(&t.title, 60)));
                        lines.push(format!("For task {t}{title}"));
                    }
                    for c in e.changes.iter().take(6) {
                        lines.push(format!("{} {}", c.op(), c.path()));
                    }
                    if e.changes.len() > 6 {
                        lines.push(format!("... and {} more changes", e.changes.len() - 6));
                    }
                    if let Some(u) = e.undone_by {
                        lines.push(format!("Undone by journal {u}"));
                    }
                    let ids: Vec<u64> = e.ids().iter().copied().filter(|i| *i != 0 && self.world.get(*i).is_some()).take(MAX_HIGHLIGHT).collect();
                    (lines.join("\n"), ids, format!("{} #{}", e.by, e.seq))
                }
                None => ("That entry is no longer in the journal (its ring keeps the newest).".into(), Vec::new(), String::new()),
            },
            RowKey::Claim(id) => match self.swarm.claims.get(id) {
                Some(c) => {
                    let ids = match &c.scope {
                        crate::claims::Scope::Path(p) => self.world.find_path(p).ok().into_iter().collect(),
                        crate::claims::Scope::Region { .. } => Vec::new(),
                    };
                    let shared: Vec<&str> = c.shared.keys().map(String::as_str).take(8).collect();
                    let text = format!(
                        "Claim {} by {} on {} ({} s left){}{}",
                        c.id,
                        c.owner,
                        c.scope.describe(),
                        secs_left(c.expires, now),
                        if shared.is_empty() { String::new() } else { format!("; shared with {}", shared.join(", ")) },
                        if c.note.is_empty() { String::new() } else { format!(": {}", c.note) }
                    );
                    (text, ids, c.owner.clone())
                }
                None => ("That claim was released or expired.".into(), Vec::new(), String::new()),
            },
            RowKey::Task(id) => match self.swarm.board.get(id) {
                Some(t) => {
                    let mut lines = vec![format!("Task {} ({}), posted by {}", t.id, t.status.name(), t.posted_by)];
                    if let Some(o) = &t.owner {
                        lines.push(format!("Taken by {}", o.name));
                    }
                    if !t.detail.is_empty() {
                        lines.push(short(&t.detail, 300));
                    }
                    for part in t.parts.iter().take(6) {
                        let on = part.entity.and_then(|i| t.entities.get(i)).map_or(String::new(), |r| format!(" of {}", r.path));
                        lines.push(format!("About: {}{on}", part.describe()));
                    }
                    if !t.progress.is_empty() {
                        lines.push(format!("Progress: {}", t.progress));
                    }
                    lines.push(format!("Work: {} journal entries, {} changes, {} refused", t.work.entries, t.work.changes, t.work.refused));
                    if !t.result.is_empty() {
                        lines.push(format!("Result: {}", short(&t.result, 200)));
                    }
                    let ids: Vec<u64> = t.entities.iter().filter_map(|r| r.attached(&self.world)).take(MAX_HIGHLIGHT).collect();
                    (lines.join("\n"), ids, format!("task {}", t.id))
                }
                None => ("That task is no longer on the board.".into(), Vec::new(), String::new()),
            },
            RowKey::Submission(id) => match self.swarm.review.get(id) {
                Some(s) => {
                    let mut lines = vec![format!("Submission {} \"{}\" by {} ({})", s.id, short(&s.title, 80), s.by.name, s.status.name())];
                    if let Some(t) = s.task {
                        lines.push(format!("For task {t}"));
                    }
                    if !s.summary.is_empty() {
                        lines.push(short(&s.summary, 300));
                    }
                    lines.push(s.checks_line());
                    for r in s.roots.iter().take(4) {
                        lines.push(match r.attached(&self.world) {
                            Some(_) => format!("Covers {} ({} entities)", r.path, r.entities),
                            None => format!("Covers {} (gone: can't be approved)", r.path),
                        });
                    }
                    if !s.journal.is_empty() {
                        lines.push(format!("{} journal entries", s.journal.len()));
                    }
                    let ids: Vec<u64> = s.roots.iter().filter_map(|r| r.attached(&self.world)).take(MAX_HIGHLIGHT).collect();
                    (lines.join("\n"), ids, format!("submission {}", s.id))
                }
                None => ("That submission is gone from the queue.".into(), Vec::new(), String::new()),
            },
            RowKey::Agent(name) => {
                let u = self.swarm.usage().get(&name).cloned().unwrap_or_default();
                let refused: Vec<String> = self.swarm.refused_of(&name).map(|m| m.iter().map(|(k, v)| format!("{k} {v}")).collect()).unwrap_or_default();
                let claims = self.swarm.claims.live(now).filter(|c| c.owner == name).count();
                let tasks: Vec<String> = self.swarm.board.tasks().filter(|t| t.owner.as_ref().is_some_and(|o| o.name == name) && t.status.active()).take(4).map(|t| format!("#{}", t.id)).collect();
                let text = format!(
                    "{name}: {} calls, {} commits charged; refused writes: {}; {} live claims; tasks in hand: {}; tokens: {}",
                    u.calls,
                    u.commits,
                    if refused.is_empty() { "none".into() } else { refused.join(", ") },
                    claims,
                    if tasks.is_empty() { "none".into() } else { tasks.join(", ") },
                    u.tokens.map_or("not reported (no harness reports them yet)".into(), |t| format!("{t} (reported by the agent, not measured)"))
                );
                (text, Vec::new(), String::new())
            }
            RowKey::Lease(id) => (format!("Lease {id}: ports, GPU time or a scratch folder (nothing in the world to draw)."), Vec::new(), String::new()),
            RowKey::Intent(id) => (format!("Intent {id}: see intent_list for its scope."), Vec::new(), String::new()),
            RowKey::Note(id) => (format!("Note {id}: see note_list."), Vec::new(), String::new()),
        };
        let p = &mut self.editor.ui.swarm;
        p.detail = short(&detail.replace('\n', " | "), DETAIL_CHARS);
        p.highlight = highlight;
        p.highlight_label = label;
    }

    /// The viewport's swarm marks: the selected row's entities (amber, with its label) and, with
    /// the claim map on, every live claim's scope in its holder's colour with the holder's name.
    pub(crate) fn editor_swarm_overlay(&mut self, painter: &mut crate::sight_overlay::Painter) {
        const AMBER: [f32; 4] = [1.0, 0.72, 0.2, 1.0];
        let now = Instant::now();
        let p = &self.editor.ui.swarm;
        for (i, id) in p.highlight.iter().enumerate() {
            let Some(e) = self.world.get(*id) else { continue };
            let (lo, hi) = e.world_aabb();
            painter.box_edges(&corners(lo, hi), AMBER, 2.0);
            if i == 0 && !p.highlight_label.is_empty() {
                if let Some(a) = painter.project(specula_core::math::Vec3::new((lo.x + hi.x) * 0.5, hi.y, (lo.z + hi.z) * 0.5)) {
                    painter.label(&p.highlight_label, a, AMBER, 1000);
                }
            }
        }
        if !p.claim_map {
            return;
        }
        // (Names are placed like sight's labels, at most MAX_CLAIM_NAMES: none is made past that.)
        let mut named = 0;
        for c in self.swarm.claims.live(now).take(MAX_CLAIM_MAP) {
            let color = crate::sight_overlay::owner_color(&c.owner);
            let (lo, hi) = match &c.scope {
                crate::claims::Scope::Region { min, max } => (*min, *max),
                crate::claims::Scope::Path(path) => match self.world.find_path(path) {
                    // The claimed entity's own box (its subtree isn't walked here).
                    Ok(id) => match self.world.get(id) {
                        Some(e) => e.world_aabb(),
                        None => continue,
                    },
                    Err(_) => continue,
                },
            };
            if !painter.sees(lo, hi) {
                continue;
            }
            let c8 = corners(lo, hi);
            painter.box_edges(&c8, color, 1.5);
            if named < MAX_CLAIM_NAMES {
                if let Some(a) = painter.project(specula_core::math::Vec3::new((lo.x + hi.x) * 0.5, hi.y, (lo.z + hi.z) * 0.5)) {
                    painter.label(&format!("{} (claim {})", c.owner, c.id), a, color, 500);
                    named += 1;
                }
            }
        }
    }
}

fn corners(lo: specula_core::math::Vec3, hi: specula_core::math::Vec3) -> [specula_core::math::Vec3; 8] {
    std::array::from_fn(|k| specula_core::math::Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_parse_and_rows_are_cut() {
        for v in [View::Journal, View::Claims, View::Board, View::Review, View::Cost] {
            assert_eq!(View::parse(v.name()), Some(v));
        }
        assert_eq!(View::parse("nope"), None);
        assert_eq!(short("abcdef", 4), "abc\u{2026}");
        assert_eq!(short("ab\ncd", 8), "ab cd");
    }

    #[test]
    fn a_panel_fills_its_state_and_restarts_on_replacement() {
        let mut p = Panel { shown: true, ..Panel::default() };
        p.rows = (0..20).map(|i| Row { key: RowKey::Entry(i), label: format!("#{i}"), badge: String::new() }).collect();
        p.selected = Some(RowKey::Entry(3));
        p.highlight = vec![1, 2];
        p.place = Place::Entity(EntityRef { uid: crate::world::EntityUid { lineage: 1, id: 2 }, path: "/a".into() });
        let mut s = StateMap::new();
        p.fill(&mut s);
        p.world_replaced();
        assert!(p.highlight.is_empty() && p.selected.is_none() && p.rows.is_empty());
        assert_eq!(p.place, Place::All, "an entity filter holds the old world's entity");
        let j = p.to_json();
        assert_eq!(j.get("view").and_then(Json::as_str), Some("journal"));
    }
}
