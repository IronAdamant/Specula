//! Roll-ups: the change log summarised per agent or per task, so a parent agent reads a report
//! ("agent_03: 42 changes on 40 entities under /walls, 2 refused; task 7 done, 1 open question")
//! instead of paging through `world_changes`.
//!
//! `world_rollup` scans journal entries after `since` (a journal id; at most 50,000 entries per
//! call, then it hands back `next_since`) and groups them by the agent that made them (a game
//! tool's changes count for the agent that called it) or by the task each was linked to (the
//! author's current task, see [`crate::board`]). Each group has counts (entries, changes by op,
//! undos, distinct entities), where the work happened (the deepest path all changes share, and
//! the busiest top-level areas), the tick and journal range, and the board's view: tasks done,
//! taken and blocked, open questions, refused writes, live claims, intents and leases. `summary`
//! says it in one line. Groups are sorted by name (agents) or id (tasks) and bounded.

use crate::board::{Status, Task};
use crate::journal::Entry;
use crate::swarm::Swarm;
use crate::world::Change;
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// Journal entries one call examines.
pub const MAX_SCAN: usize = 50_000;
/// Distinct entities counted per group (more are reported as this many, `entities_capped`).
const MAX_ENTITIES: usize = 100_000;
/// Groups per answer, and the answer's byte cap.
pub const MAX_GROUPS: usize = 128;
const MAX_BYTES: usize = 64 * 1024;
/// Tasks named per list in a group.
const NAMED_TASKS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    Agent,
    Task,
}

#[derive(Clone, Debug)]
pub struct RollupQuery {
    pub by: By,
    pub agent: Option<String>,
    pub task: Option<u64>,
    /// Journal id: only entries after it.
    pub since: u64,
    /// Include the hosted game's own changes (ticks and frames) as a group of their own. Game
    /// tools agents call always count, for the agent that called them.
    pub game: bool,
    pub limit: usize,
}

#[derive(Default)]
struct Group {
    entries: u64,
    changes: u64,
    ops: BTreeMap<&'static str, u64>,
    undos: u64,
    entities: BTreeSet<u64>,
    capped: bool,
    areas: BTreeMap<String, u64>,
    /// The deepest path every change shares.
    under: Option<String>,
    ticks: (u64, u64),
    seqs: (u64, u64),
}

/// The top-level segment of a path (`/district_03/house` → `/district_03`).
fn area(path: &str) -> &str {
    match path[1.min(path.len())..].find('/') {
        Some(i) => &path[..i + 1],
        None => path,
    }
}

/// The deepest path both are at or below.
fn common(a: &str, b: &str) -> String {
    if a == b {
        return a.to_owned();
    }
    let mut out = String::new();
    for (x, y) in a.split('/').zip(b.split('/')).skip(1) {
        if x != y {
            break;
        }
        out.push('/');
        out.push_str(x);
    }
    if out.is_empty() {
        "/".into()
    } else {
        out
    }
}

impl Group {
    fn add(&mut self, e: &Entry) {
        if self.entries == 0 {
            self.ticks.0 = e.tick;
            self.seqs.0 = e.seq;
        }
        self.entries += 1;
        self.ticks.1 = e.tick;
        self.seqs.1 = e.seq;
        if e.kind == crate::journal::Kind::Undo {
            self.undos += 1;
        }
        for c in &e.changes {
            self.changes += 1;
            *self.ops.entry(c.op()).or_default() += 1;
            if !matches!(c, Change::Env { .. } | Change::Camera { .. } | Change::File { .. }) {
                if self.entities.len() < MAX_ENTITIES {
                    self.entities.insert(c.id());
                } else if !self.entities.contains(&c.id()) {
                    self.capped = true;
                }
            }
            let p = c.path();
            if !p.starts_with('@') {
                *self.areas.entry(area(p).to_owned()).or_default() += 1;
                self.under = Some(match self.under.take() {
                    None => p.to_owned(),
                    Some(u) => common(&u, p),
                });
            }
        }
    }

