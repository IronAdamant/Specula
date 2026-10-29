//! The review queue and the structured critique queue: how finished work reaches the owner with
//! its evidence, and how findings are worked through (docs/design/REVIEW.md).
//!
//! **Submissions** (`review_submit`) carry what the work covers (entity subtrees, recorded with a
//! stamp over every entity below them; journal entries; the task it is for), its evidence
//! (captures the engine wrote, checks as data) and, optionally, where approval moves it
//! (`promote`). A decision (`review_decide`) is final: approved, rejected (a rework task goes to
//! the submitter) or withdrawn. The world side of decisions (promotion, reverts, tasks) is in
//! `review_engine`; this module is the data, its rules and its file.
//!
//! **Critiques** (`critique_add`) are findings with their evidence (capture, packet, metric) and
//! target (an entity, a path, a submission). They are assigned, resolved, dismissed with a reason,
//! or turned into board tasks, whose end resolves (done) or reopens (cancelled) them.
//!
//! Like the board, nothing here is simulation state: ticks and sequence numbers order it, no
//! wall-clock time is stored, and it is not in the state hash. Saved as `review.json` beside
//! `board.json`. Bounded: every text and list has a cap, the store has [`MAX_BYTES`], listings
//! are paged by bytes.

use crate::authority::Authority;
use crate::board::Owner;
use crate::journal::{path_under, Writer};
use crate::world::{World, WorldError};
use specula_core::Json;
use std::collections::BTreeMap;

pub const MAX_SUBMISSIONS: usize = 4096;
pub const MAX_CRITIQUES: usize = 8192;
pub const MAX_CAPTURES: usize = 8;
pub const MAX_CHECKS: usize = 64;
pub const MAX_PATHS: usize = 64;
pub const MAX_JOURNAL: usize = 256;
/// What `review_list` says of journal refs from another epoch.
pub const STALE_JOURNAL: &str = "journal refs from an earlier run (or from before a world_load) can't be reverted: the journal numbers entries afresh; revert by hand";
/// Entity ids a submission's journal entries touched, recorded at submit (a revert may touch
/// only these); past this many none are kept and the submission can't be reverted.
pub const MAX_JOURNAL_IDS: usize = 4096;
/// Entities under one submission's paths (all roots together).
pub const MAX_SUBTREE: usize = 10_000;
pub const MAX_TITLE: usize = 200;
pub const MAX_SUMMARY: usize = 2000;
pub const MAX_NOTE: usize = 2000;
/// A file path (capture, packet).
pub const MAX_FILE_PATH: usize = 512;
/// Critiques per `critique_add`.
pub const MAX_ADD: usize = 32;
/// The queue's and critiques' text together, roughly as saved.
pub const MAX_BYTES: usize = 16 << 20;
/// Listings stop adding items past this many bytes and hand back a cursor.
const MAX_LIST_BYTES: usize = 48 * 1024;
pub const FORMAT: &str = "specula.review/v1";
/// The window's human, in attribution.
pub const WINDOW_OWNER: &str = crate::authority::WINDOW_OWNER;

fn err(code: &'static str, message: impl Into<String>) -> WorldError {
    WorldError::new(code, message)
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

/// Labels a submission's kind (what the screen and listings say; approval does what `promote` says).
pub const KINDS: [&str; 5] = ["asset", "lex", "scene", "ui", "changes"];
pub const CAPTURE_KINDS: [&str; 6] = ["contact_sheet", "before_after", "frame", "filmstrip", "orbit", "other"];
pub const CHECK_KINDS: [&str; 7] = ["style", "taste", "motion", "physics", "perf", "ui", "other"];
pub const SEVERITIES: [&str; 3] = ["error", "warning", "note"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Approved,
    Rejected,
    Withdrawn,
}

impl Status {
    pub const ALL: [Status; 4] = [Status::Pending, Status::Approved, Status::Rejected, Status::Withdrawn];

    pub fn name(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Approved => "approved",
            Status::Rejected => "rejected",
            Status::Withdrawn => "withdrawn",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|x| x.name() == s)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub path: String,
    pub kind: String,
    pub packet: Option<String>,
    pub caption: String,
}

impl Capture {
    fn to_json(&self) -> Json {
        let mut f = vec![("path", Json::from(self.path.as_str())), ("kind", Json::from(self.kind.as_str()))];
        if let Some(p) = &self.packet {
            f.push(("packet", Json::from(p.as_str())));
        }
        if !self.caption.is_empty() {
            f.push(("caption", Json::from(self.caption.as_str())));
        }
        Json::obj(f)
    }
}

/// What an agent said, and who said it: never shown as an engine result.
#[derive(Clone, Debug, PartialEq)]
pub struct Claimed<T> {
    pub by: String,
    pub value: T,
}

/// A fact as data: measured by the engine, or claimed by an agent. Renderers and listings
/// count the two apart ("2 engine checks passed; 2 reported by bob").
#[derive(Clone, Debug, PartialEq)]
pub enum Fact<T> {
    Engine(T),
    Claimed(Claimed<T>),
}

impl<T: Copy> Fact<T> {
    pub fn value(&self) -> T {
        match self {
            Fact::Engine(v) => *v,
            Fact::Claimed(c) => c.value,
        }
    }

    pub fn by_engine(&self) -> bool {
        matches!(self, Fact::Engine(_))
    }

    /// Who claimed it (`None`: the engine measured it).
    pub fn claimed_by(&self) -> Option<&str> {
        match self {
            Fact::Engine(_) => None,
            Fact::Claimed(c) => Some(&c.by),
        }
    }
}

/// A check's result, as data from the tool that ran it (or the engine).
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub kind: String,
    pub name: String,
    /// Passed: measured by the engine (`paths_exist`, `captures_exist`), or reported by the
    /// submitter (everything in `review_submit {checks}`).
    pub passed: Fact<bool>,
    /// `error`, `warning` or `note` (how much a failure matters).
    pub severity: String,
    pub metric: Option<String>,
    pub value: Option<f64>,
    pub limit: Option<f64>,
    pub detail: String,
}

impl Check {
    pub fn to_json(&self) -> Json {
        let mut f = vec![
            ("kind", Json::from(self.kind.as_str())),
            ("name", Json::from(self.name.as_str())),
            ("passed", Json::Bool(self.passed.value())),
            ("severity", Json::from(self.severity.as_str())),
        ];
        if let Some(m) = &self.metric {
            f.push(("metric", Json::from(m.as_str())));
        }
        if let Some(v) = self.value {
            f.push(("value", Json::Num(v)));
        }
        if let Some(l) = self.limit {
            f.push(("limit", Json::Num(l)));
        }
        if !self.detail.is_empty() {
            f.push(("detail", Json::from(self.detail.as_str())));
        }
        match &self.passed {
            Fact::Engine(_) => f.push(("engine", Json::Bool(true))),
            Fact::Claimed(c) => f.push(("reported_by", Json::from(c.by.as_str()))),
        }
        Json::obj(f)
    }
}

/// An entity subtree a submission covers, as it was when submitted.
#[derive(Clone, Debug, PartialEq)]
pub struct Root {
    pub path: String,
    pub id: u64,
    pub version: u64,
    /// [`stamp`] over the subtree (each entity's id and version).
    pub stamp: u64,
    pub entities: usize,
}

/// Where approval moves the work.
#[derive(Clone, Debug, PartialEq)]
pub struct Promote {
    /// The parent path the roots move under.
    pub to: String,
    /// Delete a same-named entity already there (in the same batch).
    pub replace: bool,
    /// Taken off the moved roots (default `staging`; `None`: none).
    pub remove_tag: Option<String>,
}

