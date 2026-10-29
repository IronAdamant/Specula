//! Incidents: what the watchdogs and invariant checks found, grouped and deduplicated
//! (docs/design/TRACE.md, "Watchdogs and incidents"). A debugger agent triages them
//! (docs/design/DEBUGGER.md): `trace_incidents` lists them, `trace_incident` shows one and
//! acknowledges, resolves or links it to a critique or task.
//!
//! One incident per `(code, key)` (the stall of one tool, the refusals of one rule): first and
//! last seen, a count, the related calls, agents, frames and ticks (the newest few of each), the
//! latest evidence, and a status. Its events are rate-limited ([`EVENT_EVERY_US`]); the count
//! isn't. Global, like the trace: monitors on any thread raise them, tools on connection threads
//! read them.

use specula_core::trace::{self, Level, Subsystem, Value};
use specula_core::Json;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Incidents kept (resolved ones are forgotten first, then acknowledged, then open).
pub const MAX_INCIDENTS: usize = 256;
/// Calls, agents, frames, ticks and event ids kept per incident (the newest).
const KEEP: usize = 8;
/// Notes kept per incident, and their length.
const MAX_NOTES: usize = 16;
const MAX_NOTE_CHARS: usize = 512;
/// Links kept per incident.
const MAX_LINKS: usize = 8;
/// Evidence kept per incident (compact JSON bytes; more is replaced by a note saying so).
const MAX_EVIDENCE_BYTES: usize = 4096;
/// An incident's events come at most this often (its count still counts every one).
pub const EVENT_EVERY_US: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Open,
    Acknowledged,
    Resolved,
    /// Seen again after it was resolved.
    Recurring,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Acknowledged => "acknowledged",
            Status::Resolved => "resolved",
            Status::Recurring => "recurring",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        [Status::Open, Status::Acknowledged, Status::Resolved, Status::Recurring].into_iter().find(|v| v.as_str() == s)
    }

    /// Still wants attention.
    pub fn active(self) -> bool {
        matches!(self, Status::Open | Status::Recurring)
    }
}

/// When something happened: trace time, the engine's tick and frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    pub time_us: u64,
    pub tick: u64,
    pub frame: u64,
}

impl Stamp {
    pub fn now() -> Stamp {
        let (tick, frame) = trace::now();
        Stamp { time_us: trace::now_us(), tick, frame }
    }

