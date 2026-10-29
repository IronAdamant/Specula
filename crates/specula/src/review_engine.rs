//! The engine side of the review, critique and director tools (arguments in `review_tools`):
//! resolving submissions against the world, decisions (promotion, reverts, rework tasks),
//! critiques' board tasks, plans and their progress, and saving the queue with the board.
//! See docs/design/REVIEW.md.

use crate::authority::Authority;
use crate::board::{Owner, Status as TaskStatus, TaskSpec};
use crate::conflict::Yours;
use crate::director::{self, PlanItem};
use crate::journal::{path_under, Kind, UndoRequest, Writer};
use crate::review::{self, Capture, Check, CritiqueAction, Decision, Review, Root, Status, SubmitSpec, Target, WINDOW_OWNER};
use crate::review_tools::{DecideOne, PlanArgs, PlanSource, ReviewCall, ScreenOp, SubmitArgs, Which, MAX_DECISIONS};
use crate::swarm::Swarm;
use crate::world::{EntityKey, Op, UpdateSpec, World, WorldError};
use crate::write_policy::{Rule, WorldWrite};
use crate::Engine;
use specula_core::Json;
use specula_protocol::{Server, SessionInfo, ToolOutput};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The queue's file in a world folder (beside `board.json`).
pub const REVIEW_FILE: &str = "review.json";
const REVIEW_TMP: &str = "review.json.tmp";
/// Journal entries `journal_since` scans (newest kept).
const JOURNAL_SCAN: usize = 50_000;
/// Ancestors followed to find a task's plan.
const MAX_PLAN_DEPTH: usize = 32;
/// A detailed `review_list` page stops adding whole submissions past this many bytes.
const MAX_DETAILED_BYTES: usize = 160 * 1024;

fn out(j: Json) -> ToolOutput {
    ToolOutput::text(j.to_string())
}

fn fail(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_string())
}

fn hello_required(what: &str) -> ToolOutput {
    fail(&WorldError::new("hello_required", format!("{what} belong to agent names: call agent_hello with a unique name (and a role) first")))
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        let mut o: String = s.chars().take(n.saturating_sub(1)).collect();
        o.push('…');
        o
    }
}

/// Writes `text` to `dir/review.json` (temporary file, then rename).
pub(crate) fn write_review(dir: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    // Essential (the review queue beside the world): accounted, written even below the floor.
    let old = crate::disk::file_len(&dir.join(REVIEW_FILE));
    crate::disk::book().admit(crate::disk::Category::Worlds, text.len() as u64, crate::disk::Need::Essential).map_err(std::io::Error::other)?;
    let tmp = dir.join(REVIEW_TMP);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, dir.join(REVIEW_FILE))?;
    crate::disk::book().replaced(crate::disk::Category::Worlds, "review", old, text.len() as u64);
    Ok(())
}

/// The ops that move a submission's roots under `p.to` (and take the staging tag off them), and
/// what moves where. Refused when the target is gone or a name there is taken (unless `replace`).
pub(crate) fn promotion_ops(world: &World, roots: &[Root], p: &review::Promote) -> Result<(Vec<Op>, Vec<Json>), WorldError> {
    world.find_path(&p.to).map_err(|_| WorldError::new("no_target", format!("promote.to '{}' no longer exists", p.to)).with("to", p.to.as_str()))?;
    let mut ops: Vec<Op> = Vec::new();
    let mut moved: Vec<Json> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    for r in roots {
        let Some(e) = world.get(r.id) else { continue };
        let dest = format!("{}/{}", p.to, e.name);
        let already_there = e.parent.is_some_and(|parent| world.path_of(parent) == p.to);
        if !already_there {
            if let Ok(other) = world.find_path(&dest) {
                if p.replace {
                    ops.push(Op::Delete(EntityKey::Id(other), None));
                } else {
                    taken.push(dest.clone());
                    continue;
                }
            }
        }
        let tag = p.remove_tag.as_ref().filter(|t| e.tags.contains(*t)).cloned();
        if already_there && tag.is_none() {
            continue;
        }
        ops.push(Op::Set(
            EntityKey::Id(r.id),
            UpdateSpec { parent: (!already_there).then(|| Some(EntityKey::Path(p.to.clone()))), remove_tags: tag.into_iter().collect(), ..Default::default() },
        ));
        moved.push(Json::obj([("from", Json::from(r.path.as_str())), ("to", Json::from(dest))]));
    }
    if !taken.is_empty() {
        return Err(WorldError::new(
            "name_taken",
            format!("{} already exist(s) under {}: pass promote.replace (resubmit), rename the work, or move what's there", taken.join(", "), p.to),
        )
        .with("taken", Json::arr(taken.into_iter().map(Json::from))));
    }
    Ok((ops, moved))
}

/// What a rejection's revert would take back: a submission's journal entries, the journal epoch
/// they were recorded in, who submitted them and the entity ids they touched then.
#[derive(Clone, Copy)]
pub(crate) struct RevertOf<'a> {
    pub id: u64,
    pub journal: &'a [u64],
    pub epoch: &'a str,
    pub by: &'a Owner,
    pub ids: Option<&'a [u64]>,
}

/// Why a rejection may not revert submission `r.id`'s entries, if it may not.
/// Reverting must only take back this submission's own work, so it is refused when:
/// - its entries are from another journal epoch (an earlier run, or before a `world_load`): the
///   same seqs now name other entries, or none;
/// - an entry is gone from the journal, wasn't made by the submitter, or touches an entity it
///   didn't touch when submitted (or there were too many to record);
/// - another approved submission covers one of the entries;
/// - any later journal entry not in this submission (by anyone, the submitter included) touched
///   the same entities (undoing would also wipe that change, approved or not);
/// - an entity it touched is at or under an approved submission's roots.
pub(crate) fn revert_refusal(swarm: &Swarm, world: &World, r: RevertOf) -> Option<WorldError> {
    let id = r.id;
    let refuse = |code: &'static str, why: String| {
        Some(
            WorldError::new(code, format!("{why}: reverting would undo more than submission {id}'s own work; reject without revert, and the submitter undoes what is only theirs (world_undo)"))
                .with("submission", id),
        )
    };
    let epoch = swarm.journal().epoch();
    if r.journal.is_empty() {
        return None;
    }
    if r.epoch.is_empty() || r.epoch != epoch {
        return Some(
            WorldError::new("journal_from_earlier_run", format!("submission {id}: {}; reject without revert", review::STALE_JOURNAL))
                .with("submission", id)
                .with("journal_epoch", r.epoch)
                .with("live_epoch", epoch),
        );
    }
    let Some(recorded) = r.ids else {
        return refuse("revert_unverifiable", format!("its entries touched more than {} entities, too many to record at submit", review::MAX_JOURNAL_IDS));
    };
    let recorded: BTreeSet<u64> = recorded.iter().copied().collect();
    for seq in r.journal {
        let Some(e) = swarm.journal().get(*seq) else {
            return refuse("journal_entry_gone", format!("journal entry {seq} is no longer in the journal (evicted)"));
        };
        if !(*e.by == *r.by.name && r.by.key.as_deref().is_none_or(|k| e.session.as_deref() == Some(k))) {
            return refuse("not_submitters_entry", format!("journal entry {seq} was made by {}, not the submitter {}", e.by, r.by.name));
        }
        if let Some(other) = e.ids().iter().find(|i| !recorded.contains(i)) {
            return refuse("revert_scope_changed", format!("journal entry {seq} touches entity {other}, which it didn't when submitted"));
        }
    }
    let mine: BTreeSet<u64> = r.journal.iter().copied().collect();
    let approved: Vec<&review::Submission> = swarm.review.submissions().filter(|s| s.id != id && s.status == Status::Approved).collect();
    if let Some(other) = approved.iter().find(|s| s.journal_in(epoch).iter().any(|j| mine.contains(j))) {
        return refuse("revert_overlaps_approved", format!("its journal entries overlap approved submission {}", other.id));
    }
    let touched: BTreeSet<u64> = r.journal.iter().filter_map(|s| swarm.journal().get(*s)).flat_map(|e| e.ids().iter().copied()).filter(|i| *i != 0).collect();
    let first = *r.journal.iter().min()?;
    // Every later agent entry (the journal ring is itself bounded): none may be skipped here.
    if let Some(e) = swarm.journal().entries_after(first, false).find(|e| !mine.contains(&e.seq) && e.ids().iter().any(|i| touched.contains(i))) {
        return refuse("revert_conflict", format!("journal entry {} by {} changed the same entities later", e.seq, e.by));
    }
    for s in &approved {
        for r in &s.roots {
            // Where the approved work is now (a promotion moved it).
            let root_now = if world.get(r.id).is_some() { world.path_of(r.id) } else { r.path.clone() };
            if touched.iter().any(|t| *t == r.id || (world.get(*t).is_some() && path_under(&world.path_of(*t), &root_now))) {
                return refuse("revert_overlaps_approved", format!("it touches {root_now}, approved as submission {}", s.id));
            }
        }
    }
    None
}