impl Promote {
    fn to_json(&self) -> Json {
        Json::obj([
            ("to", Json::from(self.to.as_str())),
            ("replace", Json::Bool(self.replace)),
            ("remove_tag", self.remove_tag.as_deref().map_or(Json::Null, Json::from)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub verdict: Status,
    pub by: String,
    pub note: String,
    pub tick: u64,
    /// Promotion: `{journal, moved: [paths]}`.
    pub promoted: Option<Json>,
    /// The rework task a rejection posted.
    pub task: Option<u64>,
    /// The journal entry of a rejection's revert.
    pub reverted: Option<u64>,
}

impl Decision {
    fn to_json(&self) -> Json {
        let mut f = vec![
            ("verdict", Json::from(self.verdict.name())),
            ("by", Json::from(self.by.as_str())),
            ("note", Json::from(self.note.as_str())),
            ("tick", Json::from(self.tick)),
        ];
        if let Some(p) = &self.promoted {
            f.push(("promoted", p.clone()));
        }
        if let Some(t) = self.task {
            f.push(("task", Json::from(t)));
        }
        if let Some(r) = self.reverted {
            f.push(("reverted", Json::from(r)));
        }
        Json::obj(f)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Submission {
    pub id: u64,
    pub status: Status,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub batch: Option<String>,
    /// Who submitted it (name and, within this run, identity).
    pub by: Owner,
    pub task: Option<u64>,
    pub captures: Vec<Capture>,
    pub checks: Vec<Check>,
    pub roots: Vec<Root>,
    pub journal: Vec<u64>,
    /// The journal epoch `journal`'s seqs belong to ([`crate::journal::Journal::epoch`]): from
    /// another epoch (an earlier run, or before a `world_load`) they name other entries, or none.
    pub journal_epoch: String,
    /// The entity ids those entries touched when it was submitted; `None` when there were more
    /// than [`MAX_JOURNAL_IDS`].
    pub journal_ids: Option<Vec<u64>>,
    pub promote: Option<Promote>,
    pub supersedes: Option<u64>,
    pub superseded_by: Option<u64>,
    pub tick: u64,
    /// The store's change counter at its last change (`review_list {changed_since}`).
    pub updated_seq: u64,
    pub decision: Option<Decision>,
    /// Its text's size as last measured; not saved.
    pub size: usize,
}

impl Submission {
    pub fn measure(&self) -> usize {
        let caps: usize = self.captures.iter().map(|c| 64 + c.path.len() + c.kind.len() + c.packet.as_ref().map_or(0, String::len) + c.caption.len()).sum();
        let checks: usize = self.checks.iter().map(|c| 128 + c.kind.len() + c.name.len() + c.detail.len() + c.metric.as_ref().map_or(0, String::len)).sum();
        let roots: usize = self.roots.iter().map(|r| 96 + r.path.len()).sum();
        let decision = self.decision.as_ref().map_or(0, |d| 128 + d.by.len() + d.note.len() + d.promoted.as_ref().map_or(0, |p| p.to_string().len()));
        512 + self.title.len() + self.summary.len() + self.kind.len() + self.batch.as_ref().map_or(0, String::len) + self.by.name.len() + caps + checks + roots + 12 * self.journal.len() + 12 * self.journal_ids.as_ref().map_or(0, Vec::len) + self.journal_epoch.len() + decision
            + self.promote.as_ref().map_or(0, |p| 64 + p.to.len())
    }

    /// Failed checks, errors first (then warnings, then notes), in check order.
    pub fn failed(&self) -> Vec<&Check> {
        let mut v: Vec<&Check> = self.checks.iter().filter(|c| !c.passed.value()).collect();
        v.sort_by_key(|c| SEVERITIES.iter().position(|s| *s == c.severity).unwrap_or(3));
        v
    }

    /// `(passed, failed)` of the engine's checks and of the reported ones.
    pub fn check_counts(&self) -> ((usize, usize), (usize, usize)) {
        let count = |engine: bool| {
            let of: Vec<&Check> = self.checks.iter().filter(|c| c.passed.by_engine() == engine).collect();
            let passed = of.iter().filter(|c| c.passed.value()).count();
            (passed, of.len() - passed)
        };
        (count(true), count(false))
    }

    /// The checks in two parts for the review screen, so they can't look alike (STYLE.md: an
    /// agent's claim never looks like an engine pass): the engine's results (`""` when none),
    /// and what the submitter reported (`""` when nothing).
    pub fn checks_parts(&self) -> (String, String) {
        let ((ep, ef), (rp, rf)) = self.check_counts();
        let part = |p: usize, f: usize| match (p, f) {
            (p, 0) => format!("{p} passed"),
            (0, f) => format!("{f} failed"),
            (p, f) => format!("{p} passed, {f} failed"),
        };
        let engine = if ep + ef == 0 { String::new() } else { format!("Engine checks: {}", part(ep, ef)) };
        let reported = if rp + rf == 0 { String::new() } else { format!("Reported by {} (not checked by the engine): {}", self.by.name, part(rp, rf)) };
        (engine, reported)
    }

    /// The checks as one line, engine results and reported ones apart (listings, tests).
    pub fn checks_line(&self) -> String {
        let ((ep, ef), (rp, rf)) = self.check_counts();
        let part = |p: usize, f: usize, what: &str| match (p, f) {
            (0, 0) => None,
            (p, 0) => Some(format!("{p} {what} passed")),
            (0, f) => Some(format!("{f} {what} failed")),
            (p, f) => Some(format!("{p} {what} passed, {f} failed")),
        };
        let reported = format!("reported by {}", self.by.name);
        let parts: Vec<String> = [part(ep, ef, "engine check(s)"), part(rp, rf, &reported)].into_iter().flatten().collect();
        if parts.is_empty() {
            "Checks: none".into()
        } else {
            format!("Checks: {}", parts.join("; "))
        }
    }

    /// Whether `w` submitted it (name and identity; a loaded submission's by name alone).
    pub fn submitted_by(&self, w: Writer) -> bool {
        self.by.is(w)
    }

    /// Its journal entries, while `epoch` (the live journal's) is the one they were submitted in;
    /// none otherwise (the same seqs now name other entries, or nothing).
    pub fn journal_in(&self, epoch: &str) -> &[u64] {
        if self.journal_current(epoch) {
            &self.journal
        } else {
            &[]
        }
    }

    /// Whether its journal refs are from the live journal's `epoch`.
    pub fn journal_current(&self, epoch: &str) -> bool {
        !self.journal_epoch.is_empty() && self.journal_epoch == epoch
    }

    /// The compact listing item; `open_critiques` from the store, `epoch` the live journal's.
    pub fn summary_json(&self, open_critiques: usize, epoch: &str) -> Json {
        let mut f = vec![
            ("id", Json::from(self.id)),
            ("status", Json::from(self.status.name())),
            ("kind", Json::from(self.kind.as_str())),
            ("title", Json::from(self.title.as_str())),
            ("submitted_by", Json::from(self.by.name.as_str())),
        ];
        if let Some(b) = &self.batch {
            f.push(("batch", Json::from(b.as_str())));
        }
        if let Some(t) = self.task {
            f.push(("task", Json::from(t)));
        }
        let failed = self.failed();
        let ((ep, ef), (rp, rf)) = self.check_counts();
        f.push((
            "checks",
            Json::obj([
                ("total", Json::from(self.checks.len())),
                ("failed", Json::from(failed.len())),
                ("failed_names", Json::arr(failed.iter().take(4).map(|c| Json::from(format!("{}:{}", c.kind, c.name))))),
                // Measured by the engine vs reported by the submitter (never added together as passes).
                ("engine", Json::obj([("passed", Json::from(ep)), ("failed", Json::from(ef))])),
                ("reported", Json::obj([("passed", Json::from(rp)), ("failed", Json::from(rf)), ("by", Json::from(self.by.name.as_str()))])),
            ]),
        ));
        f.push(("captures", Json::from(self.captures.len())));
        if let Some(c) = self.captures.first() {
            f.push(("capture", Json::from(c.path.as_str())));
        }
        if !self.roots.is_empty() {
            f.push(("paths", Json::arr(self.roots.iter().take(4).map(|r| Json::from(r.path.as_str())))));
            f.push(("entities", Json::from(self.roots.iter().map(|r| r.entities).sum::<usize>())));
        }
        if !self.journal.is_empty() {
            f.push(("journal_entries", Json::from(self.journal.len())));
            if !self.journal_current(epoch) {
                f.push(("journal_stale", Json::from(STALE_JOURNAL)));
            }
        }
        if let Some(p) = &self.promote {
            f.push(("promote_to", Json::from(p.to.as_str())));
        }
        if open_critiques > 0 {
            f.push(("open_critiques", Json::from(open_critiques)));
        }
        if let Some(d) = &self.decision {
            f.push(("decided_by", Json::from(d.by.as_str())));
            if !d.note.is_empty() {
                f.push(("note", Json::from(short(&d.note, 160))));
            }
            if let Some(t) = d.task {
                f.push(("rework_task", Json::from(t)));
            }
        }
        if let Some(s) = self.superseded_by {
            f.push(("superseded_by", Json::from(s)));
        }
        f.push(("updated_seq", Json::from(self.updated_seq)));
        Json::obj(f)
    }

    /// Everything; `epoch` is the live journal's.
    pub fn to_json(&self, critiques: &[&Critique], epoch: &str) -> Json {
        let mut j = self.summary_json(critiques.iter().filter(|c| c.status.open()).count(), epoch);
        j.set("summary", Json::from(self.summary.as_str()));
        j.set("captures", Json::arr(self.captures.iter().map(Capture::to_json)));
        j.set("checks", Json::arr(self.checks.iter().map(Check::to_json)));
        j.set(
            "roots",
            Json::arr(self.roots.iter().map(|r| {
                Json::obj([
                    ("path", Json::from(r.path.as_str())),
                    ("id", Json::from(r.id)),
                    ("version", Json::from(r.version)),
                    ("entities", Json::from(r.entities)),
                    ("stamp", Json::from(format!("{:016x}", r.stamp))),
                ])
            })),
        );
        j.set("journal", Json::arr(self.journal.iter().map(|s| Json::from(*s))));
        if !self.journal.is_empty() {
            j.set("journal_epoch", Json::from(self.journal_epoch.as_str()));
            // A count for agents (up to 4,096 ids is tokens); the ids themselves are in the file.
            j.set("journal_entities", self.journal_ids.as_ref().map_or(Json::Null, |ids| Json::from(ids.len())));
        }
        if let Some(p) = &self.promote {
            j.set("promote", p.to_json());
        }
        if let Some(s) = self.supersedes {
            j.set("supersedes", Json::from(s));
        }
        j.set("tick", Json::from(self.tick));
        if let Some(d) = &self.decision {
            j.set("decision", d.to_json());
        }
        j.set("critiques", Json::arr(critiques.iter().take(16).map(|c| c.summary_json())));
        j
    }
}

/// The FNV-1a stamp of a subtree: each persistent entity's id and version in walk order (stable
/// across runs and toolchains, unlike std's hasher).
pub fn stamp(world: &World, root: u64, limit: usize) -> (u64, usize) {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut n = 0usize;
    world.walk(Some(root), |e, _| {
        if e.transient || n > limit {
            return;
        }
        n += 1;
        for b in e.id.to_le_bytes().into_iter().chain(e.version.to_le_bytes()) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    });
    (h, n)
}

// ---- critiques ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CStatus {
    Open,
    Assigned,
    /// A board task carries it (resolves when the task is done).
    InTask,
    Resolved,
    Dismissed,
}

impl CStatus {
    pub const ALL: [CStatus; 5] = [CStatus::Open, CStatus::Assigned, CStatus::InTask, CStatus::Resolved, CStatus::Dismissed];

    pub fn name(self) -> &'static str {
        match self {
            CStatus::Open => "open",
            CStatus::Assigned => "assigned",
            CStatus::InTask => "in_task",
            CStatus::Resolved => "resolved",
            CStatus::Dismissed => "dismissed",
        }
    }

    pub fn parse(s: &str) -> Option<CStatus> {
        CStatus::ALL.into_iter().find(|x| x.name() == s)
    }

    /// Still to be worked through.
    pub fn open(self) -> bool {
        matches!(self, CStatus::Open | CStatus::Assigned | CStatus::InTask)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Entity { id: u64, path: String },
    Path(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
    pub name: String,
    pub value: Option<f64>,
    pub limit: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Critique {
    pub id: u64,
    pub version: u64,
    pub status: CStatus,
    pub severity: String,
    /// Who found it, as its author says (`sight-skeptic`, `lane-reviewer`, a check's name):
    /// a claim, always shown with `by` beside it. Only the owner may say `owner` (or another
    /// authority role): see [`Review::add_critiques`].
    pub source: String,
    pub by: Owner,
    /// The author's authority when it was added: who else may close or reopen it
    /// ([`Review::check_critique`]).
    pub authority: Authority,
    pub text: String,
    pub capture: Option<String>,
    pub packet: Option<String>,
    pub metric: Option<Metric>,
    pub target: Option<Target>,
    pub submission: Option<u64>,
    pub assignee: Option<String>,
    pub task: Option<u64>,
    /// The resolution or dismissal note.
    pub resolution: String,
    pub resolved_by: Option<String>,
    pub tick: u64,
    pub updated_tick: u64,
    pub updated_seq: u64,
    pub size: usize,
}

impl Critique {
    pub fn measure(&self) -> usize {
        256 + self.severity.len()
            + self.source.len()
            + self.by.name.len()
            + self.text.len()
            + self.capture.as_ref().map_or(0, String::len)
            + self.packet.as_ref().map_or(0, String::len)
            + self.metric.as_ref().map_or(0, |m| 48 + m.name.len())
            + match &self.target {
                Some(Target::Entity { path, .. } | Target::Path(path)) => path.len() + 32,
                None => 0,
            }
            + self.assignee.as_ref().map_or(0, String::len)
            + self.resolution.len()
    }

    pub fn summary_json(&self) -> Json {
        let mut f = vec![
            ("id", Json::from(self.id)),
            ("status", Json::from(self.status.name())),
            ("severity", Json::from(self.severity.as_str())),
            ("source", Json::from(self.source.as_str())),
            ("by", Json::from(self.by.name.as_str())),
            ("text", Json::from(short(&self.text, 200))),
        ];
        if self.authority > Authority::Agent {
            f.push(("authority", Json::from(self.authority.name())));
        }
        if let Some(r) = &self.resolved_by {
            f.push(("resolved_by", Json::from(r.as_str())));
        }
        match &self.target {
            Some(Target::Entity { id, path }) => f.push(("entity", Json::obj([("id", Json::from(*id)), ("path", Json::from(path.as_str()))]))),
            Some(Target::Path(p)) => f.push(("path", Json::from(p.as_str()))),
            None => {}
        }
        if let Some(s) = self.submission {
            f.push(("submission", Json::from(s)));
        }
        if let Some(m) = &self.metric {
            let mut mf = vec![("name", Json::from(m.name.as_str()))];
            if let Some(v) = m.value {
                mf.push(("value", Json::Num(v)));
            }
            if let Some(l) = m.limit {
                mf.push(("limit", Json::Num(l)));
            }
            f.push(("metric", Json::obj(mf)));
        }
        if let Some(c) = &self.capture {
            f.push(("capture", Json::from(c.as_str())));
        }
        if let Some(a) = &self.assignee {
            f.push(("assignee", Json::from(a.as_str())));
        }
        if let Some(t) = self.task {
            f.push(("task", Json::from(t)));
        }
        f.push(("version", Json::from(self.version)));
        Json::obj(f)
    }

    pub fn to_json(&self) -> Json {
        let mut j = self.summary_json();
        j.set("text", Json::from(self.text.as_str()));
        if let Some(p) = &self.packet {
            j.set("packet", Json::from(p.as_str()));
        }
        if !self.resolution.is_empty() {
            j.set("resolution", Json::from(self.resolution.as_str()));
        }
        j.set("tick", Json::from(self.tick));
        j.set("updated_tick", Json::from(self.updated_tick));
        j.set("updated_seq", Json::from(self.updated_seq));
        j
    }
}

/// What `critique_add` gives for one item.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CritiqueSpec {
    pub text: String,
    pub severity: String,
    pub source: String,
    pub capture: Option<String>,
    pub packet: Option<String>,
    pub metric: Option<Metric>,
    pub target: Option<Target>,
    pub submission: Option<u64>,
    pub assignee: Option<String>,
}

/// A `critique_update` that passed [`Review::check_critique`] (who may, and the critique's
/// state); [`Review::apply_critique`] takes it. Only `check_critique` makes one.
#[derive(Debug)]
pub struct CritiqueCheck {
    id: u64,
    version: u64,
}

impl CritiqueCheck {
    pub fn id(&self) -> u64 {
        self.id
    }
}

/// `critique_update`'s action.
#[derive(Clone, Debug, PartialEq)]
pub enum CritiqueAction {
    Assign(String),
    Resolve(String),
    Dismiss(String),
    Reopen,
    /// Post a board task for it (assigned to this agent, if given).
    ToTask { assign: Option<String> },
}

/// `critique_list`'s filters.
#[derive(Clone, Debug, Default)]
pub struct CritiqueQuery {
    pub statuses: Vec<CStatus>,
    pub severity: Option<String>,
    pub source: Option<String>,
    pub submission: Option<u64>,
    pub path_prefix: Option<String>,
    pub assignee: Option<String>,
    pub changed_since: Option<u64>,
    pub after: Option<u64>,
    pub limit: usize,
    pub detailed: bool,
}

/// `review_list`'s filters.
#[derive(Clone, Debug, Default)]
pub struct SubmissionQuery {
    pub statuses: Vec<Status>,
    pub kind: Option<String>,
    pub batch: Option<String>,
    pub by: Option<String>,
    pub task: Option<u64>,
    pub changed_since: Option<u64>,
    pub after: Option<u64>,
    pub limit: usize,
}

/// A submission ready to store: validated and resolved against the world by the engine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubmitSpec {
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub batch: Option<String>,
    pub task: Option<u64>,
    pub captures: Vec<Capture>,
    pub checks: Vec<Check>,
    pub roots: Vec<Root>,
    pub journal: Vec<u64>,
    /// See [`Submission::journal_epoch`] and [`Submission::journal_ids`].
    pub journal_epoch: String,
    pub journal_ids: Option<Vec<u64>>,
    pub promote: Option<Promote>,
    pub supersedes: Option<u64>,
}

/// See the module docs.
#[derive(Default)]
pub struct Review {
    subs: BTreeMap<u64, Submission>,
    crits: BTreeMap<u64, Critique>,
    next_sub: u64,
    next_crit: u64,
    /// The last change's sequence number (feeds: `changed_since`).
    seq: u64,
    /// Bumped by every change that is saved.
    revision: u64,
    bytes: usize,
    /// `review.json` as last rendered, at which revision.
    rendered: Option<(u64, String)>,
}

impl Review {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Rewrites every file a submission or critique names (`f` answers the new path, or `None`
    /// to keep it); returns how many changed. A change is saved like any other (the revision moves).
    pub fn map_files(&mut self, f: &mut dyn FnMut(&str) -> Option<String>) -> usize {
        let mut n = 0;
        let mut one = |s: &mut String| {
            if let Some(new) = f(s).filter(|new| new != s) {
                *s = new;
                n += 1;
            }
        };
        for sub in self.subs.values_mut() {
            for c in &mut sub.captures {
                one(&mut c.path);
                if let Some(p) = &mut c.packet {
                    one(p);
                }
            }
        }
        for c in self.crits.values_mut() {
            if let Some(p) = &mut c.capture {
                one(p);
            }
            if let Some(p) = &mut c.packet {
                one(p);
            }
        }
        if n > 0 {
            self.bump();
            self.rendered = None;
        }
        n
    }

    /// Every file a submission or critique names (captures and packets): their evidence.
    pub fn file_refs(&self) -> Vec<&str> {
        let subs = self.subs.values().flat_map(|s| s.captures.iter().flat_map(|c| std::iter::once(c.path.as_str()).chain(c.packet.as_deref())));
        let crits = self.crits.values().flat_map(|c| c.capture.as_deref().into_iter().chain(c.packet.as_deref()));
        subs.chain(crits).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.subs.is_empty() && self.crits.is_empty()
    }

    /// [`Review::render`], again only when something changed since the last call.
    pub fn render_cached(&mut self) -> String {
        if let Some((rev, text)) = &self.rendered {
            if *rev == self.revision {
                return text.clone();
            }
        }
        let text = self.render();
        self.rendered = Some((self.revision, text.clone()));
        text
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn get(&self, id: u64) -> Option<&Submission> {
        self.subs.get(&id)
    }

    pub fn critique(&self, id: u64) -> Option<&Critique> {
        self.crits.get(&id)
    }

    pub fn submissions(&self) -> impl Iterator<Item = &Submission> + '_ {
        self.subs.values()
    }

    pub fn critiques(&self) -> impl Iterator<Item = &Critique> + '_ {
        self.crits.values()
    }

    pub fn pending(&self) -> impl Iterator<Item = &Submission> + '_ {
        self.subs.values().filter(|s| s.status == Status::Pending)
    }

    pub fn pending_count(&self) -> usize {
        self.pending().count()
    }

    pub fn critiques_on(&self, submission: u64) -> Vec<&Critique> {
        self.crits.values().filter(|c| c.submission == Some(submission)).collect()
    }

    fn open_on(&self, submission: u64) -> usize {
        self.crits.values().filter(|c| c.submission == Some(submission) && c.status.open()).count()
    }

    fn bump(&mut self) -> u64 {
        self.seq += 1;
        self.revision += 1;
        self.seq
    }

    fn check_budget(&self, more: usize) -> Result<(), WorldError> {
        if self.bytes + more <= MAX_BYTES {
            return Ok(());
        }
        Err(err(
            "review_full",
            format!(
                "the review queue and critiques hold about {} MiB of text, their limit is {} MiB: decide pending submissions and \
                 resolve or dismiss critiques (the oldest decided and closed ones then make room)",
                self.bytes >> 20,
                MAX_BYTES >> 20
            ),
        )
        .with("bytes", self.bytes)
        .with("budget", MAX_BYTES))
    }

    fn resize_sub(&mut self, id: u64) {
        if let Some(s) = self.subs.get_mut(&id) {
            let n = s.measure();
            self.bytes = (self.bytes + n).saturating_sub(s.size);
            s.size = n;
        }
    }

    fn resize_crit(&mut self, id: u64) {
        if let Some(c) = self.crits.get_mut(&id) {
            let n = c.measure();
            self.bytes = (self.bytes + n).saturating_sub(c.size);
            c.size = n;
        }
    }

    /// Drops the oldest decided submissions while over the count or byte limit.
    fn room_for_submission(&mut self, size: usize) -> Result<(), WorldError> {
        while self.subs.len() >= MAX_SUBMISSIONS || self.bytes + size > MAX_BYTES {
            let Some(old) = self.subs.values().find(|s| s.status != Status::Pending).map(|s| s.id) else { break };
            if let Some(s) = self.subs.remove(&old) {
                self.bytes = self.bytes.saturating_sub(s.size);
            }
        }
        if self.subs.len() >= MAX_SUBMISSIONS {
            return Err(err("too_many_submissions", format!("the queue holds {MAX_SUBMISSIONS} pending submissions; decide some first")).with("pending", self.subs.len()));
        }
        self.check_budget(size)
    }

    fn room_for_critiques(&mut self, n: usize, size: usize) -> Result<(), WorldError> {
        while self.crits.len() + n > MAX_CRITIQUES || self.bytes + size > MAX_BYTES {
            let Some(old) = self.crits.values().find(|c| !c.status.open()).map(|c| c.id) else { break };
            if let Some(c) = self.crits.remove(&old) {
                self.bytes = self.bytes.saturating_sub(c.size);
            }
        }
        if self.crits.len() + n > MAX_CRITIQUES {
            return Err(err("too_many_critiques", format!("{MAX_CRITIQUES} critiques are open; resolve or dismiss some first")).with("open", self.crits.len()));
        }
        self.check_budget(size)
    }

    /// `review_submit`: stores a resolved submission. `supersedes` withdraws an older pending one of
    /// the same submitter (or links a rejected one).
    pub fn submit(&mut self, w: Writer, tick: u64, spec: SubmitSpec) -> Result<&Submission, WorldError> {
        if let Some(old) = spec.supersedes {
            let o = self.subs.get(&old).ok_or_else(|| err("submission_not_found", format!("supersedes {old}: there is no submission {old}")).with("submission", old))?;
            // A submission read back after a restart knows its submitter by name alone: it can't
            // be superseded (a namesake would withdraw it); submit anew and let a reviewer decide it.
            if !o.submitted_by(w) || o.by.key.is_none() {
                return Err(err("not_yours", format!("submission {old} was submitted by {}; only they may supersede it, and only in the run it was made in", o.by.name)).with("submission", old));
            }
            if matches!(o.status, Status::Approved | Status::Withdrawn) {
                return Err(err("already_decided", format!("submission {old} is {}; supersede a pending or rejected one", o.status.name())).with("submission", old));
            }
        }
        if let Some(t) = spec.task {
            if self.subs.values().any(|s| s.status == Status::Pending && s.task == Some(t) && s.title == spec.title && Some(s.id) != spec.supersedes) {
                return Err(err(
                    "duplicate_submission",
                    format!("a pending submission for task {t} already has this title; pass supersedes: <its id> to replace it"),
                )
                .with("task", t));
            }
        }
        let mut s = Submission {
            id: 0,
            status: Status::Pending,
            kind: spec.kind,
            title: spec.title,
            summary: spec.summary,
            batch: spec.batch,
            by: Owner { name: w.name.to_owned(), key: w.session.map(str::to_owned) },
            task: spec.task,
            captures: spec.captures,
            checks: spec.checks,
            roots: spec.roots,
            journal: spec.journal,
            journal_epoch: spec.journal_epoch,
            journal_ids: spec.journal_ids,
            promote: spec.promote,
            supersedes: spec.supersedes,
            superseded_by: None,
            tick,
            updated_seq: 0,
            decision: None,
            size: 0,
        };
        let size = s.measure();
        self.room_for_submission(size)?;
        self.next_sub += 1;
        s.id = self.next_sub;
        let id = s.id;
        s.updated_seq = self.bump();
        self.subs.insert(id, s);
        self.resize_sub(id);
        if let Some(old) = self.subs[&id].supersedes {
            let seq = self.bump();
            if let Some(o) = self.subs.get_mut(&old) {
                o.superseded_by = Some(id);
                o.updated_seq = seq;
                if o.status == Status::Pending {
                    o.status = Status::Withdrawn;
                    o.decision = Some(Decision { verdict: Status::Withdrawn, by: w.name.to_owned(), note: format!("superseded by {id}"), tick, promoted: None, task: None, reverted: None });
                }
            }
            self.resize_sub(old);
        }
        Ok(&self.subs[&id])
    }

    /// Why `w` (with `authority`, from [`crate::authority::authority`]) may not decide
    /// submission `id` with `verdict`, if it may not.
    pub fn check_decide(&self, w: Writer, authority: Authority, id: u64, verdict: Status) -> Result<&Submission, WorldError> {
        let s = self.subs.get(&id).ok_or_else(|| err("submission_not_found", format!("there is no submission {id} (see review_list)")).with("submission", id))?;
        if s.status != Status::Pending {
            return Err(err("already_decided", format!("submission {id} is already {}", s.status.name()))
                .with("submission", id)
                .with("status", s.status.name()));
        }
        if verdict == Status::Withdrawn {
            // After a restart the submitter is known by name alone: a namesake withdraws it only
            // with reviewer authority.
            if !s.submitted_by(w) || (s.by.key.is_none() && authority < Authority::Reviewer) {
                return Err(crate::authority::refusal(crate::authority::Site::CritiqueUpdate, format!("only {} (who submitted {id}) may withdraw it; after a restart the submitter is known by name alone, so withdrawing then needs reviewer authority", s.by.name)).with("submission", id));
            }
            return Ok(s);
        }
        crate::authority::require(authority, Authority::Reviewer, crate::authority::Site::ReviewDecide, "deciding submissions (the owner can also decide at the window's review screen)")?;
        if s.submitted_by(w) {
            return Err(err("self_review", format!("you submitted {id}: another reviewer (or the owner) decides it")).with("submission", id));
        }
        Ok(s)
    }

    /// Records a decision (the engine did the world side first).
    pub fn decide(&mut self, id: u64, decision: Decision) -> Result<&Submission, WorldError> {
        let seq = self.bump();
        let s = self.subs.get_mut(&id).ok_or_else(|| err("submission_not_found", format!("there is no submission {id}")).with("submission", id))?;
        s.status = decision.verdict;
        s.decision = Some(decision);
        s.updated_seq = seq;
        self.resize_sub(id);
        Ok(&self.subs[&id])
    }

    /// `review_list`: one page in id order, with counts per status.
    pub fn list(&self, q: &SubmissionQuery, epoch: &str) -> Json {
        let matching = self.subs.values().filter(|s| {
            (q.statuses.is_empty() || q.statuses.contains(&s.status))
                && q.kind.as_deref().is_none_or(|k| s.kind == k)
                && q.batch.as_deref().is_none_or(|b| s.batch.as_deref() == Some(b))
                && q.by.as_deref().is_none_or(|b| s.by.name == b)
                && q.task.is_none_or(|t| s.task == Some(t))
                && q.changed_since.is_none_or(|c| s.updated_seq > c)
        });
        let total = matching.clone().count();
        let (items, next) = page(matching.filter(|s| q.after.is_none_or(|a| s.id > a)).map(|s| (s.id, s.summary_json(self.open_on(s.id), epoch))), q.limit);
        let counts: Vec<(&str, Json)> = Status::ALL.iter().map(|st| (st.name(), Json::from(self.subs.values().filter(|s| s.status == *st).count()))).collect();
        let mut batches: BTreeMap<&str, usize> = BTreeMap::new();
        for s in self.pending() {
            if let Some(b) = &s.batch {
                *batches.entry(b.as_str()).or_default() += 1;
            }
        }
        Json::obj([
            ("total", Json::from(total)),
            ("submissions", Json::Arr(items)),
            ("next_cursor", next),
            ("review_seq", Json::from(self.seq)),
            ("counts", Json::obj(counts)),
            ("pending_batches", Json::obj(batches.into_iter().take(32).map(|(k, n)| (k.to_owned(), Json::from(n))))),
        ])
    }

    // ---- critiques ----

    /// `critique_add`: all or nothing. `authority`: the author's ([`crate::authority::authority`];
    /// the owner's at the window). A source naming an authority role (`owner`, `lead`...) is
    /// refused unless the author has it: a source is the author's claim, never a credential.
    pub fn add_critiques(&mut self, w: Writer, authority: Authority, tick: u64, specs: Vec<CritiqueSpec>) -> Result<Vec<u64>, WorldError> {
        if let Some((i, s)) = specs.iter().enumerate().find(|(_, s)| Authority::of_role(&s.source).is_some_and(|a| a > authority)) {
            return Err(crate::authority::refusal(
                crate::authority::Site::CritiqueSource,
                format!("items[{i}]: source '{}' names an authority you don't have ({}); name the check or agent that found it (it defaults to you)", s.source, authority.name()),
            )
            .with("item", i)
            .with("source", s.source.as_str()));
        }
        if let Some((i, s)) = specs.iter().enumerate().find(|(_, s)| s.submission.is_some_and(|id| !self.subs.contains_key(&id))) {
            let id = s.submission.unwrap_or(0);
            return Err(err("submission_not_found", format!("items[{i}]: there is no submission {id}")).with("item", i).with("submission", id));
        }
        let size: usize = specs.iter().map(|s| 256 + s.text.len() + s.source.len() + s.capture.as_ref().map_or(0, String::len) + s.packet.as_ref().map_or(0, String::len)).sum();
        self.room_for_critiques(specs.len(), size)?;
        let mut ids = Vec::with_capacity(specs.len());
        for s in specs {
            self.next_crit += 1;
            let id = self.next_crit;
            let seq = self.bump();
            let status = if s.assignee.is_some() { CStatus::Assigned } else { CStatus::Open };
            self.crits.insert(
                id,
                Critique {
                    id,
                    version: 1,
                    status,
                    severity: s.severity,
                    source: s.source,
                    by: Owner { name: w.name.to_owned(), key: w.session.map(str::to_owned) },
                    authority,
                    text: s.text,
                    capture: s.capture,
                    packet: s.packet,
                    metric: s.metric,
                    target: s.target,
                    submission: s.submission,
                    assignee: s.assignee,
                    task: None,
                    resolution: String::new(),
                    resolved_by: None,
                    tick,
                    updated_tick: tick,
                    updated_seq: seq,
                    size: 0,
                },
            );
            self.resize_crit(id);
            ids.push(id);
        }
        Ok(ids)
    }

    /// Checks a `critique_update` action on `id` by `w` (with `authority`) without applying it;
    /// [`Review::apply_critique`] needs what it returns, so no change skips the check.
    ///
    /// Who may: its author anything; its assignee may resolve it; anyone else needs reviewer
    /// authority and at least its author's (an owner's critique is closed by the owner or
    /// resolved by its assignee).
    ///
    /// ```compile_fail
    /// // A check can't be made up: only check_critique returns one.
    /// let c = specula::review::CritiqueCheck { id: 1, version: 1 };
    /// ```
    pub fn check_critique(&self, w: Writer, authority: Authority, id: u64, action: &CritiqueAction, expect: Option<u64>) -> Result<CritiqueCheck, WorldError> {
        let c = self.crits.get(&id).ok_or_else(|| err("critique_not_found", format!("there is no critique {id} (see critique_list)")).with("critique", id))?;
        // After a restart the author is known by name alone (identity keys are per run): a
        // namesake is the author only with the authority the critique was made with.
        let author = c.by.is(w) && (c.by.key.is_some() || authority >= c.authority);
        let assignee = c.assignee.as_deref() == Some(w.name) && matches!(action, CritiqueAction::Resolve(_));
        let need = c.authority.max(Authority::Reviewer);
        if !author && !assignee && authority < need {
            return Err(crate::authority::refusal(
                crate::authority::Site::CritiqueUpdate,
                format!(
                    "critique {id} is {}'s{}: only its author, its assignee (to resolve it) or {} authority may change it (you have {})",
                    c.by.name,
                    if c.authority > Authority::Agent { format!(" (made with {} authority)", c.authority.name()) } else { String::new() },
                    need.name(),
                    authority.name()
                ),
            )
            .with("critique", id)
            .with("by", c.by.name.as_str())
            .with("needs", need.name()));
        }
        self.check_critique_state(c, id, action, expect)?;
        Ok(CritiqueCheck { id, version: c.version })
    }

    fn check_critique_state(&self, c: &Critique, id: u64, action: &CritiqueAction, expect: Option<u64>) -> Result<(), WorldError> {
        if let Some(v) = expect.filter(|v| *v != c.version) {
            return Err(err("stale_critique", format!("critique {id} is at version {}, not {v}: re-read it (critique_list) and retry", c.version))
                .with("critique", id)
                .with("current_version", c.version)
                .with("current", c.summary_json()));
        }
        match action {
            CritiqueAction::Reopen if c.status.open() => Err(err("not_closed", format!("critique {id} is {}; only a resolved or dismissed one reopens", c.status.name())).with("critique", id)),
            CritiqueAction::Reopen => Ok(()),
            _ if !c.status.open() => Err(err("closed", format!("critique {id} is {}; reopen it first", c.status.name())).with("critique", id)),
            CritiqueAction::ToTask { .. } if c.status == CStatus::InTask => {
                Err(err("already_in_task", format!("critique {id} is already carried by task {}", c.task.unwrap_or(0))).with("critique", id).with("task", c.task.unwrap_or(0)))
            }
            _ => Ok(()),
        }
    }

    /// Applies a checked `critique_update` (`task`: the board task `ToTask` posted). Refused when
    /// the critique changed since `check` was made.
    pub fn apply_critique(&mut self, check: CritiqueCheck, by: &str, tick: u64, action: CritiqueAction, task: Option<u64>) -> Result<&Critique, WorldError> {
        let id = check.id;
        if self.crits.get(&id).map(|c| c.version) != Some(check.version) {
            return Err(err("stale_critique", format!("critique {id} changed since it was checked: re-read it and retry")).with("critique", id));
        }
        let seq = self.bump();
        let c = self.crits.get_mut(&id).ok_or_else(|| err("critique_not_found", format!("there is no critique {id}")).with("critique", id))?;
        match action {
            CritiqueAction::Assign(a) => {
                c.assignee = Some(a);
                if c.status == CStatus::Open {
                    c.status = CStatus::Assigned;
                }
            }
            CritiqueAction::Resolve(note) => {
                c.status = CStatus::Resolved;
                c.resolution = note;
                c.resolved_by = Some(by.to_owned());
            }
            CritiqueAction::Dismiss(reason) => {
                c.status = CStatus::Dismissed;
                c.resolution = reason;
                c.resolved_by = Some(by.to_owned());
            }
            CritiqueAction::Reopen => {
                c.status = if c.assignee.is_some() { CStatus::Assigned } else { CStatus::Open };
                c.resolved_by = None;
                c.task = None;
            }
            CritiqueAction::ToTask { assign } => {
                c.status = CStatus::InTask;
                c.task = task;
                if assign.is_some() {
                    c.assignee = assign;
                }
            }
        }
        c.version += 1;
        c.updated_tick = tick;
        c.updated_seq = seq;
        self.resize_crit(id);
        Ok(&self.crits[&id])
    }

    /// Critiques carried by board tasks follow them: a done task resolves its critique (with the
    /// task's summary), a cancelled or vanished one opens it again. Returns how many changed.
    pub fn sync_tasks(&mut self, tick: u64, task: impl Fn(u64) -> Option<(crate::board::Status, String, String)>) -> usize {
        let carried: Vec<u64> = self.crits.values().filter(|c| c.status == CStatus::InTask).map(|c| c.id).collect();
        let mut changed = 0;
        for id in carried {
            let Some(t) = self.crits[&id].task else { continue };
            let next = match task(t) {
                Some((crate::board::Status::Done, owner, summary)) => {
                    Some((CStatus::Resolved, format!("task {t} done{}", if summary.is_empty() { String::new() } else { format!(": {}", short(&summary, 300)) }), Some(owner)))
                }
                Some((crate::board::Status::Cancelled, _, _)) => Some((CStatus::Open, format!("task {t} was cancelled"), None)),
                None => Some((CStatus::Open, format!("task {t} is no longer on the board"), None)),
                Some(_) => None,
            };
            let Some((status, note, by)) = next else { continue };
            let seq = self.bump();
            if let Some(c) = self.crits.get_mut(&id) {
                c.status = if status == CStatus::Open && c.assignee.is_some() { CStatus::Assigned } else { status };
                c.resolution = note;
                c.resolved_by = by;
                if status == CStatus::Open {
                    c.task = None;
                }
                c.version += 1;
                c.updated_tick = tick;
                c.updated_seq = seq;
            }
            self.resize_crit(id);
            changed += 1;
        }
        changed
    }

    /// `critique_list`: one page in id order, with counts per status.
    pub fn list_critiques(&self, q: &CritiqueQuery) -> Json {
        let matching = self.crits.values().filter(|c| {
            (q.statuses.is_empty() || q.statuses.contains(&c.status))
                && q.severity.as_deref().is_none_or(|s| c.severity == s)
                && q.source.as_deref().is_none_or(|s| c.source == s)
                && q.submission.is_none_or(|s| c.submission == Some(s))
                && q.assignee.as_deref().is_none_or(|a| c.assignee.as_deref() == Some(a))
                && q.changed_since.is_none_or(|s| c.updated_seq > s)
                && q.path_prefix.as_deref().is_none_or(|p| match &c.target {
                    Some(Target::Entity { path, .. } | Target::Path(path)) => path_under(path, p),
                    None => false,
                })
        });
        let total = matching.clone().count();
        let detailed = q.detailed;
        let (items, next) = page(
            matching.filter(|c| q.after.is_none_or(|a| c.id > a)).map(|c| (c.id, if detailed { c.to_json() } else { c.summary_json() })),
            q.limit,
        );
        let counts: Vec<(&str, Json)> = CStatus::ALL.iter().map(|st| (st.name(), Json::from(self.crits.values().filter(|c| c.status == *st).count()))).collect();
        Json::obj([
            ("total", Json::from(total)),
            ("critiques", Json::Arr(items)),
            ("next_cursor", next),
            ("review_seq", Json::from(self.seq)),
            ("counts", Json::obj(counts)),
        ])
    }

    // ---- persistence ----

    /// The store as `review.json` text.
    pub fn render(&self) -> String {
        let mut out = format!("{{\"format\": \"{FORMAT}\", \"next_submission\": {}, \"next_critique\": {}, \"seq\": {},\n\"submissions\": [", self.next_sub, self.next_crit, self.seq);
        for (i, s) in self.subs.values().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            let mut j = s.to_json(&[], &s.journal_epoch);
            // The file keeps every field; critiques are saved on their own.
            j.set("summary", Json::from(s.summary.as_str()));
            if let Some(ids) = s.journal_ids.as_ref().filter(|_| !s.journal.is_empty()) {
                j.set("journal_ids", Json::arr(ids.iter().map(|i| Json::from(*i))));
            }
            out.push_str(&j.to_string());
        }
        out.push_str("\n],\n\"critiques\": [");
        for (i, c) in self.crits.values().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            out.push_str(&c.to_json().to_string());
        }
        out.push_str("\n]}\n");
        out
    }

    /// Reads `review.json` text (see [`Review::render`]). Owners come back by name.
    pub fn parse(text: &str) -> Result<Review, String> {
        let doc = Json::parse(text).map_err(|e| e.to_string())?;
        if doc.get("format").and_then(Json::as_str) != Some(FORMAT) {
            return Err(format!("not a review file (format must be \"{FORMAT}\")"));
        }
        let num = |j: &Json, k: &str| j.get(k).and_then(Json::as_u64).unwrap_or(0);
        let text = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
        let opt = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).map(str::to_owned);
        let f64_of = |j: &Json, k: &str| j.get(k).and_then(Json::as_f64);
        let mut r = Review { next_sub: num(&doc, "next_submission"), next_crit: num(&doc, "next_critique"), seq: num(&doc, "seq"), ..Review::default() };
        for s in doc.get("submissions").and_then(Json::as_array).unwrap_or_default().iter().take(MAX_SUBMISSIONS) {
            let id = s.get("id").and_then(Json::as_u64).ok_or("a submission has no id")?;
            let status = s.get("status").and_then(Json::as_str).and_then(Status::parse).ok_or(format!("submission {id}: bad status"))?;
            let arr = |k: &str| s.get(k).and_then(Json::as_array).unwrap_or_default();
            let captures = arr("captures")
                .iter()
                .take(MAX_CAPTURES)
                .map(|c| Capture { path: text(c, "path"), kind: text(c, "kind"), packet: opt(c, "packet"), caption: text(c, "caption") })
                .collect();
            let checks = arr("checks")
                .iter()
                .take(MAX_CHECKS)
                .map(|c| Check {
                    kind: text(c, "kind"),
                    name: text(c, "name"),
                    passed: {
                        let v = c.get("passed").and_then(Json::as_bool).unwrap_or(false);
                        if c.get("engine").and_then(Json::as_bool) == Some(true) {
                            Fact::Engine(v)
                        } else {
                            Fact::Claimed(Claimed { by: opt(c, "reported_by").unwrap_or_else(|| text(s, "submitted_by")), value: v })
                        }
                    },
                    severity: text(c, "severity"),
                    metric: opt(c, "metric"),
                    value: f64_of(c, "value"),
                    limit: f64_of(c, "limit"),
                    detail: text(c, "detail"),
                })
                .collect();
            let roots = arr("roots")
                .iter()
                .take(MAX_PATHS)
                .map(|x| Root {
                    path: text(x, "path"),
                    id: num(x, "id"),
                    version: num(x, "version"),
                    stamp: u64::from_str_radix(&text(x, "stamp"), 16).unwrap_or(0),
                    entities: num(x, "entities") as usize,
                })
                .collect();
            let decision = s.get("decision").filter(|d| matches!(d, Json::Obj(_))).map(|d| Decision {
                verdict: d.get("verdict").and_then(Json::as_str).and_then(Status::parse).unwrap_or(status),
                by: text(d, "by"),
                note: text(d, "note"),
                tick: num(d, "tick"),
                promoted: d.get("promoted").cloned(),
                task: d.get("task").and_then(Json::as_u64),
                reverted: d.get("reverted").and_then(Json::as_u64),
            });
            let promote = s.get("promote").filter(|p| matches!(p, Json::Obj(_))).map(|p| Promote {
                to: text(p, "to"),
                replace: p.get("replace").and_then(Json::as_bool).unwrap_or(false),
                remove_tag: opt(p, "remove_tag"),
            });
            r.subs.insert(
                id,
                Submission {
                    id,
                    status,
                    kind: text(s, "kind"),
                    title: text(s, "title"),
                    summary: text(s, "summary"),
                    batch: opt(s, "batch"),
                    by: Owner { name: text(s, "submitted_by"), key: None },
                    task: s.get("task").and_then(Json::as_u64),
                    captures,
                    checks,
                    roots,
                    journal: arr("journal").iter().filter_map(Json::as_u64).take(MAX_JOURNAL).collect(),
                    // Files from before epochs have none: their refs are never current.
                    journal_epoch: text(s, "journal_epoch"),
                    journal_ids: s.get("journal_ids").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_u64).take(MAX_JOURNAL_IDS).collect()),
                    promote,
                    supersedes: s.get("supersedes").and_then(Json::as_u64),
                    superseded_by: s.get("superseded_by").and_then(Json::as_u64),
                    tick: num(s, "tick"),
                    updated_seq: num(s, "updated_seq"),
                    decision,
                    size: 0,
                },
            );
            r.next_sub = r.next_sub.max(id);
            r.resize_sub(id);
        }
        for c in doc.get("critiques").and_then(Json::as_array).unwrap_or_default().iter().take(MAX_CRITIQUES) {
            let id = c.get("id").and_then(Json::as_u64).ok_or("a critique has no id")?;
            let status = c.get("status").and_then(Json::as_str).and_then(CStatus::parse).ok_or(format!("critique {id}: bad status"))?;
            let target = match (c.get("entity"), opt(c, "path")) {
                (Some(e @ Json::Obj(_)), _) => Some(Target::Entity { id: num(e, "id"), path: text(e, "path") }),
                (_, Some(p)) => Some(Target::Path(p)),
                _ => None,
            };
            let metric = c.get("metric").filter(|m| matches!(m, Json::Obj(_))).map(|m| Metric { name: text(m, "name"), value: f64_of(m, "value"), limit: f64_of(m, "limit") });
            r.crits.insert(
                id,
                Critique {
                    id,
                    version: num(c, "version").max(1),
                    status,
                    severity: text(c, "severity"),
                    source: text(c, "source"),
                    authority: c.get("authority").and_then(Json::as_str).and_then(Authority::parse).or_else(|| crate::authority::of_writer_name(&text(c, "by"))).unwrap_or_default(),
                    by: Owner { name: text(c, "by"), key: None },
                    text: text(c, "text"),
                    capture: opt(c, "capture"),
                    packet: opt(c, "packet"),
                    metric,
                    target,
                    submission: c.get("submission").and_then(Json::as_u64),
                    assignee: opt(c, "assignee"),
                    task: c.get("task").and_then(Json::as_u64),
                    resolution: text(c, "resolution"),
                    resolved_by: opt(c, "resolved_by"),
                    tick: num(c, "tick"),
                    updated_tick: num(c, "updated_tick"),
                    updated_seq: num(c, "updated_seq"),
                    size: 0,
                },
            );
            r.next_crit = r.next_crit.max(id);
            r.resize_crit(id);
        }
        Ok(r)
    }
}