    fn to_fields(&self, out: &mut Vec<(&'static str, Json)>) {
        out.push(("entries", Json::from(self.entries)));
        out.push(("changes", Json::from(self.changes)));
        out.push(("ops", Json::obj(self.ops.iter().map(|(k, v)| (*k, Json::from(*v))))));
        if self.undos > 0 {
            out.push(("undos", Json::from(self.undos)));
        }
        out.push(("entities", Json::from(self.entities.len())));
        if self.capped {
            out.push(("entities_capped", Json::from(true)));
        }
        if let Some(u) = &self.under {
            out.push(("under", Json::from(u.as_str())));
        }
        let mut areas: Vec<(&String, &u64)> = self.areas.iter().collect();
        areas.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        out.push((
            "areas",
            Json::arr(areas.into_iter().take(3).map(|(p, n)| Json::obj([("path", Json::from(p.as_str())), ("changes", Json::from(*n))]))),
        ));
        if self.entries > 0 {
            out.push(("ticks", Json::arr([Json::from(self.ticks.0), Json::from(self.ticks.1)])));
            out.push(("journal", Json::arr([Json::from(self.seqs.0), Json::from(self.seqs.1)])));
        }
    }

    /// "42 changes in 3 entries (30 spawn, 12 set) on 40 entities under /walls"
    fn sentence(&self) -> String {
        if self.entries == 0 {
            return "no changes".into();
        }
        let ops: Vec<String> = self.ops.iter().map(|(k, v)| format!("{v} {k}")).collect();
        let mut s = format!("{} changes in {} entries ({}) on {} entities", self.changes, self.entries, ops.join(", "), self.entities.len());
        if let Some(u) = self.under.as_deref().filter(|u| *u != "/") {
            s.push_str(&format!(" under {u}"));
        }
        if self.undos > 0 {
            s.push_str(&format!(", {} undone", self.undos));
        }
        s
    }
}

fn task_ref(t: &Task) -> Json {
    Json::obj([("id", Json::from(t.id)), ("title", Json::from(t.title.as_str()))])
}

fn refused_json(sw: &Swarm, name: &str) -> (u64, Json) {
    match sw.refused_of(name) {
        Some(m) => (m.values().sum(), Json::obj(m.iter().map(|(k, v)| (*k, Json::from(*v))))),
        None => (0, Json::obj::<&str>([])),
    }
}

/// `world_rollup`'s answer.
pub fn rollup(sw: &Swarm, q: &RollupQuery, now: Instant) -> Json {
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    let mut by_task: BTreeMap<u64, Group> = BTreeMap::new();
    let (mut scanned, mut last, mut more, mut unlinked) = (0usize, q.since, false, 0u64);
    // Game tools are agents' work and always count; the game's own ticks and frames only with `game`.
    for e in sw.journal().entries_after(q.since, true) {
        if scanned == MAX_SCAN {
            more = true;
            break;
        }
        scanned += 1;
        last = e.seq;
        if e.kind.is_game() && e.kind != crate::journal::Kind::GameTool && !q.game {
            continue;
        }
        match q.by {
            By::Agent => {
                // A game tool's changes are the calling agent's work; the game's own ticks are the game's.
                let who = e.via.as_deref().unwrap_or(&e.by);
                if q.agent.as_deref().is_none_or(|a| a == who) {
                    groups.entry(who.to_owned()).or_default().add(e);
                }
            }
            By::Task => match sw.board.task_of_entry(e.seq) {
                Some(t) if q.task.is_none_or(|x| x == t) => by_task.entry(t).or_default().add(e),
                Some(_) => {}
                None => unlinked += 1,
            },
        }
    }
    let limit = q.limit.clamp(1, MAX_GROUPS);
    let mut items = Vec::new();
    let mut bytes = 0;
    let mut cut = false;
    let mut push = |item: Json, items: &mut Vec<Json>| {
        let size = item.to_string().len();
        if items.len() >= limit || (!items.is_empty() && bytes + size > MAX_BYTES) {
            cut = true;
            return false;
        }
        bytes += size;
        items.push(item);
        true
    };
    match q.by {
        By::Agent => {
            // Agents with board work but no journal entries in range are reported too.
            if q.agent.is_none() {
                for t in sw.board.tasks().filter(|t| t.status.active()) {
                    if let Some(o) = &t.owner {
                        groups.entry(o.name.clone()).or_default();
                    }
                }
            } else if let Some(a) = &q.agent {
                groups.entry(a.clone()).or_default();
            }
            for (name, g) in &groups {
                let mut f = vec![("agent", Json::from(name.as_str()))];
                let owned = |s: Status| sw.board.tasks().filter(move |t| t.status == s && t.owner.as_ref().is_some_and(|o| &o.name == name));
                let done: Vec<&Task> = owned(Status::Done).collect();
                let taken: Vec<&Task> = owned(Status::Taken).collect();
                let blocked: Vec<&Task> = owned(Status::Blocked).collect();
                let questions: usize = sw
                    .board
                    .tasks()
                    .flat_map(|t| t.questions.iter())
                    .filter(|x| x.answer.is_none() && &x.by == name)
                    .count();
                let (refused, refused_by) = refused_json(sw, name);
                let claims = sw.claims.live(now).filter(|c| &c.owner == name).count();
                let intents = sw.intents.of(name, now).count();
                let leases = sw.leases.of(name, now);
                let mut s = format!("{name}: {}", g.sentence());
                if refused > 0 {
                    s.push_str(&format!("; {refused} refused"));
                }
                let mut tasks = Vec::new();
                for (n, label) in [(done.len(), "done"), (taken.len(), "taken"), (blocked.len(), "blocked")] {
                    if n > 0 {
                        tasks.push(format!("{n} {label}"));
                    }
                }
                if !tasks.is_empty() {
                    s.push_str(&format!("; tasks: {}", tasks.join(", ")));
                    if let Some(t) = done.last() {
                        s.push_str(&format!(" (last done: '{}')", t.title));
                    }
                }
                if questions > 0 {
                    s.push_str(&format!("; {questions} open question{}", if questions == 1 { "" } else { "s" }));
                }
                if let Some(b) = blocked.first() {
                    s.push_str(&format!("; blocked on task {}: {}", b.id, b.blocker));
                }
                f.push(("summary", Json::from(s)));
                g.to_fields(&mut f);
                f.push(("refused", Json::from(refused)));
                f.push(("refused_by_code", refused_by));
                let list = |v: &[&Task]| Json::arr(v.iter().rev().take(NAMED_TASKS).map(|t| task_ref(t)));
                f.push((
                    "tasks",
                    Json::obj([("done", list(&done)), ("taken", list(&taken)), ("blocked", list(&blocked)), ("open_questions_asked", Json::from(questions))]),
                ));
                f.push(("claims", Json::from(claims)));
                f.push(("intents", Json::from(intents)));
                f.push(("leases", Json::from(leases)));
                if !push(Json::obj(f), &mut items) {
                    break;
                }
            }
        }
        By::Task => {
            // Tasks asked for, or active and recently finished ones, even without entries in range.
            match q.task {
                Some(t) => {
                    by_task.entry(t).or_default();
                }
                None => {
                    for t in sw.board.tasks().filter(|t| t.status.active()) {
                        by_task.entry(t.id).or_default();
                    }
                }
            }
            for (id, g) in &by_task {
                let Some(t) = sw.board.get(*id) else { continue };
                let open = t.open_questions();
                let owner = t.owner.as_ref().map_or("nobody".to_owned(), |o| o.name.clone());
                let mut s = format!("task {id} '{}' ({}, {owner}): {}", t.title, t.status.name(), g.sentence());
                if t.work.refused > 0 {
                    s.push_str(&format!("; {} refused", t.work.refused));
                }
                if open > 0 {
                    s.push_str(&format!("; {open} open question{}", if open == 1 { "" } else { "s" }));
                }
                if t.status == Status::Blocked {
                    s.push_str(&format!("; blocked: {}", t.blocker));
                }
                if !t.result.is_empty() {
                    s.push_str(&format!("; result: {}", t.result));
                }
                let mut f = vec![
                    ("task", Json::from(*id)),
                    ("title", Json::from(t.title.as_str())),
                    ("status", Json::from(t.status.name())),
                    ("owner", t.owner.as_ref().map_or(Json::Null, |o| Json::from(o.name.as_str()))),
                    ("summary", Json::from(s)),
                ];
                g.to_fields(&mut f);
                f.push((
                    "work_total",
                    Json::obj([
                        ("journal_entries", Json::from(t.work.entries)),
                        ("changes", Json::from(t.work.changes)),
                        ("refused", Json::from(t.work.refused)),
                    ]),
                ));
                f.push(("open_questions", Json::from(open)));
                f.push(("notes", Json::from(sw.board.notes_on_task(*id, usize::MAX).len())));
                if !push(Json::obj(f), &mut items) {
                    break;
                }
            }
        }
    }
    let mut out = vec![
        ("by", Json::from(if q.by == By::Agent { "agent" } else { "task" })),
        ("groups", Json::Arr(items)),
        ("scanned", Json::from(scanned)),
        ("since", Json::from(q.since)),
        ("next_since", Json::from(last)),
        ("more", Json::from(more)),
        ("journal_last_id", Json::from(sw.journal().last_seq())),
        ("board_seq", Json::from(sw.board.seq())),
    ];
    if cut {
        out.push(("groups_cut", Json::from(true)));
    }
    if q.by == By::Task {
        out.push(("entries_without_task", Json::from(unlinked)));
    }
    if !sw.journal().replayable() {
        out.push(("note", Json::from("older journal entries were evicted; counts cover the entries kept")));
    }
    Json::obj(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::TaskSpec;
    use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
    use crate::journal::{Kind, Writer};
    use crate::world::{EntityKey, Recording, Shape, SpawnSpec, World};
    use specula_protocol::SessionInfo;

    fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "t".into(), client_version: "1".into(), protocol_version: "x".into() }
    }