/// The undo that reverts a submission's journal entries for a rejection, or why it is refused
/// ([`revert_refusal`]). It runs as the decider through the write policy's undo (`Live::undo`):
/// claims apply step by step as the decider. `force` only lets the decider undo entries that
/// aren't theirs; everything force would otherwise let through (a later change to the same
/// entities, by anyone; entries of another epoch or another agent) was refused here first.
pub(crate) fn revert_request(swarm: &Swarm, world: &World, r: RevertOf) -> Result<UndoRequest, WorldError> {
    if let Some(e) = revert_refusal(swarm, world, r) {
        return Err(e);
    }
    Ok(UndoRequest { ids: Some(r.journal.to_vec()), count: r.journal.len(), force: true })
}

impl Engine {
    /// The captures folder (every run's captures are below it).
    pub(crate) fn captures_root(&self) -> &Path {
        self.capture_dir.parent().unwrap_or(&self.capture_dir)
    }

    /// Critiques carried by board tasks follow them (see `Review::sync_tasks`).
    pub(crate) fn sync_critiques(&mut self) {
        let tick = self.clock.tick();
        let board = &self.swarm.board;
        let changed = self.swarm.review.sync_tasks(tick, |t| board.get(t).map(|x| (x.status, x.owner.as_ref().map_or_else(|| x.posted_by.clone(), |o| o.name.clone()), x.result.clone())));
        if changed > 0 {
            self.review_ui.changed(&self.swarm.review);
        }
    }

    /// `(name, role)` of the connected agents that said agent_hello.
    fn roster(&self, server: &Server) -> Vec<(String, String)> {
        server.sessions().iter().filter(|s| self.swarm.introduced(&s.id)).map(|s| (self.swarm.agent_name(s), self.swarm.role(&s.id).to_owned())).collect()
    }

    pub(crate) fn handle_review(&mut self, call: ReviewCall, session: &SessionInfo, server: &Server) -> ToolOutput {
        let by = self.swarm.agent_name(session);
        let key = self.swarm.key(session);
        let w = Writer::new(&by, Some(&key));
        let named = self.swarm.introduced(&session.id);
        let authority = crate::authority::authority(&self.swarm, &session.id);
        self.sync_critiques();
        match call {
            ReviewCall::Submit(_) | ReviewCall::Decide { .. } | ReviewCall::CritiqueAdd(_) | ReviewCall::CritiqueUpdate { .. } | ReviewCall::Plan(_) if !named => {
                hello_required("submissions, decisions, critiques and plans")
            }
            ReviewCall::Submit(a) => match self.submit(w, a) {
                Ok(j) => out(j),
                Err(e) => fail(&e),
            },
            ReviewCall::List { id: Some(id), .. } => {
                match self.swarm.review.get(id) {
                    Some(s) => {
                        let mut j = s.to_json(&self.swarm.review.critiques_on(id), self.swarm.journal().epoch());
                        self.mark_missing(&mut j);
                        out(j)
                    }
                    None => fail(&WorldError::new("submission_not_found", format!("there is no submission {id} (see review_list)")).with("submission", id)),
                }
            }
            ReviewCall::List { query, detailed, .. } => {
                let mut j = self.swarm.review.list(&query, self.swarm.journal().epoch());
                self.mark_missing(&mut j);
                if detailed {
                    // Each listed submission in full, within a byte budget (the cursor resumes).
                    let ids: Vec<u64> = j.get("submissions").and_then(Json::as_array).unwrap_or_default().iter().filter_map(|s| s.get("id").and_then(Json::as_u64)).collect();
                    let mut full: Vec<Json> = Vec::with_capacity(ids.len());
                    let mut bytes = 0usize;
                    for id in &ids {
                        let Some(s) = self.swarm.review.get(*id) else { continue };
                        let item = s.to_json(&self.swarm.review.critiques_on(*id), self.swarm.journal().epoch());
                        let n = item.to_string().len();
                        if !full.is_empty() && bytes + n > MAX_DETAILED_BYTES {
                            let last = full.last().and_then(|x| x.get("id")).cloned().unwrap_or(Json::Null);
                            j.set("next_cursor", last);
                            break;
                        }
                        bytes += n;
                        full.push(item);
                    }
                    j.set("submissions", Json::Arr(full));
                }
                j.set("screen", self.review_ui.status_json(&self.swarm.review));
                out(j)
            }
            ReviewCall::Decide { which, verdict, note, revert, decisions, promote, force } => {
                let list = match self.decisions_of(which, verdict, note, revert, decisions) {
                    Ok(l) => l,
                    Err(e) => return fail(&e),
                };
                let mut j = self.decide_all(w, authority, list, promote, force);
                // Every one refused: an error (some decided: the answer lists the refusals).
                if j.get("decided").and_then(Json::as_u64) == Some(0) {
                    let code = j.get("results").and_then(Json::as_array).and_then(|r| r.first()).and_then(|r| r.get("error")).cloned();
                    let mut e = Json::obj([("error", code.unwrap_or(Json::from("refused"))), ("message", Json::from("nothing was decided: each submission's refusal is in results"))]);
                    if let Json::Obj(pairs) = &mut j {
                        if let Json::Obj(top) = &mut e {
                            top.append(pairs);
                        }
                    }
                    ToolOutput::error(e.to_string())
                } else {
                    out(j)
                }
            }
            ReviewCall::Screen(op) if op != ScreenOp::Status && crate::authority::authority(&self.swarm, &session.id) < Authority::Reviewer => fail(
                &crate::authority::require(authority, Authority::Reviewer, crate::authority::Site::ReviewScreen, "putting work on the owner's review screen (anyone may ask for its status)")
                    .expect_err("checked"),
            ),
            ReviewCall::Screen(ScreenOp::Open(_)) if self.review_ui.closed_by_human_recently() => fail(&WorldError::new(
                "owner_closed",
                "the owner closed the review screen moments ago: let them be (humans are never locked out); review_decide still works, or try again in a minute",
            )),
            ReviewCall::Screen(op) => {
                match op {
                    ScreenOp::Open(id) => {
                        if let Some(id) = id.filter(|id| self.swarm.review.get(*id).is_none()) {
                            return fail(&WorldError::new("submission_not_found", format!("there is no submission {id}")).with("submission", id));
                        }
                        self.review_ui.open(id, &self.swarm.review);
                    }
                    ScreenOp::Close => self.review_ui.close(),
                    ScreenOp::Next => self.review_ui.step(1, &self.swarm.review),
                    ScreenOp::Prev => self.review_ui.step(-1, &self.swarm.review),
                    ScreenOp::Status => {}
                }
                let mut j = self.review_ui.status_json(&self.swarm.review);
                if !self.windowed {
                    j.set("note", Json::from("headless: there is no window, but captures with include_ui still show the screen"));
                }
                out(j)
            }
            ReviewCall::CritiqueAdd(items) => {
                let tick = self.clock.tick();
                let mut specs = Vec::with_capacity(items.len());
                let mut call = crate::evidence::CallCopies::engine_thread();
                for (i, (mut spec, assign)) in items.into_iter().enumerate() {
                    if let Some(Target::Entity { id, path }) = &mut spec.target {
                        match self.world.get(*id) {
                            Some(_) => *path = self.world.path_of(*id),
                            None => return fail(&WorldError::new("not_found", format!("items[{i}]: no entity with id {id}")).with("item", i).with("id", *id)),
                        }
                    }
                    if spec.source.is_empty() {
                        spec.source = by.clone();
                    }
                    // Its evidence (any file the engine wrote) is kept beside the queue, like a submission's.
                    for field in ["capture", "packet"] {
                        let slot = if field == "capture" { &mut spec.capture } else { &mut spec.packet };
                        if let Some(given) = slot.clone() {
                            match self.keep_evidence(&given, false, &mut call) {
                                Ok(kept) => *slot = Some(kept),
                                Err(e) => return fail(&WorldError { message: format!("items[{i}].{field}: {}", e.message), ..e }.with("item", i)),
                            }
                        }
                    }
                    spec.assignee = assign;
                    specs.push(spec);
                }
                let tasks: Vec<Option<u64>> = specs.iter().map(|s| s.submission.and_then(|id| self.swarm.review.get(id)).and_then(|s| s.task)).collect();
                match self.swarm.review.add_critiques(w, authority, tick, specs) {
                    Ok(ids) => {
                        for (id, task) in ids.iter().zip(tasks) {
                            if task.is_some() {
                                self.swarm.board.log(&by, tick, "critique", task, format!("critique {id}"));
                            }
                        }
                        self.review_ui.changed(&self.swarm.review);
                        out(Json::obj([("added", Json::arr(ids.into_iter().map(Json::from))), ("review_seq", Json::from(self.swarm.review.seq()))]))
                    }
                    Err(e) => fail(&e),
                }
            }
            ReviewCall::CritiqueList(q) => {
                let mut j = self.swarm.review.list_critiques(&q);
                self.mark_missing(&mut j);
                out(j)
            }
            ReviewCall::CritiqueUpdate { ids, action, expect } => {
                let mut results = Vec::with_capacity(ids.len());
                for id in ids {
                    results.push(match self.update_critique(w, authority, id, action.clone(), expect) {
                        Ok(j) => j,
                        Err(e) => {
                            let mut j = e.to_json();
                            j.set("critique", Json::from(id));
                            j
                        }
                    });
                }
                self.review_ui.changed(&self.swarm.review);
                let refused = results.iter().filter(|r| r.get("error").is_some()).count();
                let j = Json::obj([("results", Json::Arr(results)), ("refused", Json::from(refused))]);
                if refused > 0 && j.get("results").and_then(Json::as_array).is_some_and(|r| r.len() == refused) {
                    ToolOutput::error(j.to_string())
                } else {
                    out(j)
                }
            }
            ReviewCall::Plan(a) => match self.plan(w, a, server) {
                Ok(j) => out(j),
                Err(e) => fail(&e),
            },
            ReviewCall::Progress { plan, detailed } => match self.progress(plan, detailed) {
                Ok(j) => out(j),
                Err(e) => fail(&e),
            },
        }
    }