/// Items in order until `limit` or the byte budget, with the cursor for the rest.
fn page(items: impl Iterator<Item = (u64, Json)>, limit: usize) -> (Vec<Json>, Json) {
    let mut out = Vec::new();
    let mut bytes = 0usize;
    let mut last = None;
    let mut more = false;
    for (id, j) in items {
        if out.len() >= limit.max(1) {
            more = true;
            break;
        }
        let n = j.to_string().len();
        if !out.is_empty() && bytes + n > MAX_LIST_BYTES {
            more = true;
            break;
        }
        bytes += n;
        last = Some(id);
        out.push(j);
    }
    (out, if more { last.map_or(Json::Null, Json::from) } else { Json::Null })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANN: Writer = Writer { name: "ann", session: Some("k-ann") };
    const BOB: Writer = Writer { name: "bob", session: Some("k-bob") };

    fn spec(title: &str) -> SubmitSpec {
        SubmitSpec {
            kind: "asset".into(),
            title: title.into(),
            summary: "twelve props".into(),
            batch: Some("crypt".into()),
            task: Some(4),
            captures: vec![Capture { path: "c/orbit.png".into(), kind: "contact_sheet".into(), packet: None, caption: String::new() }],
            checks: vec![
                Check { kind: "style".into(), name: "budget".into(), passed: Fact::Claimed(Claimed { by: "ann".into(), value: true }), severity: "error".into(), metric: None, value: None, limit: None, detail: String::new() },
                Check { kind: "taste".into(), name: "silhouette".into(), passed: Fact::Claimed(Claimed { by: "ann".into(), value: false }), severity: "warning".into(), metric: Some("iou".into()), value: Some(0.8), limit: Some(0.75), detail: String::new() },
                Check { kind: "style".into(), name: "palette".into(), passed: Fact::Claimed(Claimed { by: "ann".into(), value: false }), severity: "error".into(), metric: None, value: None, limit: None, detail: String::new() },
            ],
            roots: vec![Root { path: "/staging/crypt".into(), id: 7, version: 2, stamp: 0xabc, entities: 12 }],
            journal: vec![3, 4],
            journal_epoch: "00000000000000e1.0.1".into(),
            journal_ids: Some(vec![7, 8]),
            promote: Some(Promote { to: "/world/props".into(), replace: false, remove_tag: Some("staging".into()) }),
            supersedes: None,
        }
    }

    #[test]
    fn submissions_are_decided_once_and_never_by_their_author() {
        let mut r = Review::default();
        let s = r.submit(ANN, 10, spec("crypt props")).unwrap();
        assert_eq!((s.id, s.status), (1, Status::Pending));
        assert_eq!(s.failed().iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["palette", "silhouette"], "errors first");
        assert_eq!(r.check_decide(ANN, Authority::Reviewer, 1, Status::Approved).unwrap_err().code, "self_review");
        assert_eq!(r.check_decide(BOB, Authority::Agent, 1, Status::Approved).unwrap_err().code, "not_reviewer");
        assert_eq!(r.check_decide(BOB, Authority::Owner, 1, Status::Withdrawn).unwrap_err().code, "not_yours");
        assert!(r.check_decide(BOB, Authority::Reviewer, 1, Status::Approved).is_ok());
        assert!(r.check_decide(BOB, Authority::Owner, 1, Status::Rejected).is_ok(), "the window is the owner");
        // A second identical pending submission for the task is refused.
        assert_eq!(r.submit(ANN, 11, spec("crypt props")).unwrap_err().code, "duplicate_submission");
        let d = Decision { verdict: Status::Rejected, by: "bob".into(), note: "palette".into(), tick: 12, promoted: None, task: Some(9), reverted: None };
        r.decide(1, d).unwrap();
        assert_eq!(r.check_decide(BOB, Authority::Reviewer, 1, Status::Approved).unwrap_err().code, "already_decided");
        // Resubmitting links the rejected one; a pending one superseded is withdrawn.
        let mut s2 = spec("crypt props v2");
        s2.supersedes = Some(1);
        let id2 = r.submit(ANN, 13, s2).unwrap().id;
        assert_eq!((r.get(1).unwrap().superseded_by, r.get(1).unwrap().status), (Some(id2), Status::Rejected));
        let mut s3 = spec("crypt props v3");
        s3.supersedes = Some(id2);
        assert_eq!(r.submit(BOB, 14, s3.clone()).unwrap_err().code, "not_yours");
        let id3 = r.submit(ANN, 14, s3).unwrap().id;
        assert_eq!(r.get(id2).unwrap().status, Status::Withdrawn);
        assert_eq!(r.pending_count(), 1);
        let list = r.list(&SubmissionQuery { statuses: vec![Status::Pending], limit: 10, ..Default::default() }, "");
        assert_eq!(list.get("total"), Some(&Json::from(1)));
        let item = &list.get("submissions").unwrap().as_array().unwrap()[0];
        assert_eq!(item.get("id"), Some(&Json::from(id3)));
        assert_eq!(item.get("checks").unwrap().get("failed"), Some(&Json::from(2)));
        assert_eq!(list.get("pending_batches").unwrap().get("crypt"), Some(&Json::from(1)));
    }

    #[test]
    fn critiques_follow_their_tasks() {
        let mut r = Review::default();
        r.submit(ANN, 1, spec("x")).unwrap();
        let c = CritiqueSpec { text: "the rim light is flat".into(), severity: "warning".into(), source: "sight-skeptic".into(), submission: Some(1), ..Default::default() };
        assert_eq!(r.add_critiques(BOB, Authority::Agent, 2, vec![CritiqueSpec { submission: Some(99), ..c.clone() }]).unwrap_err().code, "submission_not_found");
        let ids = r.add_critiques(BOB, Authority::Agent, 2, vec![c.clone(), c.clone()]).unwrap();
        assert_eq!(ids, [1, 2]);
        let a = Authority::Agent;
        assert!(r.check_critique(BOB, a, 1, &CritiqueAction::Reopen, None).is_err());
        assert_eq!(r.check_critique(BOB, a, 1, &CritiqueAction::Resolve("x".into()), Some(5)).unwrap_err().code, "stale_critique");
        let k = r.check_critique(BOB, a, 1, &CritiqueAction::ToTask { assign: Some("ann".into()) }, None).unwrap();
        r.apply_critique(k, "bob", 3, CritiqueAction::ToTask { assign: Some("ann".into()) }, Some(40)).unwrap();
        let k = r.check_critique(BOB, a, 2, &CritiqueAction::ToTask { assign: None }, None).unwrap();
        r.apply_critique(k, "bob", 3, CritiqueAction::ToTask { assign: None }, Some(41)).unwrap();
        assert_eq!(r.check_critique(BOB, a, 1, &CritiqueAction::ToTask { assign: None }, None).unwrap_err().code, "already_in_task");
        use crate::board::Status as T;
        let n = r.sync_tasks(4, |t| match t {
            40 => Some((T::Done, "ann".into(), "lit from the left".into())),
            41 => Some((T::Cancelled, String::new(), String::new())),
            _ => None,
        });
        assert_eq!(n, 2);
        let c1 = r.critique(1).unwrap();
        assert_eq!((c1.status, c1.resolved_by.as_deref()), (CStatus::Resolved, Some("ann")));
        assert!(c1.resolution.contains("lit from the left"));
        assert_eq!(r.critique(2).unwrap().status, CStatus::Open);
        assert_eq!(r.sync_tasks(5, |_| None), 0, "nothing in tasks any more");
        // Ann didn't write critique 2 and isn't its assignee: a reviewer may close it.
        assert_eq!(r.check_critique(ANN, a, 2, &CritiqueAction::Dismiss("by design".into()), None).unwrap_err().code, "not_yours");
        let k = r.check_critique(ANN, Authority::Reviewer, 2, &CritiqueAction::Dismiss("by design".into()), None).unwrap();
        r.apply_critique(k, "ann", 6, CritiqueAction::Dismiss("by design".into()), None).unwrap();
        assert!(r.check_critique(BOB, a, 2, &CritiqueAction::Assign("x".into()), None).is_err());
        let k = r.check_critique(BOB, a, 2, &CritiqueAction::Reopen, None).unwrap();
        r.apply_critique(k, "bob", 7, CritiqueAction::Reopen, None).unwrap();
        let list = r.list_critiques(&CritiqueQuery { statuses: vec![CStatus::Open], limit: 5, ..Default::default() });
        assert_eq!(list.get("total"), Some(&Json::from(1)));
        let full = r.get(1).unwrap().to_json(&r.critiques_on(1), "");
        assert_eq!(full.get("critiques").unwrap().as_array().unwrap().len(), 2);
        assert_eq!(full.get("open_critiques"), Some(&Json::from(1)));
    }

    #[test]
    fn the_store_round_trips_through_its_file() {
        let mut r = Review::default();
        r.submit(ANN, 1, spec("a")).unwrap();
        r.submit(ANN, 2, SubmitSpec { task: None, ..spec("b") }).unwrap();
        let d = Decision { verdict: Status::Approved, by: "bob".into(), note: String::new(), tick: 3, promoted: Some(Json::obj([("journal", Json::from(8))])), task: None, reverted: None };
        r.decide(2, d).unwrap();
        let c = CritiqueSpec {
            text: "too shiny".into(),
            severity: "note".into(),
            source: "owner".into(),
            metric: Some(Metric { name: "spec".into(), value: Some(0.9), limit: Some(0.5) }),
            target: Some(Target::Entity { id: 7, path: "/staging/crypt".into() }),
            ..Default::default()
        };
        r.add_critiques(BOB, Authority::Owner, 4, vec![c]).unwrap();
        let text = r.render();
        let back = Review::parse(&text).unwrap();
        assert_eq!(back.render(), text, "render(parse(render)) is stable");
        assert_eq!(back.get(1).unwrap().by, Owner { name: "ann".into(), key: None }, "loaded owners come back by name");
        assert_eq!(back.get(1).unwrap().roots[0].stamp, 0xabc);
        assert_eq!(back.get(2).unwrap().decision.as_ref().unwrap().promoted, Some(Json::obj([("journal", Json::from(8))])));
        assert_eq!(back.critique(1).unwrap().metric.as_ref().unwrap().limit, Some(0.5));
        assert!(Review::parse("{\"format\": \"x\"}").is_err());
        // A loaded submission's author is matched by name alone.
        assert!(back.get(1).unwrap().submitted_by(Writer { name: "ann", session: Some("new-key") }));
        // M4 audit (probe_revert.py): journal refs keep their epoch and the ids they touched, and
        // count only in that epoch (a restart numbers entries from 1 again).
        let s = back.get(1).unwrap();
        assert_eq!((s.journal_epoch.as_str(), s.journal_ids.as_deref()), ("00000000000000e1.0.1", Some(&[7u64, 8][..])));
        assert_eq!(s.journal_in("00000000000000e1.0.1"), &[3, 4]);
        assert!(s.journal_in("00000000000000e2.0.1").is_empty());
        assert_eq!(s.summary_json(0, "00000000000000e2.0.1").get("journal_stale").and_then(Json::as_str), Some(STALE_JOURNAL));
        assert!(s.summary_json(0, "00000000000000e1.0.1").get("journal_stale").is_none());
        // Review of the M4 fixes: agents see a count, not up to 4,096 ids (the file keeps them).
        let full = s.to_json(&[], "00000000000000e1.0.1");
        assert!(full.get("journal_ids").is_none());
        assert_eq!(full.get("journal_entities"), Some(&Json::from(2)));
        assert!(text.contains("\"journal_ids\":[7,8]"));
        // A file from before epochs: its refs are never current.
        let old = text.replace("\"journal_epoch\":\"00000000000000e1.0.1\",", "");
        assert_ne!(old, text);
        let s = Review::parse(&old).unwrap().get(1).unwrap().clone();
        assert!(s.journal_epoch.is_empty() && s.journal_in("").is_empty());
    }

    #[test]
    fn listings_and_the_store_are_bounded() {
        let mut r = Review::default();
        for i in 0..30 {
            r.submit(ANN, i, SubmitSpec { summary: "x".repeat(MAX_SUMMARY), task: None, ..spec(&format!("s{i}")) }).unwrap();
        }
        let list = r.list(&SubmissionQuery { limit: 200, ..Default::default() }, "");
        assert!(list.to_string().len() < MAX_LIST_BYTES + 4096);
        let page1 = list.get("submissions").unwrap().as_array().unwrap().len();
        assert!(page1 >= 1);
        // Byte budget: a full store refuses more, after dropping what is decided.
        r.bytes = MAX_BYTES;
        assert_eq!(r.submit(ANN, 99, spec("late")).unwrap_err().code, "review_full");
    }

    /// Every provenance key a detailed item has, its concise item has too.
    fn provenance_kept(detailed: &Json, concise: &Json, what: &str) {
        let Json::Obj(pairs) = detailed else { panic!("{what}: an object") };
        for (k, _) in pairs {
            if crate::response::PROVENANCE.contains(&k.as_str()) {
                assert!(concise.get(k).is_some(), "{what}: the concise item drops provenance '{k}' (detailed has it): {concise}");
            }
        }
    }

    const CARL: Writer = Writer { name: "carl", session: Some("k-carl") };

    #[test]
    fn critiques_belong_to_their_authors() {
        let mut r = Review::default();
        r.submit(ANN, 1, spec("x")).unwrap();
        let c = CritiqueSpec { text: "too dark".into(), severity: "warning".into(), source: "sight-skeptic".into(), submission: Some(1), assignee: Some("ann".into()), ..Default::default() };
        // M4 finding: carl (no authority) resolved, dismissed and reopened anyone's critique,
        // even the owner's.
        let window = Writer::new(crate::authority::WINDOW_OWNER, Some(crate::authority::WINDOW_OWNER));
        let owner_c = r.add_critiques(window, Authority::Owner, 2, vec![CritiqueSpec { source: crate::authority::ROLE_OWNER.into(), ..c.clone() }]).unwrap()[0];
        let bobs = r.add_critiques(BOB, Authority::Agent, 2, vec![c.clone()]).unwrap()[0];
        let dismiss = CritiqueAction::Dismiss("meh".into());
        for (id, action) in [(owner_c, &dismiss), (bobs, &dismiss), (bobs, &CritiqueAction::Assign("carl".into())), (bobs, &CritiqueAction::ToTask { assign: None })] {
            let e = r.check_critique(CARL, Authority::Agent, id, action, None).unwrap_err();
            assert_eq!((e.code, e.detail("refused_at")), ("not_yours", Some(&Json::from("critique_update"))), "critique {id} {action:?}");
        }
        // A reviewer may close bob's, not the owner's; the assignee may resolve either.
        assert!(r.check_critique(CARL, Authority::Reviewer, bobs, &dismiss, None).is_ok());
        assert_eq!(r.check_critique(CARL, Authority::Lead, owner_c, &dismiss, None).unwrap_err().code, "not_yours");
        assert!(r.check_critique(ANN, Authority::Agent, owner_c, &CritiqueAction::Resolve("fixed".into()), None).is_ok());
        assert_eq!(r.check_critique(ANN, Authority::Agent, owner_c, &dismiss, None).unwrap_err().code, "not_yours", "an assignee resolves, never dismisses");
        assert!(r.check_critique(BOB, Authority::Agent, bobs, &dismiss, None).is_ok(), "the author may");
        // A check made before the critique changed is refused at apply.
        let k = r.check_critique(BOB, Authority::Agent, bobs, &dismiss, None).unwrap();
        let k2 = r.check_critique(BOB, Authority::Agent, bobs, &CritiqueAction::Assign("x".into()), None).unwrap();
        r.apply_critique(k2, "bob", 3, CritiqueAction::Assign("x".into()), None).unwrap();
        assert_eq!(r.apply_critique(k, "bob", 4, dismiss.clone(), None).unwrap_err().code, "stale_critique");
        // A source is a claim, not a credential: only the owner says "owner".
        for (who, a) in [(CARL, Authority::Agent), (CARL, Authority::Lead)] {
            let e = r.add_critiques(who, a, 5, vec![CritiqueSpec { source: crate::authority::ROLE_OWNER.into(), ..c.clone() }]).unwrap_err();
            assert_eq!(e.code, "source_reserved");
        }
        assert!(r.add_critiques(CARL, Authority::Lead, 5, vec![CritiqueSpec { source: crate::authority::ROLE_LEAD.into(), ..c.clone() }]).is_ok());
        // The owner's critique keeps its authority through the file.
        let back = Review::parse(&r.render()).unwrap();
        assert_eq!(back.critique(owner_c).unwrap().authority, Authority::Owner);
        assert_eq!(back.critique(bobs).unwrap().authority, Authority::Agent);
    }

    #[test]
    fn provenance_survives_concise_listings() {
        let mut r = Review::default();
        let mut s = spec("crypt");
        s.checks = vec![Check { kind: "style".into(), name: "palette".into(), passed: Fact::Claimed(Claimed { by: "ann".into(), value: true }), severity: "error".into(), metric: None, value: None, limit: None, detail: String::new() }];
        let id = r.submit(ANN, 1, s).unwrap().id;
        r.decide(id, Decision { verdict: Status::Approved, by: "bob".into(), note: "ok".into(), tick: 2, promoted: None, task: None, reverted: None }).unwrap();
        let c = CritiqueSpec { text: "x".into(), severity: "note".into(), source: "taste".into(), submission: Some(id), ..Default::default() };
        let cid = r.add_critiques(BOB, Authority::Agent, 3, vec![c]).unwrap()[0];
        let k = r.check_critique(BOB, Authority::Agent, cid, &CritiqueAction::Resolve("done".into()), None).unwrap();
        r.apply_critique(k, "bob", 4, CritiqueAction::Resolve("done".into()), None).unwrap();
        let crit = r.critique(cid).unwrap();
        provenance_kept(&crit.to_json(), &crit.summary_json(), "critique_list");
        assert_eq!(crit.summary_json().get("by"), Some(&Json::from("bob")));
        assert_eq!(crit.summary_json().get("resolved_by"), Some(&Json::from("bob")));
        let sub = r.get(id).unwrap();
        provenance_kept(&sub.to_json(&r.critiques_on(id), ""), &sub.summary_json(1, ""), "review_list");
        // The listing's counts keep engine results and reported ones apart.
        let checks = sub.summary_json(0, "").get("checks").cloned().unwrap();
        assert_eq!(checks.get("reported").and_then(|r| r.get("passed")), Some(&Json::from(1)));
        assert_eq!(checks.get("engine").and_then(|r| r.get("passed")), Some(&Json::from(0)));
    }

    #[test]
    fn the_checks_line_never_counts_reported_checks_as_the_engines() {
        let mut r = Review::default();
        let mut s = spec("crypt");
        let check = |name: &str, passed: Fact<bool>| Check { kind: "style".into(), name: name.into(), passed, severity: "error".into(), metric: None, value: None, limit: None, detail: String::new() };
        s.checks = vec![
            check("paths_exist", Fact::Engine(true)),
            check("captures_exist", Fact::Engine(true)),
            check("palette", Fact::Claimed(Claimed { by: "bob".into(), value: true })),
            check("budget", Fact::Claimed(Claimed { by: "bob".into(), value: true })),
        ];
        let id = r.submit(BOB, 1, s).unwrap().id;
        // M4 finding: the screen said "all 4 passed".
        assert_eq!(r.get(id).unwrap().checks_line(), "Checks: 2 engine check(s) passed; 2 reported by bob passed");
        // The screen shows them as two lines (the reported one in a neutral colour; review.json).
        let (engine, reported) = r.get(id).unwrap().checks_parts();
        assert_eq!(engine, "Engine checks: 2 passed");
        assert_eq!(reported, "Reported by bob (not checked by the engine): 2 passed");
        let ui = include_str!("../ui/review.json");
        assert!(ui.contains(r#""text": "{checks_reported}", "role": "small""#), "the reported part is neutral, never the engine's green");
        let text = r.render();
        let back = Review::parse(&text).unwrap();
        assert_eq!(back.get(id).unwrap().checks, r.get(id).unwrap().checks, "provenance round-trips through the file");
    }

    /// Review of the authority lane: after a restart (keys are per run) anyone with the owner's
    /// name could dismiss the owner's critique, and a namesake could withdraw or supersede a
    /// submission.
    #[test]
    fn namesakes_after_a_restart_are_not_the_author() {
        let mut r = Review::default();
        let boss = Writer::new("boss", Some("k-boss"));
        let c = CritiqueSpec { text: "too dark".into(), severity: "warning".into(), source: "taste".into(), ..Default::default() };
        let id = r.add_critiques(boss, Authority::Owner, 1, vec![c.clone()]).unwrap()[0];
        let imposter = Writer::new("boss", Some("k-other"));
        let dismiss = CritiqueAction::Dismiss("meh".into());
        assert!(r.check_critique(imposter, Authority::Agent, id, &dismiss, None).is_err());
        let mut back = Review::parse(&r.render()).unwrap();
        assert_eq!(back.check_critique(imposter, Authority::Agent, id, &dismiss, None).unwrap_err().code, "not_yours", "a namesake after reload");
        assert!(back.check_critique(imposter, Authority::Owner, id, &dismiss, None).is_ok(), "the owner (by authority) may");
        // An agent's own critique after a restart: its namesake with the same (agent) authority
        // is taken as its author (nothing better is known), as before.
        let mine = back.add_critiques(ANN, Authority::Agent, 2, vec![c]).unwrap()[0];
        let back2 = Review::parse(&back.render()).unwrap();
        assert!(back2.check_critique(Writer::new("ann", Some("k-x")), Authority::Agent, mine, &dismiss, None).is_ok());
        // Submissions: a namesake can't withdraw or supersede one read back from the file.
        r.submit(ANN, 3, spec("crypt")).unwrap();
        let mut back = Review::parse(&r.render()).unwrap();
        let ann2 = Writer::new("ann", Some("k-ann-2"));
        assert_eq!(back.check_decide(ann2, Authority::Agent, 1, Status::Withdrawn).unwrap_err().code, "not_yours");
        assert!(back.check_decide(ann2, Authority::Reviewer, 1, Status::Withdrawn).is_ok());
        assert_eq!(back.submit(ann2, 4, SubmitSpec { supersedes: Some(1), ..spec("crypt again") }).unwrap_err().code, "not_yours");
    }

    #[test]
    fn reserved_sources_are_compared_as_they_look() {
        let mut r = Review::default();
        r.submit(ANN, 1, spec("x")).unwrap();
        for src in ["Owner", "OWNER", "window:owner", "Window:Owner", "\u{043e}wner", "LEAD", "Reviewer"] {
            let c = CritiqueSpec { text: "x".into(), severity: "note".into(), source: src.into(), ..Default::default() };
            assert_eq!(r.add_critiques(BOB, Authority::Agent, 2, vec![c]).unwrap_err().code, "source_reserved", "{src:?}");
        }
    }
}