    fn spawn(sw: &mut Swarm, w: &mut World, who: Writer, path: &str) -> Result<(), crate::world::WorldError> {
        let (parent, name) = path.rsplit_once('/').unwrap();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some(name.into());
        s.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.into()));
        sw.commit(w, 1, who, Kind::Spawn, |w| w.spawn(s, who.name).map(|_| ())).map(|_| ())
    }

    #[test]
    fn a_parent_reads_a_report_per_agent_and_per_task() {
        let (s1, s2) = (session("s1"), session("s2"));
        let live = vec![s1.clone(), s2.clone()];
        let mut sw = Swarm::default();
        let mut w = World::default();
        w.set_recording(Recording::Full);
        sw.hello(&s1, "child_1".into(), "builder".into(), None, &live).unwrap();
        sw.hello(&s2, "child_2".into(), "builder".into(), None, &live).unwrap();
        let (k1, k2) = (sw.key(&s1), sw.key(&s2));
        let (c1, c2) = (Writer::new("child_1", Some(&k1)), Writer::new("child_2", Some(&k2)));
        let walls = sw.board.post(Writer::new("lead", Some("k0")), 0, TaskSpec { title: "walls".into(), ..TaskSpec::default() }).unwrap().id;
        sw.board.take(c1, 0, walls, None).unwrap();
        spawn(&mut sw, &mut w, c1, "/walls").unwrap();
        for i in 0..3 {
            spawn(&mut sw, &mut w, c1, &format!("/walls/w{i}")).unwrap();
        }
        // child_2 claims /keep; child_1's write there is refused and counted.
        spawn(&mut sw, &mut w, c2, "/keep").unwrap();
        sw.take_claim(&w, &s2, ClaimSpec { scope: Scope::path("/keep").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
        assert!(spawn(&mut sw, &mut w, c1, "/keep/x").is_err());
        sw.board.update(c1, 2, walls, crate::board::TaskPatch { question: Some("how tall?".into()), ..Default::default() }).unwrap();
        sw.board.done(c1, 3, walls, "4 walls".into(), false, None).unwrap();

        let by_agent = rollup(&sw, &RollupQuery { by: By::Agent, agent: None, task: None, since: 0, game: false, limit: 10 }, Instant::now());
        let groups = by_agent.get("groups").unwrap().as_array().unwrap();
        assert_eq!(groups.len(), 2);
        let one = &groups[0];
        assert_eq!(one.get("agent"), Some(&Json::from("child_1")));
        assert_eq!((one.get("changes"), one.get("entities"), one.get("under")), (Some(&Json::from(4)), Some(&Json::from(4)), Some(&Json::from("/walls"))));
        assert_eq!(one.get("refused"), Some(&Json::from(1)));
        let summary = one.get("summary").unwrap().as_str().unwrap();
        assert!(summary.contains("4 changes") && summary.contains("1 refused") && summary.contains("1 done") && summary.contains("1 open question"), "{summary}");
        assert_eq!(groups[1].get("claims"), Some(&Json::from(1)));

        let by_task = rollup(&sw, &RollupQuery { by: By::Task, agent: None, task: Some(walls), since: 0, game: false, limit: 10 }, Instant::now());
        let t = &by_task.get("groups").unwrap().as_array().unwrap()[0];
        assert_eq!((t.get("task"), t.get("changes"), t.get("status")), (Some(&Json::from(walls)), Some(&Json::from(4)), Some(&Json::from("done"))));
        assert_eq!(t.get("work_total").unwrap().get("refused"), Some(&Json::from(1)));
        assert_eq!(by_task.get("entries_without_task"), Some(&Json::from(1)), "child_2's /keep");
        // A game tool's changes count for the agent that called it, without `game`.
        sw.game_tool(&mut w, 4, "sepulcra", c2, |w| {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some("tower".into());
            w.spawn(s, "sepulcra").unwrap();
        })
        .unwrap();
        let two = rollup(&sw, &RollupQuery { by: By::Agent, agent: Some("child_2".into()), task: None, since: 0, game: false, limit: 10 }, Instant::now());
        assert_eq!(two.get("groups").unwrap().as_array().unwrap()[0].get("changes"), Some(&Json::from(2)));
        // Paging by journal id.
        let later = rollup(&sw, &RollupQuery { by: By::Agent, agent: Some("child_1".into()), task: None, since: 3, game: false, limit: 10 }, Instant::now());
        assert_eq!(later.get("groups").unwrap().as_array().unwrap()[0].get("changes"), Some(&Json::from(1)));
    }

    #[test]
    fn paths_roll_up_to_their_common_area() {
        assert_eq!(area("/district_03/house/door"), "/district_03");
        assert_eq!(area("/solo"), "/solo");
        assert_eq!(common("/a/b/c", "/a/b/d"), "/a/b");
        assert_eq!(common("/a", "/b"), "/");
        assert_eq!(common("/ab", "/a"), "/");
    }
}
