//! Quaestio (Latin "a seeking"): the example quest module. Quests as data: objectives that
//! listen to the world's events, progress kept as state, rewards that ask the other modules.
//!
//! - **Data** (`quests.json`): quests with a title and summary (`{key, text}` lines), objectives
//!   and rewards. Objective kinds: `have_item` (the actor carries `count` of an item; read from
//!   `inventarium_changed` events and the inventory when the quest starts; once met it stays
//!   met), `event` (`count` events of a name whose data fields equal `match`, credited to the
//!   actor in field `who`), and `signal` (a `quaestio_signal` with that name, from dialogue or an
//!   agent). `after` holds an objective back until others are met. Rewards: `give` (inventory),
//!   `learn` (abilities), `start` (another quest); `then` starts the next quest.
//! - **State lives in the world:** an actor's `props.quests` maps quest ids to `{format, state:
//!   active|done, progress: {objective: n}, started, finished}`. The module's state store keeps
//!   only the event names worth listening to (from the data) and who has a quest running, so an
//!   idle tick costs one state read when events are about, and nothing otherwise.
//! - **Ways in:** `quaestio_start`, `quaestio_signal`, `quaestio_abandon` events; the objective
//!   events; hooks `start`, `signal`, `abandon`, `status`.
//! - **Ways out:** `quaestio_started`, `quaestio_progress`, `quaestio_completed`,
//!   `quaestio_refused` events, reward requests, a chime, and a quest tracker text in the world.

#![deny(unsafe_code)]

use specula_sdk::{Cx, HookDef, Id, Module, ModuleInfo, Patch, SdkError, Spawn, StopReason, Value, Vec3};
use std::collections::BTreeMap;

pub const MAX_QUESTS: usize = 64;
pub const MAX_OBJECTIVES: usize = 16;
pub const MAX_REWARDS: usize = 8;
pub const MAX_MATCH_FIELDS: usize = 8;
/// Actors with a quest running at once.
pub const MAX_ACTORS: usize = 64;
/// Events looked at per tick (the engine delivers at most 1,024).
pub const MAX_EVENTS_PER_TICK: usize = 256;
pub const FORMAT: i64 = 1;

const ROOT: &str = "/quaestio";
const DATA: &str = include_str!("../quests.json");

fn bad(at: &str, what: &str) -> SdkError {
    SdkError::new("bad_data", format!("quests.json: {at} {what}"))
}

fn short(s: &str) -> String {
    s.chars().take(64).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub key: String,
    pub text: String,
}