    fn to_json(self) -> Json {
        Json::obj([
            ("t_ms", Json::from((self.time_us / 100) as f64 / 10.0)),
            ("tick", Json::from(self.tick)),
            ("frame", Json::from(self.frame)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub by: String,
    pub action: &'static str,
    pub text: String,
    pub at: Stamp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Incident {
    pub id: u64,
    pub code: &'static str,
    pub key: String,
    pub level: Level,
    pub status: Status,
    pub summary: String,
    pub first: Stamp,
    pub last: Stamp,
    pub count: u64,
    /// Since it was last resolved (what "recurring" counts).
    pub since_resolved: u64,
    pub calls: VecDeque<u64>,
    pub agents: VecDeque<String>,
    pub frames: VecDeque<u64>,
    pub ticks: VecDeque<u64>,
    /// Trace events it emitted (ids in the ring).
    pub events: VecDeque<u64>,
    pub evidence: Json,
    pub notes: VecDeque<Note>,
    /// `(kind, id)`: `critique` or `task`.
    pub links: Vec<(String, u64)>,
    /// Who acked it first (who is looking: informational, it confers nothing).
    pub acked_by: Option<String>,
    last_event_us: u64,
    /// Events not emitted (rate limit).
    pub suppressed: u64,
}

fn push_newest<T: PartialEq>(v: &mut VecDeque<T>, x: T) {
    if v.back() == Some(&x) {
        return;
    }
    v.retain(|y| *y != x);
    v.push_back(x);
    while v.len() > KEEP {
        v.pop_front();
    }
}

impl Incident {
    pub fn summary_json(&self) -> Json {
        Json::obj([
            ("id", Json::from(self.id)),
            ("code", Json::from(self.code)),
            ("key", Json::from(self.key.as_str())),
            ("level", Json::from(self.level.as_str())),
            ("status", Json::from(self.status.as_str())),
            ("count", Json::from(self.count)),
            ("summary", Json::from(self.summary.as_str())),
            ("first_ms", Json::from((self.first.time_us / 100) as f64 / 10.0)),
            ("last_ms", Json::from((self.last.time_us / 100) as f64 / 10.0)),
        ])
    }

    pub fn detail_json(&self) -> Json {
        let mut j = self.summary_json();
        j.set("first", self.first.to_json());
        j.set("last", self.last.to_json());
        j.set("since_resolved", Json::from(self.since_resolved));
        j.set("calls", Json::arr(self.calls.iter().map(|c| Json::from(*c))));
        j.set("agents", Json::arr(self.agents.iter().map(|a| Json::from(a.as_str()))));
        j.set("frames", Json::arr(self.frames.iter().map(|f| Json::from(*f))));
        j.set("ticks", Json::arr(self.ticks.iter().map(|t| Json::from(*t))));
        j.set("events", Json::arr(self.events.iter().map(|e| Json::from(*e))));
        j.set("events_suppressed", Json::from(self.suppressed));
        j.set("evidence", self.evidence.clone());
        j.set(
            "notes",
            Json::arr(self.notes.iter().map(|n| {
                Json::obj([("by", Json::from(n.by.as_str())), ("action", Json::from(n.action)), ("note", Json::from(n.text.as_str())), ("at", n.at.to_json())])
            })),
        );
        j.set("acked_by", Json::from(self.acked_by.clone()));
        j.set("links", Json::arr(self.links.iter().map(|(k, id)| Json::obj([("kind", Json::from(k.as_str())), ("id", Json::from(*id))]))));
        j
    }
}

/// What a monitor found.
pub struct Raise<'a> {
    pub sub: &'static Subsystem,
    pub level: Level,
    pub code: &'static str,
    /// What makes two of them the same incident (the tool, the rule, the pass).
    pub key: &'a str,
    pub summary: String,
    /// The event's fields (the incident id is added).
    pub fields: Vec<(&'static str, Value)>,
    /// Numbers and paths kept on the incident (the latest replaces the last).
    pub evidence: Json,
    pub call: u64,
    pub agent: Option<String>,
}

#[derive(Default)]
struct Store {
    list: BTreeMap<u64, Incident>,
    by_key: BTreeMap<(&'static str, String), u64>,
    next: u64,
    evicted: u64,
}

static STORE: Mutex<Store> = Mutex::new(Store { list: BTreeMap::new(), by_key: BTreeMap::new(), next: 1, evicted: 0 });

fn store() -> MutexGuard<'static, Store> {
    STORE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn bounded_evidence(e: Json) -> Json {
    let n = e.to_string().len();
    if n <= MAX_EVIDENCE_BYTES {
        e
    } else {
        Json::obj([("dropped", Json::from(format!("evidence of {n} bytes was too big to keep (at most {MAX_EVIDENCE_BYTES})")))])
    }
}

/// Records what a monitor found (a new incident or one more of an existing one) and emits its
/// event unless one went out in the last second. Returns the incident's id.
pub fn raise(r: Raise) -> u64 {
    let now = Stamp::now();
    let agent = r.agent.or_else(|| crate::trace_sink::agent_of(r.call).map(|a| a.to_string()));
    let (id, emit) = {
        let mut s = store();
        let existing = s.by_key.get(&(r.code, r.key.to_owned())).copied();
        let id = match existing {
            Some(id) => id,
            None => {
                let id = s.next;
                s.next += 1;
                evict(&mut s);
                s.list.insert(
                    id,
                    Incident {
                        id,
                        code: r.code,
                        key: r.key.chars().take(128).collect(),
                        level: r.level,
                        status: Status::Open,
                        summary: String::new(),
                        first: now,
                        last: now,
                        count: 0,
                        since_resolved: 0,
                        calls: VecDeque::new(),
                        agents: VecDeque::new(),
                        frames: VecDeque::new(),
                        ticks: VecDeque::new(),
                        events: VecDeque::new(),
                        evidence: Json::Null,
                        notes: VecDeque::new(),
                        links: Vec::new(),
                        acked_by: None,
                        last_event_us: 0,
                        suppressed: 0,
                    },
                );
                s.by_key.insert((r.code, r.key.chars().take(128).collect()), id);
                id
            }
        };
        let inc = s.list.get_mut(&id).expect("just found or made");
        inc.count += 1;
        inc.since_resolved += 1;
        inc.last = now;
        inc.level = inc.level.min(r.level);
        inc.summary = r.summary.chars().take(256).collect();
        inc.evidence = bounded_evidence(r.evidence);
        if inc.status == Status::Resolved {
            inc.status = Status::Recurring;
        }
        if r.call != 0 {
            push_newest(&mut inc.calls, r.call);
        }
        if let Some(a) = agent {
            push_newest(&mut inc.agents, a);
        }
        push_newest(&mut inc.frames, now.frame);
        push_newest(&mut inc.ticks, now.tick);
        let emit = inc.last_event_us == 0 || now.time_us.saturating_sub(inc.last_event_us) >= EVENT_EVERY_US;
        if emit {
            inc.last_event_us = now.time_us;
        } else {
            inc.suppressed += 1;
        }
        (id, emit)
    };
    if emit && r.sub.enabled(r.level) {
        let mut fields = r.fields;
        fields.push(("incident", Value::U64(id)));
        let ctx = trace::Ctx { call: r.call, span: trace::Ctx::current().span };
        let _c = ctx.enter();
        r.sub.emit(r.level, r.code, fields);
        // The event's id, for the incident (it is the newest event of this thread).
        let event = trace::read(|ring| ring.iter().rev().take(64).find(|e| e.code == r.code && e.field("incident").and_then(Value::as_u64) == Some(id)).map(|e| e.id));
        if let Some(e) = event {
            if let Some(inc) = store().list.get_mut(&id) {
                push_newest(&mut inc.events, e);
            }
        }
    }
    id
}

/// Makes room for one more: resolved first, then acknowledged, then the oldest open.
fn evict(s: &mut Store) {
    while s.list.len() >= MAX_INCIDENTS {
        let victim = [Status::Resolved, Status::Acknowledged, Status::Open, Status::Recurring]
            .into_iter()
            .find_map(|st| s.list.values().filter(|i| i.status == st).min_by_key(|i| i.last.time_us).map(|i| i.id));
        let Some(v) = victim else { return };
        if let Some(i) = s.list.remove(&v) {
            s.by_key.remove(&(i.code, i.key));
        }
        s.evicted += 1;
    }
}

/// Incidents matching `status` and a `code` prefix, newest activity first: a page of `limit`
/// after skipping `offset`, and how many match.
pub fn list(status: Option<Status>, active_only: bool, code: Option<&str>, offset: usize, limit: usize) -> (Vec<Incident>, usize, Json) {
    let s = store();
    let mut all: Vec<&Incident> = s
        .list
        .values()
        .filter(|i| status.is_none_or(|st| i.status == st) && (!active_only || i.status.active()) && code.is_none_or(|c| i.code.starts_with(c)))
        .collect();
    all.sort_by(|a, b| b.last.time_us.cmp(&a.last.time_us).then(b.id.cmp(&a.id)));
    let mut counts: BTreeMap<&'static str, u64> = BTreeMap::new();
    for i in s.list.values() {
        *counts.entry(i.status.as_str()).or_default() += 1;
    }
    let totals = Json::obj(counts.into_iter().map(|(k, v)| (k, Json::from(v))).chain([("evicted", Json::from(s.evicted))]));
    let total = all.len();
    (all.into_iter().skip(offset).take(limit).cloned().collect(), total, totals)
}

pub fn get(id: u64) -> Option<Incident> {
    store().list.get(&id).cloned()
}

/// What an agent does to an incident.
pub enum Action {
    Ack,
    Resolve,
    Reopen,
    Link { kind: String, id: u64 },
    Note,
}

/// Applies `action` by `by` with `note`; the incident after, or an error code and message.
pub fn act(id: u64, by: &str, action: Action, note: Option<&str>) -> Result<Incident, (&'static str, String)> {
    let mut s = store();
    let Some(inc) = s.list.get_mut(&id) else {
        return Err(("no_incident", format!("no incident {id} (trace_incidents lists them; at most {MAX_INCIDENTS} are kept, resolved ones forgotten first)")));
    };
    let name = match &action {
        Action::Ack => "ack",
        Action::Resolve => "resolve",
        Action::Reopen => "reopen",
        Action::Link { .. } => "link",
        Action::Note => "note",
    };
    match action {
        Action::Ack => {
            inc.status = Status::Acknowledged;
            if inc.acked_by.is_none() {
                inc.acked_by = Some(by.chars().take(64).collect());
            }
        }
        Action::Resolve => {
            inc.status = Status::Resolved;
            inc.since_resolved = 0;
        }
        Action::Reopen => inc.status = Status::Open,
        Action::Link { kind, id } => {
            if !inc.links.iter().any(|(k, i)| *k == kind && *i == id) {
                if inc.links.len() >= MAX_LINKS {
                    return Err(("too_many_links", format!("incident {} already links {MAX_LINKS} items", inc.id)));
                }
                inc.links.push((kind, id));
            }
        }
        Action::Note => {}
    }
    if note.is_some() || name != "note" {
        if inc.notes.len() >= MAX_NOTES {
            inc.notes.pop_front();
        }
        inc.notes.push_back(Note { by: by.chars().take(64).collect(), action: name, text: note.unwrap_or_default().chars().take(MAX_NOTE_CHARS).collect(), at: Stamp::now() });
    }
    Ok(inc.clone())
}

/// How many incidents want attention (open or recurring), by level: for engine_info and
/// perf_frame.
pub fn active_counts() -> (u64, u64) {
    let s = store();
    let active = s.list.values().filter(|i| i.status.active());
    active.fold((0, 0), |(e, w), i| if i.level == Level::Error { (e + 1, w) } else { (e, w + 1) })
}

#[cfg(test)]
mod tests {
    use super::*;

    static SUB: Subsystem = Subsystem::new("test.incidents");

    fn raise_one(code: &'static str, key: &str, call: u64) -> u64 {
        raise(Raise {
            sub: &SUB,
            level: Level::Warn,
            code,
            key,
            summary: format!("{code} {key}"),
            fields: vec![("n", Value::U64(1))],
            evidence: Json::obj([("ms", Json::from(120.0))]),
            call,
            agent: Some("ann".into()),
        })
    }

    #[test]
    fn the_same_code_and_key_are_one_incident_with_a_count() {
        let a = raise_one("test.inc_dedupe", "sight_frame", 11);
        let b = raise_one("test.inc_dedupe", "sight_frame", 12);
        let c = raise_one("test.inc_dedupe", "world_save", 13);
        assert_eq!(a, b);
        assert_ne!(a, c);
        let i = get(a).unwrap();
        assert_eq!(i.count, 2);
        assert_eq!(i.calls, [11, 12]);
        assert_eq!(i.agents, ["ann"]);
        assert_eq!(i.status, Status::Open);
        assert_eq!(i.suppressed, 1, "the second event came within a second");
        let events = trace::read(|r| r.iter().filter(|e| e.code == "test.inc_dedupe").count());
        assert_eq!(events, 2, "one event per incident in the second");
    }

    #[test]
    fn resolving_and_seeing_it_again_makes_it_recurring() {
        let id = raise_one("test.inc_recur", "k", 0);
        let acked = act(id, "debugger", Action::Ack, Some("looking")).unwrap();
        assert_eq!(acked.status, Status::Acknowledged);
        act(id, "debugger", Action::Link { kind: "critique".into(), id: 7 }, None).unwrap();
        let resolved = act(id, "debugger", Action::Resolve, Some("fixed in 1234")).unwrap();
        assert_eq!(resolved.status, Status::Resolved);
        assert_eq!(resolved.links, [("critique".to_owned(), 7)]);
        assert_eq!(resolved.notes.len(), 3);
        raise_one("test.inc_recur", "k", 0);
        let again = get(id).unwrap();
        assert_eq!(again.status, Status::Recurring);
        assert_eq!(again.since_resolved, 1);
        assert!(act(999_999_999, "x", Action::Ack, None).is_err());
        let (page, total, _) = list(Some(Status::Recurring), false, Some("test.inc_recur"), 0, 10);
        assert_eq!((page.len(), total), (1, 1));
        assert!(again.detail_json().get("links").is_some());
    }

    #[test]
    fn the_store_is_bounded_and_forgets_resolved_first() {
        // A store of its own (the global one is shared with the other tests).
        let mut template = {
            let id = raise_one("test.inc_bound_template", "t", 0);
            get(id).unwrap()
        };
        // Newer than every numbered one (trace time may be only microseconds old in a test).
        template.last.time_us = u64::MAX / 2;
        let mut s = Store::default();
        for i in 0..MAX_INCIDENTS as u64 {
            let mut inc = template.clone();
            inc.id = i + 1;
            inc.key = format!("k{i}");
            inc.last.time_us = 1000 + i;
            // The newest one is resolved, an old one acknowledged.
            inc.status = if i == MAX_INCIDENTS as u64 - 1 { Status::Resolved } else if i == 5 { Status::Acknowledged } else { Status::Open };
            s.by_key.insert((inc.code, inc.key.clone()), inc.id);
            s.list.insert(inc.id, inc);
        }
        evict(&mut s);
        assert_eq!(s.list.len(), MAX_INCIDENTS - 1);
        assert!(!s.list.contains_key(&(MAX_INCIDENTS as u64)), "the resolved one went first, though newest");
        s.list.insert(9999, template.clone());
        evict(&mut s);
        assert!(!s.list.contains_key(&6), "then the acknowledged one");
        s.list.insert(9998, template);
        evict(&mut s);
        assert!(!s.list.contains_key(&1), "then the oldest open one");
        assert_eq!(s.evicted, 3);
        assert_eq!(s.by_key.len(), MAX_INCIDENTS - 3);
    }

    #[test]
    fn evidence_is_bounded() {
        let big = Json::from("x".repeat(10_000));
        let id = raise(Raise { sub: &SUB, level: Level::Info, code: "test.inc_big", key: "k", summary: "s".into(), fields: vec![], evidence: big, call: 0, agent: None });
        assert!(get(id).unwrap().evidence.to_string().len() < MAX_EVIDENCE_BYTES);
    }
}