    // ---- submissions ----

    fn submit(&mut self, w: Writer, a: SubmitArgs) -> Result<Json, WorldError> {
        let tick = self.clock.tick();
        // What it covers: subtrees, recorded with their stamps.
        let mut roots: Vec<Root> = Vec::with_capacity(a.paths.len());
        let mut total = 0usize;
        for p in &a.paths {
            let id = self.world.find_path(p).map_err(|e| e.with("field", "paths"))?;
            let e = self.world.get(id).expect("found");
            if e.transient {
                return Err(WorldError::new("transient", format!("'{p}' is transient game presentation, not work to review")).with("path", p.as_str()));
            }
            if let Some(o) = roots.iter().find(|r| path_under(p, &r.path) || path_under(&r.path, p)) {
                return Err(WorldError::new("nested_paths", format!("'{p}' and '{}' overlap: give each subtree once (the outer one covers what is below it)", o.path)).with("path", p.as_str()));
            }
            let (stamp, n) = review::stamp(&self.world, id, review::MAX_SUBTREE);
            total += n;
            if total > review::MAX_SUBTREE {
                return Err(WorldError::new("too_big", format!("the paths hold more than {} entities; submit smaller pieces (a batch label ties them together)", review::MAX_SUBTREE))
                    .with("limit", review::MAX_SUBTREE));
            }
            roots.push(Root { path: p.clone(), id, version: e.version, stamp, entities: n });
        }
        if let Some(p) = &a.promote {
            let target = self.world.find_path(&p.to).map_err(|_| {
                WorldError::new("no_target", format!("promote.to '{}' doesn't exist: create it first (world_spawn {{shape: \"group\"}})", p.to)).with("to", p.to.as_str())
            })?;
            if let Some(r) = roots.iter().find(|r| path_under(&p.to, &r.path)) {
                return Err(WorldError::new("bad_target", format!("promote.to '{}' is inside '{}', which it would move", p.to, r.path)).with("to", p.to.as_str()));
            }
            let _ = target;
        }
        // The journal entries it covers.
        // Entries another pending or approved submission already covers aren't covered twice (a
        // rejection's revert must never undo someone else's or approved work).
        let covered: BTreeSet<u64> = self
            .swarm
            .review
            .submissions()
            .filter(|s| matches!(s.status, Status::Pending | Status::Approved) && Some(s.id) != a.supersedes)
            .flat_map(|s| s.journal_in(self.swarm.journal().epoch()).iter().copied())
            .collect();
        let journal = match a.journal_since {
            Some(since) => {
                let mine: Vec<u64> = self.swarm.journal().entries_after(since, false).take(JOURNAL_SCAN).filter(|e| e.is_by(w) && !covered.contains(&e.seq)).map(|e| e.seq).collect();
                mine[mine.len().saturating_sub(review::MAX_JOURNAL)..].to_vec()
            }
            None => {
                for s in &a.journal {
                    match self.swarm.journal().get(*s) {
                        None => return Err(WorldError::new("journal_not_found", format!("journal entry {s} isn't in the journal (see world_history)")).with("journal", *s)),
                        Some(e) if !e.is_by(w) => {
                            return Err(WorldError::new("not_your_entry", format!("journal entry {s} was made by {}: a submission covers its submitter's own entries", e.by))
                                .with("journal", *s)
                                .with("by", &*e.by))
                        }
                        Some(_) if covered.contains(s) => {
                            return Err(WorldError::new("already_covered", format!("journal entry {s} is already covered by another pending or approved submission")).with("journal", *s))
                        }
                        Some(_) => {}
                    }
                }
                a.journal.clone()
            }
        };
        // The entities those entries touch now (a revert may touch only these), and their epoch.
        let mut ids: BTreeSet<u64> = BTreeSet::new();
        for s in &journal {
            ids.extend(self.swarm.journal().get(*s).map(|e| e.ids().to_vec()).unwrap_or_default());
        }
        let journal_ids = (ids.len() <= review::MAX_JOURNAL_IDS).then(|| ids.into_iter().collect::<Vec<u64>>());
        let journal_epoch = self.swarm.journal().epoch().to_owned();
        // Its evidence: captures the engine wrote, copied beside the queue (see `evidence`; the
        // connection thread copied them already): they stay as long as the queue names them. A
        // call that didn't come through the connection thread (a playtest step's) copies here, on
        // the engine thread, so at most 4 MiB.
        let mut call = crate::evidence::CallCopies::engine_thread();
        let mut captures: Vec<Capture> = Vec::with_capacity(a.captures.len());
        for (i, c) in a.captures.into_iter().enumerate() {
            let path = self.keep_evidence(&c.path, true, &mut call).map_err(|e| {
                let msg = format!("captures[{i}]: {}", e.message);
                WorldError { message: msg, ..e }.with("capture", c.path.as_str())
            })?;
            let packet = match c.packet {
                Some(p) => Some(self.keep_evidence(&p, false, &mut call).map_err(|e| {
                    let msg = format!("captures[{i}].packet: {}", e.message);
                    WorldError { message: msg, ..e }.with("packet", p.as_str())
                })?),
                None => None,
            };
            captures.push(Capture { path, packet, ..c });
        }
        let task = match a.task {
            Some(t) if self.swarm.board.get(t).is_none() => return Err(WorldError::new("task_not_found", format!("there is no task {t}")).with("task", t)),
            Some(t) => Some(t),
            None => self.swarm.board.current(w).map(|t| t.id),
        };
        let mut checks = a.checks;
        // What the submitter typed in is theirs: reported, never an engine result.
        for c in &mut checks {
            if let review::Fact::Claimed(cl) = &mut c.passed {
                cl.by = w.name.to_owned();
            }
        }
        if !roots.is_empty() {
            checks.push(Check {
                kind: "other".into(),
                name: "paths_exist".into(),
                passed: review::Fact::Engine(true),
                severity: "error".into(),
                metric: Some("entities".into()),
                value: Some(total as f64),
                limit: None,
                detail: format!("{total} entities under {} path(s)", roots.len()),
            });
        }
        checks.push(Check {
            kind: "other".into(),
            name: "captures_exist".into(),
            passed: review::Fact::Engine(!captures.is_empty()),
            severity: "warning".into(),
            metric: Some("captures".into()),
            value: Some(captures.len() as f64),
            limit: None,
            detail: if captures.is_empty() { "no captures: the reviewer has nothing to look at".into() } else { String::new() },
        });
        let spec = SubmitSpec {
            kind: a.kind,
            title: a.title,
            summary: a.summary,
            batch: a.batch,
            task,
            captures,
            checks,
            roots,
            journal,
            journal_epoch,
            journal_ids,
            promote: a.promote,
            supersedes: a.supersedes,
        };
        let epoch = self.swarm.journal().epoch().to_owned();
        let s = self.swarm.review.submit(w, tick, spec)?;
        let mut j = s.summary_json(0, &epoch);
        let (id, title, task) = (s.id, s.title.clone(), s.task);
        j.set("pending", Json::from(self.swarm.review.pending_count()));
        j.set("review_seq", Json::from(self.swarm.review.seq()));
        self.swarm.board.log(w.name, tick, "review", task, format!("submitted #{id}: {title}"));
        self.review_ui.changed(&self.swarm.review);
        Ok(j)
    }