impl Line {
    fn parse(v: Option<&Value>, at: &str) -> Result<Line, SdkError> {
        let v = v.ok_or_else(|| bad(at, "is required: {key, text}"))?;
        let key = v.get("key").and_then(Value::as_str).filter(|k| !k.is_empty() && k.len() <= 96).ok_or_else(|| bad(&format!("{at}.key"), "must be a string-table key"))?;
        let text = v.get("text").and_then(Value::as_str).filter(|t| !t.is_empty() && t.len() <= 512).ok_or_else(|| bad(&format!("{at}.text"), "must be 1..512 bytes"))?;
        Ok(Line { key: key.to_owned(), text: text.to_owned() })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    HaveItem { item: String },
    Event { event: String, who: String, matches: BTreeMap<String, Value> },
    Signal { signal: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Objective {
    pub id: String,
    pub text: Line,
    pub kind: Kind,
    pub count: u64,
    pub after: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reward {
    Give(String, u64),
    Learn(String),
    Start(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quest {
    pub title: Line,
    pub objectives: Vec<Objective>,
    pub rewards: Vec<Reward>,
    pub then: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    pub tracker: Option<(String, Vec3)>,
    pub quests: BTreeMap<String, Quest>,
}

fn name(v: Option<&Value>, at: &str) -> Result<String, SdkError> {
    v.and_then(Value::as_str).filter(|s| specula_sdk::valid_name(s, 64)).map(str::to_owned).ok_or_else(|| bad(at, "must be a name ([a-z][a-z0-9_]{0,63})"))
}

impl Data {
    pub fn parse(text: &str) -> Result<Data, SdkError> {
        let v = Value::parse_json(text)?;
        if v.get("format").and_then(Value::as_str) != Some("specula.quaestio.v1") {
            return Err(bad("format", "must be \"specula.quaestio.v1\""));
        }
        let tracker = match v.get("tracker") {
            None | Some(Value::Null) => None,
            Some(t) => {
                let actor = t.get("actor").and_then(Value::as_str).filter(|a| a.starts_with('/') && a.len() <= 256).ok_or_else(|| bad("tracker.actor", "must be an entity path"))?;
                let at = t.get("position").and_then(Value::as_vec3).filter(|p| [p.x, p.y, p.z].iter().all(|c| c.abs() <= 1.0e5)).ok_or_else(|| bad("tracker.position", "must be [x, y, z]"))?;
                Some((actor.to_owned(), at))
            }
        };
        let qs = v.get("quests").and_then(Value::as_map).filter(|q| !q.is_empty() && q.len() <= MAX_QUESTS).ok_or_else(|| bad("quests", &format!("must be an object of 1..{MAX_QUESTS} quests")))?;
        let mut quests = BTreeMap::new();
        for (qid, q) in qs {
            let at = format!("quests.{qid}");
            if !specula_sdk::valid_name(qid, 64) {
                return Err(bad(&at, "is not a name"));
            }
            Line::parse(q.get("summary"), &format!("{at}.summary"))?;
            let os = q.get("objectives").and_then(Value::as_list).filter(|o| !o.is_empty() && o.len() <= MAX_OBJECTIVES).ok_or_else(|| bad(&format!("{at}.objectives"), &format!("must be a list of 1..{MAX_OBJECTIVES}")))?;
            let mut objectives: Vec<Objective> = Vec::new();
            for (i, o) in os.iter().enumerate() {
                let at = format!("{at}.objectives[{i}]");
                let id = name(o.get("id"), &format!("{at}.id"))?;
                if objectives.iter().any(|x| x.id == id) {
                    return Err(bad(&format!("{at}.id"), "is used twice"));
                }
                let kind = match o.get("kind").and_then(Value::as_str) {
                    Some("have_item") => Kind::HaveItem { item: name(o.get("item"), &format!("{at}.item"))? },
                    Some("event") => {
                        let m = match o.get("match") {
                            None => BTreeMap::new(),
                            Some(m) => m.as_map().filter(|m| m.len() <= MAX_MATCH_FIELDS).cloned().ok_or_else(|| bad(&format!("{at}.match"), &format!("must be an object of at most {MAX_MATCH_FIELDS} fields")))?,
                        };
                        let who = match o.get("who") {
                            None => "actor".to_owned(),
                            w => name(w, &format!("{at}.who"))?,
                        };
                        Kind::Event { event: name(o.get("event"), &format!("{at}.event"))?, who, matches: m }
                    }
                    Some("signal") => Kind::Signal { signal: name(o.get("signal"), &format!("{at}.signal"))? },
                    _ => return Err(bad(&format!("{at}.kind"), "must be have_item, event or signal")),
                };
                let count = match (&kind, o.get("count")) {
                    (Kind::Signal { .. }, None) => 1,
                    (_, c) => c.and_then(Value::as_u64).filter(|c| (1..=1_000_000).contains(c)).ok_or_else(|| bad(&format!("{at}.count"), "must be 1..1000000"))?,
                };
                let after = match o.get("after") {
                    None => Vec::new(),
                    Some(a) => a
                        .as_list()
                        .ok_or_else(|| bad(&format!("{at}.after"), "must be a list of objective ids"))?
                        .iter()
                        .map(|x| x.as_str().filter(|x| objectives.iter().any(|o| o.id == *x)).map(str::to_owned).ok_or_else(|| bad(&format!("{at}.after"), "names no earlier objective")))
                        .collect::<Result<_, _>>()?,
                };
                objectives.push(Objective { id, text: Line::parse(o.get("text"), &format!("{at}.text"))?, kind, count, after });
            }
            let rs = match q.get("rewards") {
                None => &[][..],
                Some(r) => r.as_list().filter(|r| r.len() <= MAX_REWARDS).ok_or_else(|| bad(&format!("{at}.rewards"), &format!("must be a list of at most {MAX_REWARDS}")))?,
            };
            let rewards = rs
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let at = format!("{at}.rewards[{i}]");
                    if let Some(g) = r.get("give") {
                        match g.as_list() {
                            Some([it, n]) => Ok(Reward::Give(name(Some(it), &format!("{at}.give[0]"))?, n.as_u64().filter(|n| (1..=1_000_000).contains(n)).ok_or_else(|| bad(&format!("{at}.give[1]"), "must be 1..1000000"))?)),
                            _ => Err(bad(&format!("{at}.give"), "must be [item, count]")),
                        }
                    } else if r.get("learn").is_some() {
                        Ok(Reward::Learn(name(r.get("learn"), &format!("{at}.learn"))?))
                    } else if r.get("start").is_some() {
                        Ok(Reward::Start(name(r.get("start"), &format!("{at}.start"))?))
                    } else {
                        Err(bad(&at, "must be {give}, {learn} or {start}"))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let then = match q.get("then") {
                None | Some(Value::Null) => None,
                t => Some(name(t, &format!("{at}.then"))?),
            };
            quests.insert(qid.clone(), Quest { title: Line::parse(q.get("title"), &format!("{at}.title"))?, objectives, rewards, then });
        }
        for (qid, q) in &quests {
            let missing = |n: &String| !quests.contains_key(n);
            if q.then.as_ref().is_some_and(missing) || q.rewards.iter().any(|r| matches!(r, Reward::Start(n) if missing(n))) {
                return Err(bad(&format!("quests.{qid}"), "starts a quest that isn't defined"));
            }
        }
        Ok(Data { tracker, quests })
    }

    /// The event names objectives listen to (plus the module's own requests).
    pub fn watched(&self) -> Vec<String> {
        let mut names: Vec<String> = ["quaestio_start", "quaestio_signal", "quaestio_abandon", "inventarium_changed"].iter().map(|s| (*s).to_owned()).collect();
        for q in self.quests.values() {
            for o in &q.objectives {
                if let Kind::Event { event, .. } = &o.kind {
                    if !names.contains(event) {
                        names.push(event.clone());
                    }
                }
            }
        }
        names
    }

    fn quest(&self, id: &str) -> Result<&Quest, SdkError> {
        self.quests.get(id).ok_or_else(|| {
            let known: Vec<&str> = self.quests.keys().map(String::as_str).take(24).collect();
            SdkError::new("unknown_quest", format!("no quest '{}' (quests: {})", short(id), known.join(", ")))
        })
    }
}

/// One actor's record of one quest.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Record {
    pub done: bool,
    pub progress: BTreeMap<String, u64>,
    pub started: u64,
    pub finished: Option<u64>,
}

impl Record {
    pub fn from_value(v: &Value) -> Result<Record, SdkError> {
        let bad = |w: &str| SdkError::new("bad_quest_record", format!("props.quests: {w}"));
        if v.get("format").and_then(Value::as_i64) != Some(FORMAT) {
            return Err(bad(&format!("format must be {FORMAT}")));
        }
        let done = match v.get("state").and_then(Value::as_str) {
            Some("active") => false,
            Some("done") => true,
            _ => return Err(bad("state must be active or done")),
        };
        let mut progress = BTreeMap::new();
        for (k, n) in v.get("progress").and_then(Value::as_map).into_iter().flatten().take(MAX_OBJECTIVES) {
            progress.insert(k.clone(), n.as_u64().ok_or_else(|| bad("progress values must be whole numbers"))?);
        }
        Ok(Record { done, progress, started: v.get("started").and_then(Value::as_u64).unwrap_or(0), finished: v.get("finished").and_then(Value::as_u64) })
    }

    pub fn to_value(&self) -> Value {
        let mut p = Value::map();
        for (k, n) in &self.progress {
            p.insert(k, *n);
        }
        Value::map().with("format", FORMAT).with("state", if self.done { "done" } else { "active" }).with("progress", p).with("started", self.started).with("finished", self.finished)
    }

    pub fn met(&self, o: &Objective) -> bool {
        self.progress.get(&o.id).copied().unwrap_or(0) >= o.count
    }

    /// Whether `o` may progress now (the objectives it waits for are met).
    pub fn open(&self, q: &Quest, o: &Objective) -> bool {
        o.after.iter().all(|a| q.objectives.iter().find(|x| x.id == *a).is_some_and(|x| self.met(x)))
    }

    pub fn complete(&self, q: &Quest) -> bool {
        q.objectives.iter().all(|o| self.met(o))
    }
}

/// Whether event data `d` matches every field of `m`.
/// Whether notice `e` came from the module its name starts with (`facultas_defeated` from
/// `facultas`).
pub fn from_owner(e: &specula_sdk::Event) -> bool {
    e.name.split_once('_').is_some_and(|(module, _)| module == e.from)
}

pub fn matches(d: &Value, m: &BTreeMap<String, Value>) -> bool {
    m.iter().all(|(k, v)| d.get(k) == Some(v))
}

#[derive(Default)]
pub struct Quaestio;

#[cfg(not(feature = "linked"))]
specula_sdk::export_module!(Quaestio);

const ACTOR_QUEST: &str = r#"{"type": "object", "required": ["actor", "quest"], "properties": {"actor": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}, "quest": {"type": "string", "maxLength": 64}}}"#;

impl Module for Quaestio {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo {
            name: "quaestio",
            version: env!("CARGO_PKG_VERSION"),
            description: "Quests as data: objectives that count items, events and signals; progress in props.quests; rewards through the other modules.",
            order: 40,
            actions: "",
            hooks: vec![
                HookDef { name: "start", doc: "Starts quest for actor (entity path or id). Refused if it is running or done.", args: ACTOR_QUEST },
                HookDef {
                    name: "signal",
                    doc: "Sends a signal to actor's running quests (signal objectives with that name progress).",
                    args: r#"{"type": "object", "required": ["actor", "signal"], "properties": {"actor": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}, "signal": {"type": "string", "maxLength": 64}}}"#,
                },
                HookDef { name: "abandon", doc: "Drops a running quest from actor (it can be started again).", args: ACTOR_QUEST },
                HookDef {
                    name: "status",
                    doc: "Reads actor's quests: state, each objective's progress and target, titles as {key, text} (changes nothing).",
                    args: r#"{"type": "object", "required": ["actor"], "properties": {"actor": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}}}"#,
                },
            ],
        }
    }

    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        let data = Data::parse(DATA)?;
        if cx.find(ROOT)?.is_none() {
            cx.spawn(Spawn::new("group").name("quaestio"))?;
        }
        let watched: Vec<Value> = data.watched().iter().map(|s| Value::from(s.as_str())).collect();
        cx.set_state("watch", watched)
    }

    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        if cx.events().is_empty() {
            return Ok(());
        }
        let watch = cx.state("watch")?;
        let watch: Vec<&str> = watch.as_list().unwrap_or_default().iter().filter_map(Value::as_str).collect();
        let events: Vec<_> = cx.events().iter().filter(|e| watch.contains(&e.name.as_str())).take(MAX_EVENTS_PER_TICK).cloned().collect();
        if events.is_empty() {
            return Ok(());
        }
        let data = Data::parse(DATA)?;
        let mut touched: Vec<Id> = Vec::new();
        for e in &events {
            // Notices (what happened) count only from the module whose name they start with:
            // any module may emit any event, so `facultas_defeated` from elsewhere is a forgery.
            // Requests (quaestio_*) are open to every module by design.
            if !e.name.starts_with("quaestio_") && !from_owner(e) {
                cx.emit("quaestio_refused", Value::map().with("event", e.name.as_str()).with("code", "forged").with("from", short(&e.from)))?;
                continue;
            }
            let r = match e.name.as_str() {
                "quaestio_start" => actor(cx, e.data.get("actor")).and_then(|a| {
                    touched.push(a);
                    start(cx, &data, a, e.data.get("quest").and_then(Value::as_str).unwrap_or_default())
                }),
                "quaestio_signal" => actor(cx, e.data.get("actor")).and_then(|a| {
                    touched.push(a);
                    signal(cx, &data, a, e.data.get("signal").and_then(Value::as_str).unwrap_or_default())
                }),
                "quaestio_abandon" => actor(cx, e.data.get("actor")).and_then(|a| {
                    touched.push(a);
                    abandon(cx, a, e.data.get("quest").and_then(Value::as_str).unwrap_or_default())
                }),
                "inventarium_changed" => {
                    let Some(owner) = e.data.get("owner").and_then(Value::as_u64) else { continue };
                    let item = e.data.get("item").and_then(Value::as_str).unwrap_or_default().to_owned();
                    let count = e.data.get("count").and_then(Value::as_u64).unwrap_or(0);
                    touched.push(Id(owner));
                    progress(cx, &data, Id(owner), |_, o| matches!(&o.kind, Kind::HaveItem { item: i } if *i == item).then_some(Step::AtLeast(count)))
                }
                name => {
                    // An objective event: credited to the actor its `who` field names.
                    let mut r = Ok(());
                    for (qid, q) in &data.quests {
                        for o in &q.objectives {
                            let Kind::Event { event, who, matches: m } = &o.kind else { continue };
                            if event != name || !matches(&e.data, m) {
                                continue;
                            }
                            let Some(a) = e.data.get(who).and_then(Value::as_u64) else { continue };
                            touched.push(Id(a));
                            let (qid, oid) = (qid.clone(), o.id.clone());
                            r = r.and(progress(cx, &data, Id(a), |q2, o2| (q2 == qid && o2.id == oid).then_some(Step::Add(1))));
                        }
                    }
                    r
                }
            };
            if let Err(err) = r {
                if err.code != "not_found" || e.name.starts_with("quaestio_") {
                    cx.emit("quaestio_refused", Value::map().with("event", e.name.as_str()).with("code", err.code.as_str()).with("message", short(&err.message)))?;
                }
            }
        }
        if let Some((path, _)) = &data.tracker {
            if let Some(t) = cx.find(path)? {
                if touched.contains(&t) {
                    track(cx, &data)?;
                }
            }
        }
        Ok(())
    }

    fn hook(&self, cx: &mut Cx, name: &str, args: &Value) -> Result<Value, SdkError> {
        let data = Data::parse(DATA)?;
        let a = actor(cx, args.get("actor"))?;
        let s = |k: &str| args.get(k).and_then(Value::as_str).filter(|x| x.len() <= 64).ok_or_else(|| SdkError::new("invalid_field", format!("'{k}' must be a name")));
        match name {
            "start" => start(cx, &data, a, s("quest")?)?,
            "signal" => signal(cx, &data, a, s("signal")?)?,
            "abandon" => abandon(cx, a, s("quest")?)?,
            "status" => {}
            other => return Err(SdkError::new("unknown_hook", format!("quaestio has no hook '{}' (it has: start, signal, abandon, status)", short(other)))),
        }
        if name != "status" {
            track(cx, &data)?;
        }
        status(cx, &data, a)
    }

    fn stop(&self, cx: &mut Cx, reason: StopReason) -> Result<(), SdkError> {
        if reason == StopReason::Unload {
            if let Some(root) = cx.find(ROOT)? {
                cx.delete(root)?;
            }
        }
        Ok(())
    }
}

fn actor(cx: &mut Cx, v: Option<&Value>) -> Result<Id, SdkError> {
    match v {
        Some(Value::Str(p)) if p.len() <= 256 => cx.find(p)?.ok_or_else(|| SdkError::new("not_found", format!("no entity at '{}'", short(p)))),
        Some(Value::Int(i)) if *i >= 0 => {
            let id = Id(*i as u64);
            cx.entity(id)?.map(|_| id).ok_or_else(|| SdkError::new("not_found", format!("no entity {i}")))
        }
        _ => Err(SdkError::new("invalid_field", "'actor' must be an entity path or id")),
    }
}

fn records(cx: &mut Cx, a: Id) -> Result<(specula_sdk::Entity, BTreeMap<String, Record>), SdkError> {
    let e = cx.entity(a)?.ok_or_else(|| SdkError::new("not_found", format!("no entity {}", a.0)))?;
    let mut out = BTreeMap::new();
    for (k, v) in e.prop("quests").as_map().into_iter().flatten().take(MAX_QUESTS) {
        out.insert(k.clone(), Record::from_value(v)?);
    }
    Ok((e, out))
}

/// Writes one quest's record (`None` removes it) and keeps the actor's other quests: `props`
/// merge one level deep only, so the whole `quests` map is written back.
fn write(cx: &mut Cx, a: Id, qid: &str, r: Option<&Record>) -> Result<(), SdkError> {
    let (_, mut recs) = records(cx, a)?;
    match r {
        Some(r) => {
            if !recs.contains_key(qid) && recs.len() >= MAX_QUESTS {
                return Err(SdkError::new("limit", format!("an actor holds at most {MAX_QUESTS} quests")));
            }
            recs.insert(qid.to_owned(), r.clone());
        }
        None => {
            recs.remove(qid);
        }
    }
    let mut all = Value::map();
    for (k, r) in &recs {
        all.insert(k, r.to_value());
    }
    cx.set(a, Patch::new().props(Value::map().with("quests", all))).map(drop)
}

fn carried(e: &specula_sdk::Entity, item: &str) -> u64 {
    e.prop("inventory").get("slots").and_then(Value::as_list).unwrap_or_default().iter().filter(|s| s.get("item").and_then(Value::as_str) == Some(item)).filter_map(|s| s.get("count").and_then(Value::as_u64)).sum()
}

fn start(cx: &mut Cx, data: &Data, a: Id, qid: &str) -> Result<(), SdkError> {
    let q = data.quest(qid)?;
    let (e, recs) = records(cx, a)?;
    if let Some(r) = recs.get(qid) {
        return Err(SdkError::new("already", format!("the actor's quest '{qid}' is already {}", if r.done { "done" } else { "running" })));
    }
    let mut r = Record { started: cx.tick(), ..Record::default() };
    for o in &q.objectives {
        let n = match &o.kind {
            Kind::HaveItem { item } if r.open(q, o) => carried(&e, item).min(o.count),
            _ => 0,
        };
        r.progress.insert(o.id.clone(), n);
    }
    write(cx, a, qid, Some(&r))?;
    let mut active: Vec<Value> = cx.state("actors")?.as_list().unwrap_or_default().to_vec();
    if !active.iter().any(|x| x.as_u64() == Some(a.0)) && active.len() < MAX_ACTORS {
        active.push(Value::from(a.0));
        cx.set_state("actors", active)?;
    }
    cx.emit("quaestio_started", Value::map().with("actor", a.0).with("quest", qid))?;
    cx.sound("quest_start", None, 0.9)?;
    finish_if_complete(cx, data, a, qid, r)
}

fn signal(cx: &mut Cx, data: &Data, a: Id, s: &str) -> Result<(), SdkError> {
    progress(cx, data, a, |_, o| matches!(&o.kind, Kind::Signal { signal } if signal == s).then_some(Step::Add(1)))
}

fn abandon(cx: &mut Cx, a: Id, qid: &str) -> Result<(), SdkError> {
    let (_, recs) = records(cx, a)?;
    match recs.get(qid) {
        Some(r) if !r.done => write(cx, a, qid, None),
        Some(_) => Err(SdkError::new("already", "a finished quest can't be abandoned")),
        None => Err(SdkError::new("not_running", format!("the actor has no quest '{}'", short(qid)))),
    }
}

enum Step {
    Add(u64),
    AtLeast(u64),
}

/// Moves every open objective `pick` chooses on each of the actor's running quests.
fn progress(cx: &mut Cx, data: &Data, a: Id, pick: impl Fn(&str, &Objective) -> Option<Step>) -> Result<(), SdkError> {
    let (_, recs) = records(cx, a)?;
    for (qid, mut r) in recs {
        if r.done {
            continue;
        }
        let Some(q) = data.quests.get(&qid) else { continue };
        let mut changed = false;
        for o in &q.objectives {
            if r.met(o) || !r.open(q, o) {
                continue;
            }
            let Some(step) = pick(&qid, o) else { continue };
            let was = r.progress.get(&o.id).copied().unwrap_or(0);
            let now = match step {
                Step::Add(n) => was.saturating_add(n),
                // Items held: latched upward (a met objective stays met when items are spent).
                Step::AtLeast(n) => was.max(n),
            }
            .min(o.count);
            if now != was {
                r.progress.insert(o.id.clone(), now);
                changed = true;
                cx.emit("quaestio_progress", Value::map().with("actor", a.0).with("quest", qid.as_str()).with("objective", o.id.as_str()).with("value", now).with("target", o.count))?;
            }
        }
        if changed {
            write(cx, a, &qid, Some(&r))?;
            finish_if_complete(cx, data, a, &qid, r)?;
        }
    }
    Ok(())
}

fn finish_if_complete(cx: &mut Cx, data: &Data, a: Id, qid: &str, mut r: Record) -> Result<(), SdkError> {
    let q = data.quest(qid)?;
    if r.done || !r.complete(q) {
        return Ok(());
    }
    r.done = true;
    r.finished = Some(cx.tick());
    write(cx, a, qid, Some(&r))?;
    for reward in &q.rewards {
        match reward {
            Reward::Give(item, n) => cx.emit("inventarium_give", Value::map().with("owner", a.0).with("item", item.as_str()).with("count", *n).with("partial", true))?,
            Reward::Learn(ab) => cx.emit("facultas_learn", Value::map().with("caster", a.0).with("ability", ab.as_str()))?,
            Reward::Start(next) => cx.emit("quaestio_start", Value::map().with("actor", a.0).with("quest", next.as_str()))?,
        }
    }
    if let Some(next) = &q.then {
        cx.emit("quaestio_start", Value::map().with("actor", a.0).with("quest", next.as_str()))?;
    }
    cx.sound("quest_done", None, 1.0)?;
    cx.emit("quaestio_completed", Value::map().with("actor", a.0).with("quest", qid))
}

fn status(cx: &mut Cx, data: &Data, a: Id) -> Result<Value, SdkError> {
    let (_, recs) = records(cx, a)?;
    let mut out = Value::map();
    for (qid, r) in &recs {
        let Some(q) = data.quests.get(qid) else { continue };
        let objectives: Vec<Value> = q
            .objectives
            .iter()
            .map(|o| {
                Value::map()
                    .with("id", o.id.as_str())
                    .with("text", Value::map().with("key", o.text.key.as_str()).with("text", o.text.text.as_str()))
                    .with("value", r.progress.get(&o.id).copied().unwrap_or(0))
                    .with("target", o.count)
                    .with("open", r.open(q, o))
            })
            .collect();
        out.insert(
            qid,
            Value::map()
                .with("state", if r.done { "done" } else { "active" })
                .with("title", Value::map().with("key", q.title.key.as_str()).with("text", q.title.text.as_str()))
                .with("objectives", objectives),
        );
    }
    Ok(Value::map().with("quests", out))
}

/// The tracker text: the tracked actor's running quests, each objective with its count.
fn track(cx: &mut Cx, data: &Data) -> Result<(), SdkError> {
    let Some((path, at)) = &data.tracker else { return Ok(()) };
    let Some(a) = cx.find(path)? else { return Ok(()) };
    let (_, recs) = records(cx, a)?;
    let mut lines = Vec::new();
    let mut keys = Vec::new();
    for (qid, r) in &recs {
        let Some(q) = data.quests.get(qid) else { continue };
        if r.done {
            lines.push(format!("{} (done)", q.title.text));
            keys.push(Value::from(q.title.key.as_str()));
            continue;
        }
        lines.push(q.title.text.clone());
        keys.push(Value::from(q.title.key.as_str()));
        for o in q.objectives.iter().filter(|o| r.open(q, o)) {
            let v = r.progress.get(&o.id).copied().unwrap_or(0);
            lines.push(if o.count > 1 { format!("  {} {v}/{}", o.text.text, o.count) } else { format!("  {}{}", o.text.text, if v >= o.count { " (done)" } else { "" }) });
            keys.push(Value::from(o.text.key.as_str()));
        }
    }
    let root = match cx.find(ROOT)? {
        Some(r) => r,
        None => cx.spawn(Spawn::new("group").name("quaestio"))?,
    };
    let text = Value::map().with("text", lines.join("\n")).with("size", 0.16).with("max_width", 4.0).with("align", "left").with("anchor", [0.0f32, 1.0]);
    match cx.find(&format!("{ROOT}/tracker"))? {
        Some(t) if lines.is_empty() => {
            cx.delete(t)?;
        }
        Some(t) => {
            cx.set(t, Patch::new().field("text", text).props(Value::map().with("keys", keys)))?;
        }
        None if lines.is_empty() => {}
        None => {
            cx.spawn(Spawn::new("text").name("tracker").field("parent", root.0).position(*at).color("#e8e4d8").tags(&["quaestio_text"]).props(Value::map().with("keys", keys)).field("text", text))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