    fn decisions_of(&self, which: Option<Which>, verdict: Option<Status>, note: String, revert: bool, decisions: Vec<DecideOne>) -> Result<Vec<DecideOne>, WorldError> {
        let Some(which) = which else { return Ok(decisions) };
        let verdict = verdict.unwrap_or(Status::Approved);
        let ids: Vec<u64> = match which {
            Which::Ids(ids) => ids,
            Which::Batch(b) => {
                let ids: Vec<u64> = self.swarm.review.pending().filter(|s| s.batch.as_deref() == Some(b.as_str())).map(|s| s.id).take(MAX_DECISIONS).collect();
                if ids.is_empty() {
                    return Err(WorldError::new("batch_empty", format!("no pending submission has batch '{b}' (see review_list's pending_batches)")).with("batch", b));
                }
                ids
            }
        };
        Ok(ids.into_iter().map(|id| DecideOne { id, verdict, note: note.clone(), revert }).collect())
    }

    /// Decides each submission (one refused doesn't stop the others). `authority`: the agent's
    /// ([`crate::authority::authority`]), or the owner's for the human at the window.
    pub(crate) fn decide_all(&mut self, w: Writer, authority: Authority, list: Vec<DecideOne>, promote: bool, force: bool) -> Json {
        self.sync_critiques();
        let mut results = Vec::with_capacity(list.len());
        let (mut ok, mut refused) = (0usize, 0usize);
        let mut seen = BTreeSet::new();
        for d in list {
            if !seen.insert(d.id) {
                continue;
            }
            let id = d.id;
            match self.decide_one(w, authority, d, promote, force) {
                Ok(j) => {
                    ok += 1;
                    results.push(j);
                }
                Err(e) => {
                    refused += 1;
                    let mut j = e.to_json();
                    j.set("submission", Json::from(id));
                    results.push(j);
                }
            }
        }
        self.review_ui.changed(&self.swarm.review);
        let more = self.swarm.review.pending_count();
        Json::obj([
            ("decided", Json::from(ok)),
            ("refused", Json::from(refused)),
            ("results", Json::Arr(results)),
            ("pending", Json::from(more)),
            ("review_seq", Json::from(self.swarm.review.seq())),
        ])
    }

    /// What changed under a submission's roots since it was submitted (empty: nothing).
    fn staleness(&self, roots: &[Root]) -> Vec<Json> {
        roots
            .iter()
            .filter_map(|r| {
                let now = self.world.get(r.id);
                let what = match now {
                    None => "deleted".to_owned(),
                    Some(_) if self.world.path_of(r.id) != r.path => format!("moved to {}", self.world.path_of(r.id)),
                    Some(e) => {
                        let (stamp, n) = review::stamp(&self.world, r.id, review::MAX_SUBTREE);
                        if stamp == r.stamp {
                            return None;
                        }
                        format!("changed (root version {} -> {}, {} -> {n} entities)", r.version, e.version, r.entities)
                    }
                };
                Some(Json::obj([("path", Json::from(r.path.as_str())), ("what", Json::from(what))]))
            })
            .collect()
    }

    fn decide_one(&mut self, w: Writer, authority: Authority, d: DecideOne, promote: bool, force: bool) -> Result<Json, WorldError> {
        let tick = self.clock.tick();
        let s = self.swarm.review.check_decide(w, authority, d.id, d.verdict)?;
        let (roots, promote_spec, journal, task, title, submitter, kind) =
            (s.roots.clone(), s.promote.clone(), s.journal.clone(), s.task, s.title.clone(), s.by.name.clone(), s.kind.clone());
        let (journal_epoch, journal_ids, by_owner) = (s.journal_epoch.clone(), s.journal_ids.clone(), s.by.clone());
        let mut decision = Decision { verdict: d.verdict, by: w.name.to_owned(), note: d.note.clone(), tick, promoted: None, task: None, reverted: None };
        let mut problems: Vec<Json> = Vec::new();
        match d.verdict {
            Status::Approved => {
                let stale = self.staleness(&roots);
                if !stale.is_empty() && !force {
                    return Err(WorldError::new(
                        "stale_submission",
                        format!("submission {} changed since it was submitted, so its captures may not show what you'd approve: ask for a resubmission, or pass force: true", d.id),
                    )
                    .with("submission", d.id)
                    .with("changed", Json::Arr(stale)));
                }
                if let (Some(p), true) = (promote_spec.as_ref(), promote) {
                    decision.promoted = Some(self.promote(w, tick, &roots, p)?);
                }
            }
            Status::Rejected => {
                if d.note.trim().is_empty() {
                    return Err(WorldError::new("note_required", "say why it is rejected ('note'): it goes to the submitter as their rework task").with("submission", d.id));
                }
                if d.revert && !journal.is_empty() {
                    // The write policy's undo, as the decider (claims apply to them).
                    let of = RevertOf { id: d.id, journal: &journal, epoch: &journal_epoch, by: &by_owner, ids: journal_ids.as_deref() };
                    let req = revert_request(&self.swarm, &self.world, of)?;
                    let u = self.world.undo(&mut self.swarm, tick, w, &req).map_err(|e| self.conflict_help(e, Some(&Yours::named(vec!["(revert)".into()]))))?;
                    self.cameras.prune(&self.world);
                    decision.reverted = Some(u.seq);
                }
                let crits: Vec<String> = self.swarm.review.critiques_on(d.id).iter().filter(|c| c.status.open()).take(12).map(|c| format!("- [{}] {} (critique {})", c.severity, cut(&c.text, 200), c.id)).collect();
                let mut detail = format!("Submission {} \"{}\" was rejected by {}:\n{}", d.id, title, w.name, d.note);
                if !crits.is_empty() {
                    detail.push_str("\n\nOpen critiques:\n");
                    detail.push_str(&crits.join("\n"));
                }
                detail.push_str(&format!("\n\nFix it, then review_submit again with supersedes: {}.", d.id));
                let spec = TaskSpec {
                    title: cut(&format!("Rework: {title}"), crate::board::MAX_TITLE),
                    detail: cut(&detail, crate::board::MAX_DETAIL),
                    deps: Vec::new(),
                    paths: roots.iter().take(crate::board::MAX_LINKS).map(|r| r.path.clone()).collect(),
                    entities: roots.iter().take(crate::board::MAX_LINKS).map(|r| r.id).collect(),
                    claims: Vec::new(),
                    tags: vec!["review".into(), "rework".into(), format!("submission:{}", d.id)],
                    assignee: Some(submitter.clone()),
                    parent: task.filter(|t| self.swarm.board.get(*t).is_some()),
                    priority: Some(7),
                };
                match self.swarm.board.post(w, tick, spec) {
                    Ok(t) => decision.task = Some(t.id),
                    Err(e) => problems.push(e.to_json()),
                }
            }
            Status::Withdrawn => {}
            Status::Pending => unreachable!("not a verdict"),
        }
        let decision_reverted = decision.reverted.is_some();
        let epoch = self.swarm.journal().epoch().to_owned();
        let s = self.swarm.review.decide(d.id, decision)?;
        let mut j = s.summary_json(0, &epoch);
        if let Some(dd) = &s.decision {
            if let Some(p) = &dd.promoted {
                j.set("promoted", p.clone());
            }
            if let Some(r) = dd.reverted {
                j.set("reverted", Json::from(r));
            }
        }
        if !problems.is_empty() {
            j.set("problems", Json::Arr(problems));
        }
        let reverted = if decision_reverted { " (its entries reverted)" } else { "" };
        let text = format!("{} #{} ({kind}){reverted}: {}", d.verdict.name(), d.id, cut(&title, 120));
        self.swarm.board.log(w.name, tick, "review", task, text);
        Ok(j)
    }

    /// Moves a submission's roots under `p.to` in one journaled batch written as `w`.
    fn promote(&mut self, w: Writer, tick: u64, roots: &[Root], p: &review::Promote) -> Result<Json, WorldError> {
        let (ops, moved) = promotion_ops(&self.world, roots, p)?;
        if ops.is_empty() {
            return Ok(Json::obj([("journal", Json::Null), ("moved", Json::arr([]))]));
        }
        let by = w.name.to_owned();
        // An agent's write through the write policy: Rule::World, as the decider.
        let apply = |wd: &mut dyn WorldWrite| wd.apply_batch(ops, &by).map_err(|(i, e)| e.with("op_index", i));
        match self.world.commit(&mut self.swarm, tick, w, Kind::Batch, Rule::World, apply) {
            Ok((_, seq)) => {
                self.cameras.prune(&self.world);
                Ok(Json::obj([("journal", Json::from(seq)), ("moved", Json::Arr(moved))]))
            }
            Err(e) => Err(self.conflict_help(e, Some(&Yours::named(vec!["(promotion)".into()])))),
        }
    }

    // ---- critiques ----

    fn update_critique(&mut self, w: Writer, authority: Authority, id: u64, action: CritiqueAction, expect: Option<u64>) -> Result<Json, WorldError> {
        let tick = self.clock.tick();
        let check = self.swarm.review.check_critique(w, authority, id, &action, expect)?;
        let c = self.swarm.review.critique(id).expect("checked");
        let mut task = None;
        if let CritiqueAction::ToTask { assign } = &action {
            let (paths, entities) = match &c.target {
                Some(Target::Entity { id, path }) => (vec![path.clone()], vec![*id]),
                Some(Target::Path(p)) => (vec![p.clone()], vec![]),
                None => (vec![], vec![]),
            };
            let mut detail = format!("Critique {id} ({}, from {}): {}", c.severity, c.source, c.text);
            if let Some(m) = &c.metric {
                detail.push_str(&format!("\nMetric {}: {} (limit {})", m.name, m.value.map_or("?".into(), |v| v.to_string()), m.limit.map_or("none".into(), |v| v.to_string())));
            }
            if let Some(cap) = &c.capture {
                detail.push_str(&format!("\nCapture: {cap}"));
            }
            detail.push_str("\nWhen this task is done the critique resolves itself with your summary.");
            let parent = c.submission.and_then(|s| self.swarm.review.get(s)).and_then(|s| s.task).filter(|t| self.swarm.board.get(*t).is_some());
            let spec = TaskSpec {
                title: cut(&format!("Critique: {}", c.text), crate::board::MAX_TITLE),
                detail: cut(&detail, crate::board::MAX_DETAIL),
                paths,
                entities,
                tags: vec!["critique".into(), format!("critique:{id}")],
                assignee: assign.clone(),
                parent,
                priority: Some(if c.severity == "error" { 7 } else { 5 }),
                ..Default::default()
            };
            task = Some(self.swarm.board.post(w, tick, spec)?.id);
        }
        let c = self.swarm.review.apply_critique(check, w.name, tick, action, task)?;
        Ok(c.summary_json())
    }

    // ---- the director ----

    fn plan(&mut self, w: Writer, a: PlanArgs, server: &Server) -> Result<Json, WorldError> {
        let tick = self.clock.tick();
        let bad = |m: String| WorldError::new("bad_plan", m);
        let (items, heading, skipped, source) = match a.source {
            PlanSource::Items(items) => (items, None, 0, "items".to_owned()),
            PlanSource::Text { text, section } => {
                let p = director::parse_section(&text, &section).map_err(bad)?;
                (p.items, Some(p.heading), p.done_skipped, "text".to_owned())
            }
            PlanSource::File { file, section } => {
                let path = PathBuf::from(&file);
                let ok = path.is_relative()
                    && path.components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir))
                    && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
                if !ok {
                    return Err(WorldError::new("bad_file", format!("'{file}' must be a relative .md path inside the project (no '..'), like docs/ROADMAP.md")).with("file", file.as_str()));
                }
                let size = std::fs::metadata(&path).map_err(|e| WorldError::new("io", format!("could not read {file}: {e}")).with("file", file.as_str()))?.len();
                if size as usize > director::MAX_DOC_BYTES {
                    return Err(WorldError::new("too_big", format!("{file} is {size} bytes; plans read at most {} bytes", director::MAX_DOC_BYTES)).with("file", file.as_str()));
                }
                let text = std::fs::read_to_string(&path).map_err(|e| WorldError::new("io", format!("could not read {file}: {e}")).with("file", file.as_str()))?;
                let p = director::parse_section(&text, &section).map_err(|m| bad(m).with("file", file.as_str()))?;
                (p.items, Some(p.heading), p.done_skipped, file)
            }
        };
        if items.is_empty() {
            return Err(bad("the section has no unchecked '- [ ]' items to plan".into()).with("done_skipped", skipped));
        }
        if let Some(p) = a.parent.filter(|p| self.swarm.board.get(*p).is_none()) {
            return Err(WorldError::new("task_not_found", format!("parent {p} is not a task on the board")).with("task", p));
        }
        let roster = self.roster(server);
        let assignees = if a.by_role { director::assign_by_role(&items, &roster) } else { vec![None; items.len()] };
        let item_json = |it: &PlanItem, who: &Option<String>, id: Option<u64>| {
            let mut f = vec![("key", Json::from(it.key.as_str())), ("title", Json::from(it.title.as_str()))];
            if let Some(id) = id {
                f.push(("task", Json::from(id)));
            }
            if let Some(r) = &it.role {
                f.push(("role", Json::from(r.as_str())));
            }
            if let Some(a) = who {
                f.push(("assignee", Json::from(a.as_str())));
            }
            if !it.after.is_empty() {
                f.push(("after", Json::arr(it.after.iter().map(|k| Json::from(k.as_str())))));
            }
            if let Some(p) = &it.parent {
                f.push(("parent", Json::from(p.as_str())));
            }
            Json::obj(f)
        };
        let roles: BTreeSet<&str> = items.iter().filter_map(|i| i.role.as_deref()).collect();
        let unstaffed: Vec<Json> = roles.iter().filter(|r| !roster.iter().any(|(_, x)| x == *r)).map(|r| Json::from(*r)).collect();
        let title = a.title.clone().unwrap_or_else(|| cut(&format!("Plan: {}", heading.as_deref().unwrap_or("items")), crate::board::MAX_TITLE));
        if a.dry_run {
            return Ok(Json::obj([
                ("dry_run", Json::Bool(true)),
                ("title", Json::from(title)),
                ("tasks", Json::arr(items.iter().zip(&assignees).map(|(it, who)| item_json(it, who, None)))),
                ("done_skipped", Json::from(skipped)),
                ("roles_nobody_has", Json::Arr(unstaffed)),
            ]));
        }
        let detail = format!(
            "Plan from {source}{}: {} task(s). Track it with task_progress {{plan}}; workers find theirs with task_list {{ready_for_me: true, tag: \"role:<role>\"}} and submit finished work with review_submit.",
            heading.as_deref().map(|h| format!(" \u{a7} {h}")).unwrap_or_default(),
            items.len()
        );
        // All or nothing: refuse up front what the board can't hold (unfinished tasks are never
        // dropped to make room; finished ones are).
        let unfinished = self.swarm.board.tasks().filter(|t| !t.status.finished()).count();
        let need: usize = items.iter().map(|i| 1024 + i.title.len() + i.detail.len()).sum::<usize>() + 2048;
        if unfinished + items.len() + 1 > crate::board::MAX_TASKS || self.swarm.board.bytes() + need > crate::board::MAX_BOARD_BYTES {
            return Err(WorldError::new("board_full", format!("the board can't take {} more tasks now: finish or cancel some first", items.len() + 1))
                .with("unfinished", unfinished)
                .with("board_bytes", self.swarm.board.bytes()));
        }
        let plan = self
            .swarm
            .board
            .post(w, tick, TaskSpec { title, detail: cut(&detail, crate::board::MAX_DETAIL), tags: vec!["plan".into()], parent: a.parent, priority: a.priority, ..Default::default() })?
            .id;
        let mut ids: BTreeMap<String, u64> = BTreeMap::new();
        let mut posted: Vec<Json> = Vec::with_capacity(items.len());
        for (it, who) in items.iter().zip(&assignees) {
            let mut tags = vec![format!("plan:{plan}")];
            if let Some(r) = &it.role {
                tags.push(format!("role:{r}"));
            }
            let spec = TaskSpec {
                title: it.title.clone(),
                detail: it.detail.clone(),
                deps: it.after.iter().filter_map(|k| ids.get(k).copied()).collect(),
                tags,
                assignee: who.clone(),
                parent: Some(it.parent.as_ref().and_then(|k| ids.get(k).copied()).unwrap_or(plan)),
                priority: a.priority,
                ..Default::default()
            };
            match self.swarm.board.post(w, tick, spec) {
                Ok(t) => {
                    ids.insert(it.key.clone(), t.id);
                    posted.push(item_json(it, who, Some(t.id)));
                }
                Err(e) => {
                    // All or nothing: what was posted is cancelled.
                    let mut left = Vec::new();
                    for id in ids.values().copied().chain([plan]) {
                        if self.swarm.board.done(w, tick, id, "plan not posted".into(), true, None).is_err() {
                            left.push(Json::from(id));
                        }
                    }
                    return Err(e.with("posted_then_cancelled", ids.len() + 1).with("not_cancelled", Json::Arr(left)));
                }
            }
        }
        Ok(Json::obj([
            ("plan", Json::from(plan)),
            ("tasks", Json::Arr(posted)),
            ("done_skipped", Json::from(skipped)),
            ("roles_nobody_has", Json::Arr(unstaffed)),
            ("board_seq", Json::from(self.swarm.board.seq())),
        ]))
    }

    /// The plan task's id that `task` belongs to (itself included), if it's under `plan`.
    fn in_plan(&self, task: u64, plan: u64) -> bool {
        let mut cur = Some(task);
        for _ in 0..MAX_PLAN_DEPTH {
            match cur {
                Some(t) if t == plan => return true,
                Some(t) => cur = self.swarm.board.get(t).and_then(|x| x.parent),
                None => return false,
            }
        }
        false
    }

    fn progress(&mut self, plan: u64, detailed: bool) -> Result<Json, WorldError> {
        let board = &self.swarm.board;
        let p = board.get(plan).ok_or_else(|| WorldError::new("task_not_found", format!("there is no task {plan} (task_plan returns the plan's id)")).with("task", plan))?;
        let tasks: Vec<&crate::board::Task> = board.tasks().filter(|t| t.id != plan && self.in_plan(t.id, plan)).collect();
        let mut counts: BTreeMap<&str, usize> = TaskStatus::ALL.iter().map(|s| (s.name(), 0)).collect();
        let mut by_role: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
        let mut blocked = Vec::new();
        let mut questions = Vec::new();
        for t in &tasks {
            *counts.entry(t.status.name()).or_default() += 1;
            let role = t.tags.iter().find_map(|g| g.strip_prefix("role:")).unwrap_or("(none)").to_owned();
            *by_role.entry(role).or_default().entry(t.status.name()).or_default() += 1;
            if t.status == TaskStatus::Blocked && blocked.len() < 20 {
                blocked.push(Json::obj([
                    ("id", Json::from(t.id)),
                    ("title", Json::from(cut(&t.title, 100))),
                    ("owner", t.owner.as_ref().map_or(Json::Null, |o| Json::from(o.name.as_str()))),
                    ("blocker", Json::from(cut(&t.blocker, 200))),
                ]));
            }
            for q in t.questions.iter().filter(|q| q.answer.is_none()) {
                if questions.len() < 20 {
                    questions.push(Json::obj([("task", Json::from(t.id)), ("question", Json::from(u64::from(q.id))), ("by", Json::from(q.by.as_str())), ("text", Json::from(cut(&q.text, 200)))]));
                }
            }
        }
        let ids: BTreeSet<u64> = tasks.iter().map(|t| t.id).chain([plan]).collect();
        let subs: Vec<&review::Submission> = self.swarm.review.submissions().filter(|s| s.task.is_some_and(|t| ids.contains(&t))).collect();
        let pending: Vec<Json> = subs
            .iter()
            .filter(|s| s.status == Status::Pending)
            .take(20)
            .map(|s| Json::obj([("id", Json::from(s.id)), ("title", Json::from(cut(&s.title, 100))), ("by", Json::from(s.by.name.as_str())), ("batch", s.batch.as_deref().map_or(Json::Null, Json::from))]))
            .collect();
        let n_pending = subs.iter().filter(|s| s.status == Status::Pending).count();
        let n = |st: Status| subs.iter().filter(|s| s.status == st).count();
        let sub_ids: BTreeSet<u64> = subs.iter().map(|s| s.id).collect();
        let open_crits = self.swarm.review.critiques().filter(|c| c.status.open() && (c.submission.is_some_and(|s| sub_ids.contains(&s)) || c.task.is_some_and(|t| ids.contains(&t)))).count();
        let total = tasks.len();
        let done = counts["done"];
        let cancelled = counts["cancelled"];
        let live = total - cancelled;
        let finished = tasks.iter().all(|t| t.status.finished());
        let complete = total > 0 && finished && n_pending == 0;
        let next = if complete {
            "complete: every task is finished and every submission decided; finish the plan task (task_done)".to_owned()
        } else if n_pending > 0 {
            format!("{n_pending} submission(s) wait for the owner or a reviewer: put them on the review screen (review_screen open) or review_decide them")
        } else if !blocked.is_empty() {
            format!("{} task(s) are blocked: answer or unblock them", blocked.len())
        } else if !questions.is_empty() {
            format!("{} open question(s): answer them (task_update {{answer}})", questions.len())
        } else {
            let open = counts["open"];
            format!("{} task(s) in progress, {open} open: workers take them with task_list {{ready_for_me: true}}", counts["taken"])
        };
        let mut j = Json::obj([
            ("plan", Json::from(plan)),
            ("title", Json::from(p.title.as_str())),
            ("status", Json::from(p.status.name())),
            ("tasks", Json::from(total)),
            ("counts", Json::obj(counts.iter().map(|(k, v)| (k.to_string(), Json::from(*v))))),
            ("percent_done", Json::from(if live == 0 { 0.0 } else { (done as f64 * 1000.0 / live as f64).round() / 10.0 })),
            ("by_role", Json::obj(by_role.into_iter().map(|(r, c)| (r, Json::obj(c.into_iter().map(|(k, v)| (k.to_owned(), Json::from(v)))))))),
            ("blocked", Json::Arr(blocked)),
            ("open_questions", Json::Arr(questions)),
            (
                "review",
                Json::obj([
                    ("pending", Json::from(n_pending)),
                    ("approved", Json::from(n(Status::Approved))),
                    ("rejected", Json::from(n(Status::Rejected))),
                    ("withdrawn", Json::from(n(Status::Withdrawn))),
                    ("waiting", Json::Arr(pending)),
                    ("open_critiques", Json::from(open_crits)),
                ]),
            ),
            ("complete", Json::Bool(complete)),
            ("next", Json::from(next)),
        ]);
        if detailed {
            j.set(
                "task_list",
                Json::arr(tasks.iter().take(200).map(|t| {
                    let mut s = t.summary_json();
                    if let Some(r) = t.tags.iter().find_map(|g| g.strip_prefix("role:")) {
                        s.set("role", Json::from(r));
                    }
                    s
                })),
            );
        }
        Ok(j)
    }

    // ---- saving with the board ----

    /// The board's and the queue's revisions together (what the board saver compares).
    pub(crate) fn board_revision(&self) -> u64 {
        self.swarm.board.revision().wrapping_add(self.swarm.review.revision())
    }

    /// The queue's text for `review.json`, or `None` when there is nothing to write there (an
    /// empty queue and no file yet).
    pub(crate) fn review_text(&mut self, dir: &Path) -> Option<String> {
        if self.board_io.review_blocked || (self.swarm.review.is_empty() && !dir.join(REVIEW_FILE).exists()) {
            return None;
        }
        Some(self.swarm.review.render_cached())
    }

    /// Reads the queue saved with the world in `dir` (an empty one when there is none). An
    /// unreadable file is moved aside and the queue starts empty; the message says so.
    pub(crate) fn load_review(&mut self, dir: &Path) -> Option<String> {
        let path = dir.join(REVIEW_FILE);
        self.review_ui.close();
        self.board_io.review_blocked = false;
        let problem = match std::fs::read_to_string(&path) {
            Ok(text) => match Review::parse(&text) {
                Ok(r) => {
                    self.swarm.review = r;
                    return None;
                }
                Err(e) => format!("not loaded: {e}"),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.swarm.review = Review::default();
                return None;
            }
            Err(e) => format!("not read: {e}"),
        };
        self.swarm.review = Review::default();
        let aside = (1..1000).map(|n| dir.join(format!("{REVIEW_FILE}.unreadable-{n}"))).find(|p| !p.exists());
        let moved = aside.filter(|to| std::fs::rename(&path, to).is_ok());
        Some(match moved {
            Some(to) => format!("review queue {} {problem}; it was moved to {} and the queue starts empty", path.display(), to.display()),
            None => {
                self.board_io.review_blocked = true;
                format!("review queue {} {problem}; it couldn't be moved aside, so the queue is not saved there until it is fixed or removed", path.display())
            }
        })
    }

    /// One window frame of the review screen: its input, and what its buttons asked for. While
    /// it is open the game, modules and live Lex runs get no window input.
    pub(crate) fn review_frame(&mut self, size: (u32, u32)) -> bool {
        let live = self.review_ui.is_open() || self.swarm.review.pending_count() > 0;
        let pads: Vec<specula_platform::gamepad::GamepadState> = if live { self.logic.pad_states(true).to_vec() } else { Vec::new() };
        let (acts, modal, pointer) = self.review_ui.frame(&self.input, &pads, size, &self.swarm.review);
        let decided = !acts.is_empty();
        for a in acts {
            let result = match a {
                crate::review_screen::Act::Approve(id) => self.window_decide(id, Status::Approved, "approved at the review screen".into()),
                crate::review_screen::Act::Reject(id, reason) => self.window_decide(id, Status::Rejected, format!("{reason} (the owner, at the review screen)")),
                crate::review_screen::Act::ApproveBatch(ids) => self.window_decide_batch(&ids),
            };
            self.review_ui.report(&result);
        }
        if modal {
            self.input.clear();
        } else if pointer {
            // A press on the tab is the screen's, not the game's.
            use specula_platform::Event;
            self.input.retain(|e| !matches!(e, Event::MouseDown { .. } | Event::MouseUp { .. } | Event::MouseWheel { .. } | Event::PointerDown { .. } | Event::PointerUp { .. }));
        }
        if decided || self.review_ui.is_open() {
            // Critiques whose tasks finished show as such.
            self.sync_critiques();
        }
        modal
    }

    /// The review screen's quads for one image, after the game's (within the GPU's limit).
    pub(crate) fn review_overlay(&mut self, size: (u32, u32), capture: bool, out: &mut Vec<specula_gpu::OverlayQuad>) {
        let room = specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(out.len());
        self.review_ui.overlay(&mut self.gpu, size, capture, &self.swarm.review, room, out);
    }

    /// The window's human decides (the review screen's buttons).
    pub(crate) fn window_decide(&mut self, id: u64, verdict: Status, note: String) -> Json {
        let w = Writer::new(WINDOW_OWNER, Some(WINDOW_OWNER));
        self.decide_all(w, Authority::Owner, vec![DecideOne { id, verdict, note, revert: false }], true, false)
    }

    /// The window's human approves a batch's pending submissions, as the screen counted them.
    pub(crate) fn window_decide_batch(&mut self, ids: &[u64]) -> Json {
        let w = Writer::new(WINDOW_OWNER, Some(WINDOW_OWNER));
        let list: Vec<DecideOne> = ids
            .iter()
            .take(MAX_DECISIONS)
            .map(|id| DecideOne { id: *id, verdict: Status::Approved, note: "approved as a batch at the review screen".into(), revert: false })
            .collect();
        self.decide_all(w, Authority::Owner, list, true, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::claims::{ClaimSpec, Scope};
    use crate::review::Promote;
    use crate::world::{Recording, Shape, SpawnSpec};
    use specula_protocol::SessionInfo;

    fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() }
    }

    fn spawn(world: &mut World, swarm: &mut Swarm, w: Writer, name: &str, parent: Option<&str>, tags: &[&str]) -> u64 {
        let mut s = SpawnSpec::new(if parent.is_some_and(|p| p.starts_with("/staging/")) { Shape::Cube } else { Shape::Group });
        s.name = Some(name.into());
        s.parent = parent.map(|p| EntityKey::Path(p.into()));
        s.tags = tags.iter().map(|t| (*t).to_owned()).collect();
        let by = w.name.to_owned();
        swarm.commit(world, 0, w, Kind::Spawn, |wd| wd.spawn(s, &by).map(|e| e.id)).unwrap().0
    }

    fn root(world: &World, path: &str) -> Root {
        let id = world.find_path(path).unwrap();
        let (stamp, entities) = review::stamp(world, id, review::MAX_SUBTREE);
        Root { path: path.into(), id, version: world.get(id).unwrap().version, stamp, entities }
    }

    /// Review finding: promotion and revert had no cargo tests (only the live script).
    #[test]
    fn promotion_is_claim_checked_journaled_and_undoable() {
        let mut world = World::default();
        world.set_recording(Recording::Full);
        let mut swarm = Swarm::default();
        let (sa, sr) = (session("aaaaaa01"), session("rrrrrr02"));
        let live = vec![sa.clone(), sr.clone()];
        swarm.hello(&sa, "ann".into(), "builder".into(), None, &live).unwrap();
        swarm.grants.grant("rev", Authority::Reviewer).unwrap();
        swarm.hello(&sr, "rev".into(), crate::authority::ROLE_REVIEWER.into(), None, &live).unwrap();
        let (ka, kr) = (swarm.key(&sa), swarm.key(&sr));
        let (ann, rev) = (Writer::new("ann", Some(&ka)), Writer::new("rev", Some(&kr)));
        spawn(&mut world, &mut swarm, rev, "staging", None, &[]);
        spawn(&mut world, &mut swarm, rev, "world", None, &[]);
        spawn(&mut world, &mut swarm, ann, "props", Some("/staging"), &["staging"]);
        spawn(&mut world, &mut swarm, ann, "pillar", Some("/staging/props"), &[]);
        let roots = vec![root(&world, "/staging/props")];
        let p = Promote { to: "/world".into(), replace: false, remove_tag: Some("staging".into()) };
        // Ann still claims her staging: the reviewer's promotion is refused, nothing moves.
        let spec = ClaimSpec { scope: Scope::path("/staging/props").unwrap(), lease: std::time::Duration::from_secs(60), note: String::new() };
        swarm.take_claim(&world, &sa, spec).unwrap();
        let (ops, moved) = promotion_ops(&world, &roots, &p).unwrap();
        assert_eq!(moved.len(), 1);
        let by = rev.name.to_owned();
        let e = swarm.commit(&mut world, 1, rev, Kind::Batch, |wd| wd.apply_batch(ops, &by).map_err(|(_, e)| e)).unwrap_err();
        assert_eq!(e.code, "claimed");
        assert!(world.find_path("/staging/props/pillar").is_ok());
        swarm.release(ann, None).unwrap();
        let (ops, _) = promotion_ops(&world, &roots, &p).unwrap();
        let (_, seq) = swarm.commit(&mut world, 2, rev, Kind::Batch, |wd| wd.apply_batch(ops, &by).map_err(|(_, e)| e)).unwrap();
        let moved = world.find_path("/world/props").unwrap();
        assert!(world.find_path("/world/props/pillar").is_ok() && !world.get(moved).unwrap().tags.contains("staging"));
        // Journaled: the reviewer can undo the move.
        swarm.undo(&mut world, 3, rev, &UndoRequest { ids: Some(vec![seq.unwrap()]), count: 1, force: false }).unwrap();
        assert!(world.find_path("/staging/props/pillar").is_ok());
        // A name taken at the target is refused unless replace.
        spawn(&mut world, &mut swarm, rev, "props", Some("/world"), &[]);
        assert_eq!(promotion_ops(&world, &roots, &p).err().unwrap().code, "name_taken");
        let (ops, _) = promotion_ops(&world, &roots, &Promote { replace: true, ..p.clone() }).unwrap();
        assert!(matches!(ops[0], Op::Delete(..)));
        assert_eq!(promotion_ops(&world, &roots, &Promote { to: "/nowhere".into(), ..p }).err().unwrap().code, "no_target");
    }

    fn owner(w: Writer) -> Owner {
        Owner { name: w.name.to_owned(), key: w.session.map(str::to_owned) }
    }

    /// Submission `id` by `by` covering `journal`, recorded now (the live epoch, what it touches).
    fn refusal(swarm: &Swarm, world: &World, id: u64, journal: &[u64], by: Writer) -> Option<WorldError> {
        let ids: BTreeSet<u64> = journal.iter().filter_map(|s| swarm.journal().get(*s)).flat_map(|e| e.ids().to_vec()).collect();
        let (ids, epoch, by) = (ids.into_iter().collect::<Vec<u64>>(), swarm.journal().epoch().to_owned(), owner(by));
        revert_refusal(swarm, world, RevertOf { id, journal, epoch: &epoch, by: &by, ids: Some(&ids) })
    }

    fn request(swarm: &Swarm, world: &World, id: u64, journal: &[u64], by: Writer) -> Result<UndoRequest, WorldError> {
        match refusal(swarm, world, id, journal, by) {
            Some(e) => Err(e),
            None => Ok(UndoRequest { ids: Some(journal.to_vec()), count: journal.len(), force: true }),
        }
    }

    /// Review finding: a rejection's revert could undo other agents' or approved work.
    #[test]
    fn reverts_never_undo_approved_or_others_work() {
        use crate::review::{Decision, SubmitSpec};
        let mut world = World::default();
        world.set_recording(Recording::Full);
        let mut swarm = Swarm::default();
        let (sa, sr) = (session("aaaaaa01"), session("rrrrrr02"));
        let live = vec![sa.clone(), sr.clone()];
        swarm.hello(&sa, "ann".into(), "builder".into(), None, &live).unwrap();
        swarm.grants.grant("rev", Authority::Reviewer).unwrap();
        swarm.hello(&sr, "rev".into(), crate::authority::ROLE_REVIEWER.into(), None, &live).unwrap();
        let (ka, kr) = (swarm.key(&sa), swarm.key(&sr));
        let (ann, rev) = (Writer::new("ann", Some(&ka)), Writer::new("rev", Some(&kr)));
        spawn(&mut world, &mut swarm, ann, "a", None, &[]);
        let seq = swarm.journal().last_seq();
        // An approved submission covers the entry: a revert of another submission covering it is refused.
        let mut r = Review::default();
        let epoch = swarm.journal().epoch().to_owned();
        let spec = |j: Vec<u64>| SubmitSpec { kind: "changes".into(), title: format!("{j:?}"), journal: j, journal_epoch: epoch.clone(), ..Default::default() };
        let first = r.submit(ann, 0, spec(vec![seq])).unwrap().id;
        r.decide(first, Decision { verdict: Status::Approved, by: "rev".into(), note: String::new(), tick: 0, promoted: None, task: None, reverted: None }).unwrap();
        let second = r.submit(ann, 0, spec(vec![seq])).unwrap().id;
        swarm.review = r;
        assert_eq!(refusal(&swarm, &world, second, &[seq], ann).unwrap().code, "revert_overlaps_approved");
        assert!(refusal(&swarm, &world, first, &[seq], ann).is_none(), "its own approval doesn't count");
        swarm.review = Review::default();
        // Review round 2: the submitter's own later change (approved as paths only) is safe too.
        spawn(&mut world, &mut swarm, ann, "gate", None, &[]);
        let gate = swarm.journal().last_seq();
        let u = UpdateSpec { position: Some(specula_core::math::Vec3::ONE), ..Default::default() };
        let by_ann = ann.name.to_owned();
        swarm.commit(&mut world, 1, ann, Kind::Set, |wd| wd.update(&EntityKey::Path("/gate".into()), u.clone(), &by_ann).map(|_| ())).unwrap();
        let e = request(&swarm, &world, 9, &[gate], ann).unwrap_err();
        assert_eq!(e.code, "revert_conflict", "ann's later move would be wiped");
        assert!(world.find_path("/gate").is_ok());
        let paths_only = SubmitSpec { roots: vec![root(&world, "/gate")], ..spec(vec![]) };
        let s3 = swarm.review.submit(ann, 0, paths_only).unwrap().id;
        swarm.review.decide(s3, Decision { verdict: Status::Approved, by: "rev".into(), note: String::new(), tick: 0, promoted: None, task: None, reverted: None }).unwrap();
        // Someone else's later change refuses it as well.
        let by = rev.name.to_owned();
        swarm.commit(&mut world, 3, rev, Kind::Set, |wd| wd.update(&EntityKey::Path("/a".into()), u, &by).map(|_| ())).unwrap();
        assert_eq!(request(&swarm, &world, 8, &[seq], ann).unwrap_err().code, "revert_conflict");
        // A clean case reverts, undone as the decider (the journal says who).
        spawn(&mut world, &mut swarm, ann, "junk", None, &[]);
        let junk = swarm.journal().last_seq();
        assert_eq!(refusal(&swarm, &world, 7, &[junk], rev).unwrap().code, "not_submitters_entry", "only the submitter's own entries");
        let req = request(&swarm, &world, 7, &[junk], ann).unwrap();
        let undo = swarm.undo(&mut world, 5, rev, &req).unwrap().seq;
        assert!(world.find_path("/junk").is_err() && swarm.journal().get(undo).is_some_and(|e| &*e.by == "rev"));
    }

    /// M4 integration audit (probe_revert.py): alice's submission was saved with the world naming
    /// journal seq 4; after a restart the journal numbered entries from 1 again, bob made seq 4,
    /// and rejecting alice's work with revert deleted bob's entity.
    #[test]
    fn reverts_refuse_journal_refs_of_another_run() {
        let (sa, sb) = (session("aaaaaa01"), session("bbbbbb02"));
        let live = vec![sa.clone(), sb.clone()];
        // The first run: alice's entry, recorded with its epoch.
        let mut world = World::default();
        world.set_recording(Recording::Full);
        let mut swarm = Swarm::default();
        swarm.hello(&sa, "alice".into(), "builder".into(), None, &live).unwrap();
        let ka = swarm.key(&sa);
        let alice = Writer::new("alice", Some(&ka));
        spawn(&mut world, &mut swarm, alice, "alice_box", None, &[]);
        let seq = swarm.journal().last_seq();
        let (old_epoch, old_ids) = (swarm.journal().epoch().to_owned(), swarm.journal().get(seq).unwrap().ids().to_vec());
        assert!(refusal(&swarm, &world, 1, &[seq], alice).is_none(), "in its own run it may revert");
        // The second run: a new journal, and bob's entry has the same seq.
        let mut world = World::default();
        world.set_recording(Recording::Full);
        let mut swarm = Swarm::default();
        swarm.hello(&sb, "bob".into(), "builder".into(), None, &live).unwrap();
        let kb = swarm.key(&sb);
        let bob = Writer::new("bob", Some(&kb));
        spawn(&mut world, &mut swarm, bob, "bob_box", None, &[]);
        assert_eq!(swarm.journal().last_seq(), seq, "the seq is reused");
        assert_ne!(swarm.journal().epoch(), old_epoch);
        let alice_owner = Owner { name: "alice".into(), key: None };
        let of = RevertOf { id: 1, journal: &[seq], epoch: &old_epoch, by: &alice_owner, ids: Some(&old_ids) };
        assert_eq!(revert_request(&swarm, &world, of).unwrap_err().code, "journal_from_earlier_run");
        // Even with the live epoch (a forged or mixed-up file), bob's entry isn't alice's to revert.
        let epoch = swarm.journal().epoch().to_owned();
        let of = RevertOf { epoch: &epoch, ..of };
        assert_eq!(revert_request(&swarm, &world, of).unwrap_err().code, "not_submitters_entry");
        // Bob's own, but touching entities other than the ones recorded at submit.
        let bob_owner = owner(bob);
        let of = RevertOf { by: &bob_owner, ids: Some(&[]), ..of };
        assert_eq!(revert_request(&swarm, &world, of).unwrap_err().code, "revert_scope_changed");
        assert_eq!(revert_request(&swarm, &world, RevertOf { ids: None, ..of }).unwrap_err().code, "revert_unverifiable");
        assert!(world.find_path("/bob_box").is_ok(), "nothing was undone");
        // An entry evicted (or never there) is refused, not skipped.
        let ids = swarm.journal().get(seq).unwrap().ids().to_vec();
        let of = RevertOf { journal: &[seq, 999], ids: Some(&ids), ..of };
        assert_eq!(revert_request(&swarm, &world, of).unwrap_err().code, "journal_entry_gone");
    }

    #[test]
    fn png_signatures_are_checked() {
        let dir = std::env::temp_dir().join(format!("specula-review-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = specula_core::png::encode_rgba8(1, 1, &[1, 2, 3, 255], specula_core::deflate::Level::Fastest);
        assert!(crate::evidence::is_png(&png) && !crate::evidence::is_png(b"not a png at all"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
