//! The desync detector: when two runs that should agree (same seed, same commands; a replay and
//! its original) stop agreeing, say **where first**: the tick, and the component that differs.
//!
//! **What is recorded.** As every tick begins the engine records the world's state hash, its id
//! counter and the hosted game's own subsystem hashes ([`crate::game::Game::state_hashes`]: a
//! game's simulation state usually lives outside the world). The last [`MAX_TICKS`] ticks are
//! kept. The journal says which entities changed going into each tick, with their full state.
//!
//! **Traces.** `desync_trace` writes a run's trace (format `specula.trace/v1`) to a file: per
//! tick the world hash and the subsystem hashes, and per tick the entities that changed going
//! into it (id, path, content hash, and a 16-bit digest of each field, so a difference can be
//! narrowed to fields without storing whole states). Entity lists cover the ticks the journal
//! still holds (`entities_from_tick`).
//!
//! **Comparing.** `desync_compare {a, b}` takes two traces (files, or `live` for this engine's
//! own) and walks their common ticks: the first tick where the world hash or any subsystem hash
//! differs is the divergence. It reports which subsystems differ and, when the world differs,
//! which entities changed differently going into that tick (one run changed it and the other
//! didn't, or both did with different results) and which of their fields differ. `b: "replay"`
//! checks this engine against its own journal instead: the journal is replayed from its base tick
//! by tick and compared with the recorded hashes, and entities whose live state differs from the
//! replay are named (a change that bypassed the journal shows up here).
//!
//! Tick `T`'s hash is taken as tick `T` begins, after everything stamped with an earlier tick
//! (the game's tick `T-1` and agents' edits made while the clock read `T-1`), so the entities
//! listed under tick `T` are exactly what made hash `T` differ from hash `T-1`.
//!
//! **Module code is part of the input** (DETERMINISM.md rule 8). The log also keeps every module
//! load, reload, unload and disable with its build hash (and, once older events are dropped, the
//! builds they left running), and traces carry them (`modules`). A comparison works out which
//! build of each module ran in each run at the divergent tick and names any module whose build,
//! or the tick it was loaded at, differs (`modules`, and `module <name>: ...` first in
//! `components`): the likely cause. Runs that match although their builds differ say so too
//! (`module_builds_differ`): a mismatch is reported, not treated as a failure.

use crate::journal::{hex, Journal, ModuleEvent};
use crate::world::{Change, Entity, World, CAMERA_ID_BASE};
use specula_core::hash::hash_str;
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

pub const FORMAT: &str = "specula.trace/v1";
/// Ticks of hashes kept (about 2 minutes 16 s at 30 Hz).
pub const MAX_TICKS: usize = 4096;
/// Entities and subsystems named in a comparison.
const MAX_NAMED: usize = 20;
/// Trace files read at most this big.
pub const MAX_TRACE_BYTES: u64 = 256 << 20;
/// Entity hashes one `desync_compare {b: "replay"}` call may spend on per-tick world hashes
/// (about 40 ms): past it the answer hands back `next_from_tick` to continue.
pub const REPLAY_BUDGET: usize = 1_500_000;
/// Module events a tick log keeps (older ones fold into its base).
pub const MAX_MODULE_EVENTS: usize = 256;
/// Entity fields left out of digests (bookkeeping, or derived from the parent).
const IGNORED: [&str; 6] = ["id", "version", "created_by", "modified_by", "model_state", "world_position"];

/// One tick's hashes.
#[derive(Clone, Debug, PartialEq)]
pub struct TickRecord {
    pub tick: u64,
    pub world: u64,
    pub next_id: u64,
    pub subsystems: Vec<(&'static str, u64)>,
}

/// A module's code changing, as traces record it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleMark {
    pub tick: u64,
    pub module: String,
    /// `load`, `reload`, `unload`, `disabled`.
    pub event: String,
    /// The build running afterwards (`None`: unloaded or disabled).
    pub build: Option<u64>,
}

/// Which module builds a run had: those running before its first kept event, and the events.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleHistory {
    /// Module → (build, the tick it was loaded at), before `events`.
    pub base: BTreeMap<String, (u64, u64)>,
    /// Oldest first.
    pub events: Vec<ModuleMark>,
}

impl ModuleHistory {
    /// Module → (build, loaded at) for the code that ran in tick `t - 1`, which hash `t` shows (an
    /// event while the clock read `T` affects tick `T` first).
    pub fn builds_at(&self, t: u64) -> BTreeMap<String, (u64, u64)> {
        let mut m = self.base.clone();
        for e in self.events.iter().filter(|e| e.tick < t) {
            match e.build {
                Some(b) => {
                    m.insert(e.module.clone(), (b, e.tick));
                }
                None => {
                    m.remove(&e.module);
                }
            }
        }
        m
    }

    fn to_json(&self) -> Json {
        Json::obj([
            ("base", Json::obj(self.base.iter().map(|(n, (b, t))| (n.as_str(), Json::arr([Json::from(hex(*b)), Json::from(*t)]))))),
            (
                "events",
                Json::arr(self.events.iter().map(|e| {
                    Json::arr([Json::from(e.tick), Json::from(e.module.as_str()), Json::from(e.event.as_str()), Json::from(e.build.map(hex))])
                })),
            ),
        ])
    }

    fn from_json(j: &Json) -> Result<ModuleHistory, String> {
        let hash = |j: &Json| j.as_str().and_then(|s| u64::from_str_radix(s, 16).ok());
        let mut h = ModuleHistory::default();
        for (name, v) in j.get("base").and_then(Json::as_object).unwrap_or_default() {
            let r = v.as_array().filter(|r| r.len() == 2).ok_or("modules.base: each module is [build_hash, loaded_tick]")?;
            let (b, t) = (hash(&r[0]).ok_or("modules.base: bad build hash")?, r[1].as_u64().ok_or("modules.base: bad tick")?);
            h.base.insert(name.clone(), (b, t));
        }
        for e in j.get("events").and_then(Json::as_array).unwrap_or_default() {
            let r = e.as_array().filter(|r| r.len() == 4).ok_or("modules.events: each is [tick, module, event, build_hash or null]")?;
            h.events.push(ModuleMark {
                tick: r[0].as_u64().ok_or("modules.events: bad tick")?,
                module: r[1].as_str().ok_or("modules.events: bad module")?.to_owned(),
                event: r[2].as_str().ok_or("modules.events: bad event")?.to_owned(),
                build: if r[3].is_null() { None } else { Some(hash(&r[3]).ok_or("modules.events: bad build hash")?) },
            });
        }
        Ok(h)
    }
}

/// The engine's per-tick hashes (see the module docs).
#[derive(Default)]
pub struct TickLog {
    records: VecDeque<TickRecord>,
    /// Reused for the game's hashes each tick (no allocation per tick once warm).
    pub scratch: Vec<(&'static str, u64)>,
    /// Module code changes (bounded: the oldest fold into `modules.base`).
    modules: ModuleHistory,
}

impl TickLog {
    /// Records tick `tick`'s hashes (the game's are in `scratch`, which is cleared).
    pub fn record(&mut self, tick: u64, world: u64, next_id: u64) {
        let rec = if self.records.len() >= MAX_TICKS {
            let mut r = self.records.pop_front().expect("full");
            r.tick = tick;
            r.world = world;
            r.next_id = next_id;
            r.subsystems.clear();
            r
        } else {
            TickRecord { tick, world, next_id, subsystems: Vec::new() }
        };
        let mut rec = rec;
        // A recycled record keeps its vector's capacity: no allocation per tick once warm.
        rec.subsystems.append(&mut self.scratch);
        self.records.push_back(rec);
    }

    /// Forgets the ticks (a new world was loaded: its ticks start a new run). Module history
    /// stays: the modules keep running in the new world.
    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Records a module event (the engine journals it too).
    pub fn note_module(&mut self, ev: &ModuleEvent) {
        if self.modules.events.len() >= MAX_MODULE_EVENTS {
            let old = self.modules.events.remove(0);
            match old.build {
                Some(b) => self.modules.base.insert(old.module, (b, old.tick)),
                None => self.modules.base.remove(&old.module),
            };
        }
        self.modules.events.push(ModuleMark {
            tick: ev.tick,
            module: ev.module.clone(),
            event: ev.event.to_owned(),
            build: ev.running().then_some(ev.build_hash),
        });
    }

    pub fn modules(&self) -> &ModuleHistory {
        &self.modules
    }

    pub fn records(&self) -> impl Iterator<Item = &TickRecord> + '_ {
        self.records.iter()
    }

    pub fn range(&self) -> Option<(u64, u64)> {
        Some((self.records.front()?.tick, self.records.back()?.tick))
    }
}

/// One entity's state going into a tick, as a trace records it.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub path: String,
    /// The content hash afterwards; `None` = deleted.
    pub hash: Option<u64>,
    /// Field → 16-bit digest of its value.
    pub fields: Vec<(String, u16)>,
}

/// A run's trace (see the module docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trace {
    pub game: Option<String>,
    pub tick_hz: u32,
    /// Tick → (world hash, subsystem hashes).
    pub ticks: BTreeMap<u64, (u64, Vec<(String, u64)>)>,
    /// Tick → entity id → its state going into that tick.
    pub changes: BTreeMap<u64, BTreeMap<u64, Mark>>,
    /// Entity lists are complete from this tick on.
    pub entities_from: u64,
    /// Module builds (`None`: a trace written before they were recorded).
    pub modules: Option<ModuleHistory>,
}

fn digest(v: &Json) -> u16 {
    (hash_str(&v.to_string()) & 0xffff) as u16
}

/// Field digests of an entity's listing.
fn fields_of(e: &Entity, path: &str) -> Vec<(String, u16)> {
    let j = e.to_json(path);
    j.as_object()
        .unwrap_or_default()
        .iter()
        .filter(|(k, _)| !IGNORED.contains(&k.as_str()))
        .flat_map(|(k, v)| {
            if k == "props" {
                v.as_object().unwrap_or_default().iter().map(|(pk, pv)| (format!("props.{pk}"), digest(pv))).collect::<Vec<_>>()
            } else {
                vec![(k.clone(), digest(v))]
            }
        })
        .collect()
}

fn mark_of(c: &Change) -> Option<(u64, Mark)> {
    let id = c.id();
    if id >= CAMERA_ID_BASE {
        return None; // cameras are agents' views, not world state
    }
    Some(match c {
        Change::Spawn { after, path } | Change::Set { after, path, .. } => {
            (id, Mark { path: path.clone(), hash: Some(after.content_hash()), fields: fields_of(after, path) })
        }
        Change::Delete { path, .. } => (id, Mark { path: path.clone(), hash: None, fields: Vec::new() }),
        Change::Env { after, .. } => {
            let j = after.to_json();
            let fields = j.as_object().unwrap_or_default().iter().map(|(k, v)| (k.clone(), digest(v))).collect();
            (id, Mark { path: "@env".into(), hash: Some(hash_str(&j.to_string())), fields })
        }
        Change::Pose { path, pose } => {
            let v = |x: [f32; 3]| Json::arr(x.map(|c| Json::from(f64::from(c))));
            let fields: Vec<(String, u16)> = [
                ("position", v([pose.position.x, pose.position.y, pose.position.z])),
                ("rotation_deg", v([pose.rotation_deg.x, pose.rotation_deg.y, pose.rotation_deg.z])),
                ("scale", v([pose.scale.x, pose.scale.y, pose.scale.z])),
                ("color", v(pose.color)),
            ]
            .into_iter()
            .map(|(k, j)| (k.to_owned(), digest(&j)))
            .collect();
            let h = fields.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, (_, d)| (h ^ u64::from(*d)).wrapping_mul(0x100_0000_01b3));
            (id, Mark { path: path.clone(), hash: Some(h), fields })
        }
        Change::Camera { .. } => return None,
    })
}

impl Trace {
    /// This engine's trace for ticks `from..=to` (`None`: everything kept).
    pub fn live(log: &TickLog, journal: &Journal, game: Option<&str>, tick_hz: u32, from: Option<u64>, to: Option<u64>) -> Trace {
        let (lo, hi) = (from.unwrap_or(0), to.unwrap_or(u64::MAX));
        let ticks = log
            .records()
            .filter(|r| (lo..=hi).contains(&r.tick))
            .map(|r| (r.tick, (r.world, r.subsystems.iter().map(|(n, h)| ((*n).to_owned(), *h)).collect())))
            .collect::<BTreeMap<_, _>>();
        let mut changes: BTreeMap<u64, BTreeMap<u64, Mark>> = BTreeMap::new();
        if let (Some(&first), Some(&last)) = (ticks.keys().next(), ticks.keys().next_back()) {
            for e in journal.entries() {
                let into = e.tick + 1;
                if into < first || into > last {
                    continue;
                }
                for c in &e.changes {
                    if let Some((id, m)) = mark_of(c) {
                        changes.entry(into).or_default().insert(id, m);
                    }
                }
            }
        }
        // Complete from past the newest tick either journal ring evicted entries of.
        let entities_from = journal.complete_from_tick();
        Trace { game: game.map(str::to_owned), tick_hz, ticks, changes, entities_from, modules: Some(log.modules.clone()) }
    }

    /// The trace as `specula.trace/v1` text (one tick per line).
    pub fn render(&self) -> String {
        let mut out = format!(
            "{{\"format\": \"{FORMAT}\", \"game\": {}, \"tick_hz\": {}, \"from_tick\": {}, \"to_tick\": {}, \"entities_from_tick\": {},\n\"ticks\": [",
            self.game.as_deref().map_or(Json::Null, Json::from),
            self.tick_hz,
            Json::from(self.ticks.keys().next().copied()),
            Json::from(self.ticks.keys().next_back().copied()),
            self.entities_from
        );
        for (i, (t, (w, subs))) in self.ticks.iter().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            let subs = Json::obj(subs.iter().map(|(n, h)| (n.clone(), Json::from(hex(*h)))));
            out.push_str(&Json::arr([Json::from(*t), Json::from(hex(*w)), subs]).to_string());
        }
        out.push_str("\n],\n\"changes\": [");
        for (i, (t, marks)) in self.changes.iter().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            let items = marks.iter().map(|(id, m)| {
                let fields: Vec<String> = m.fields.iter().map(|(k, d)| format!("{k}={d:04x}")).collect();
                Json::arr([Json::from(*id), Json::from(m.path.as_str()), m.hash.map_or(Json::Null, |h| Json::from(hex(h))), Json::from(fields.join(";"))])
            });
            out.push_str(&Json::arr([Json::from(*t), Json::arr(items)]).to_string());
        }
        out.push_str("\n]");
        if let Some(m) = &self.modules {
            out.push_str(",\n\"modules\": ");
            out.push_str(&m.to_json().to_string());
        }
        out.push_str("}\n");
        out
    }

    /// Reads `specula.trace/v1` text.
    pub fn parse(text: &str) -> Result<Trace, String> {
        let doc = Json::parse(text).map_err(|e| e.to_string())?;
        if doc.get("format").and_then(Json::as_str) != Some(FORMAT) {
            return Err(format!("not a trace (format must be \"{FORMAT}\")"));
        }
        let hash = |j: Option<&Json>| j.and_then(Json::as_str).and_then(|s| u64::from_str_radix(s, 16).ok());
        let mut t = Trace {
            game: doc.get("game").and_then(Json::as_str).map(str::to_owned),
            tick_hz: doc.get("tick_hz").and_then(Json::as_u64).unwrap_or(0) as u32,
            entities_from: doc.get("entities_from_tick").and_then(Json::as_u64).unwrap_or(0),
            modules: doc.get("modules").map(ModuleHistory::from_json).transpose()?,
            ..Trace::default()
        };
        for row in doc.get("ticks").and_then(Json::as_array).unwrap_or_default() {
            let r = row.as_array().filter(|r| r.len() == 3).ok_or("each tick is [tick, world_hash, {subsystem: hash}]")?;
            let tick = r[0].as_u64().ok_or("a tick number is missing")?;
            let world = hash(Some(&r[1])).ok_or(format!("tick {tick}: bad world hash"))?;
            let subs = r[2].as_object().unwrap_or_default().iter().filter_map(|(k, v)| Some((k.clone(), hash(Some(v))?))).collect();
            t.ticks.insert(tick, (world, subs));
        }
        for row in doc.get("changes").and_then(Json::as_array).unwrap_or_default() {
            let r = row.as_array().filter(|r| r.len() == 2).ok_or("each changes row is [tick, [[id, path, hash, fields], ...]]")?;
            let tick = r[0].as_u64().ok_or("a changes row has no tick")?;
            let marks = t.changes.entry(tick).or_default();
            for m in r[1].as_array().unwrap_or_default() {
                let m = m.as_array().filter(|m| m.len() == 4).ok_or(format!("tick {tick}: each change is [id, path, hash, fields]"))?;
                let id = m[0].as_u64().ok_or(format!("tick {tick}: a change has no id"))?;
                let fields = m[3]
                    .as_str()
                    .unwrap_or("")
                    .split(';')
                    .filter_map(|kv| kv.split_once('='))
                    .filter_map(|(k, d)| Some((k.to_owned(), u16::from_str_radix(d, 16).ok()?)))
                    .collect();
                marks.insert(id, Mark { path: m[1].as_str().unwrap_or("").to_owned(), hash: hash(Some(&m[2])), fields });
            }
        }
        Ok(t)
    }

    /// Reads a trace file (at most [`MAX_TRACE_BYTES`]).
    pub fn read(path: &Path) -> Result<Trace, String> {
        let size = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
        if size > MAX_TRACE_BYTES {
            return Err(format!("{} is {size} bytes; traces are read up to {MAX_TRACE_BYTES}", path.display()));
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Trace::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// A short description for answers.
    pub fn summary(&self) -> Json {
        Json::obj([
            ("game", self.game.as_deref().map_or(Json::Null, Json::from)),
            ("ticks", Json::from(self.ticks.len())),
            ("from_tick", Json::from(self.ticks.keys().next().copied())),
            ("to_tick", Json::from(self.ticks.keys().next_back().copied())),
            ("entities_from_tick", Json::from(self.entities_from)),
            ("entity_changes", Json::from(self.changes.values().map(BTreeMap::len).sum::<usize>())),
            ("module_events", self.modules.as_ref().map_or(Json::Null, |m| Json::from(m.events.len()))),
        ])
    }
}

/// Modules whose code differs between two traces at hash tick `t` (another build, loaded at
/// another tick, or running in one run only); `None` when a trace doesn't record modules.
fn module_differences(a: &Trace, b: &Trace, t: u64) -> Option<Vec<(String, Json)>> {
    let (ma, mb) = (a.modules.as_ref()?.builds_at(t), b.modules.as_ref()?.builds_at(t));
    let names: BTreeSet<&String> = ma.keys().chain(mb.keys()).collect();
    let side = |m: &BTreeMap<String, (u64, u64)>, n: &str| {
        m.get(n).map_or(Json::Null, |(b, at)| Json::obj([("build_hash", Json::from(hex(*b))), ("loaded_at_tick", Json::from(*at))]))
    };
    let mut out = Vec::new();
    for n in names {
        let (x, y) = (ma.get(n), mb.get(n));
        let what = match (x, y) {
            (Some((bx, _)), Some((by, _))) if bx != by => format!("module {n}: build {} in a, {} in b", hex(*bx), hex(*by)),
            (Some((_, tx)), Some((_, ty))) if tx != ty => format!("module {n}: same build, loaded at tick {tx} in a, {ty} in b"),
            (Some(_), None) => format!("module {n}: running in a only"),
            (None, Some(_)) => format!("module {n}: running in b only"),
            _ => continue,
        };
        let item = Json::obj([("module", Json::from(n.as_str())), ("a", side(&ma, n)), ("b", side(&mb, n)), ("difference", Json::from(what.as_str()))]);
        out.push((what, item));
    }
    Some(out)
}

/// Fields whose digests differ (or exist on one side only).
fn field_diff(a: &Mark, b: &Mark) -> Vec<String> {
    let mut names: BTreeSet<&str> = BTreeSet::new();
    names.extend(a.fields.iter().map(|(k, _)| k.as_str()));
    names.extend(b.fields.iter().map(|(k, _)| k.as_str()));
    let get = |m: &Mark, k: &str| m.fields.iter().find(|(n, _)| n == k).map(|(_, d)| *d);
    names.into_iter().filter(|k| get(a, k) != get(b, k)).map(str::to_owned).collect()
}

/// The first divergence between two traces (see the module docs).
pub fn compare(a: &Trace, b: &Trace) -> Result<Json, crate::world::WorldError> {
    use crate::world::WorldError;
    let common: Vec<u64> = a.ticks.keys().filter(|t| b.ticks.contains_key(t)).copied().collect();
    let range = |t: &Trace| Json::arr([Json::from(t.ticks.keys().next().copied()), Json::from(t.ticks.keys().next_back().copied())]);
    if common.is_empty() {
        return Err(WorldError::new(
            "no_common_ticks",
            "the two traces share no tick: record both runs over the same ticks (desync_trace from_tick/to_tick), from the same start",
        )
        .with("a_ticks", range(a))
        .with("b_ticks", range(b)));
    }
    let mut last_match = None;
    for &t in &common {
        let ((wa, sa), (wb, sb)) = (&a.ticks[&t], &b.ticks[&t]);
        let mut names: BTreeSet<&str> = sa.iter().map(|(n, _)| n.as_str()).collect();
        names.extend(sb.iter().map(|(n, _)| n.as_str()));
        let sub = |s: &[(String, u64)], n: &str| s.iter().find(|(k, _)| k == n).map(|(_, h)| *h);
        let subsystems: Vec<Json> = names
            .into_iter()
            .filter(|n| sub(sa, n) != sub(sb, n))
            .take(MAX_NAMED)
            .map(|n| Json::obj([("name", Json::from(n)), ("a", Json::from(sub(sa, n).map(hex))), ("b", Json::from(sub(sb, n).map(hex)))]))
            .collect();
        if wa == wb && subsystems.is_empty() {
            last_match = Some(t);
            continue;
        }
        let mut out = vec![
            ("match", Json::from(false)),
            ("first_divergent_tick", Json::from(t)),
            ("last_matching_tick", Json::from(last_match)),
            ("diverged_before_common_range", Json::from(last_match.is_none())),
            ("world", Json::obj([("differs", Json::from(wa != wb)), ("a", Json::from(hex(*wa))), ("b", Json::from(hex(*wb)))])),
            ("subsystems", Json::Arr(subsystems.clone())),
        ];
        let mut components = Vec::new();
        if wa != wb {
            let complete = t >= a.entities_from && t >= b.entities_from;
            let empty = BTreeMap::new();
            let (ma, mb) = (a.changes.get(&t).unwrap_or(&empty), b.changes.get(&t).unwrap_or(&empty));
            let ids: BTreeSet<u64> = ma.keys().chain(mb.keys()).copied().collect();
            let mut entities = Vec::new();
            let mut differing = 0;
            for id in ids {
                let (x, y) = (ma.get(&id), mb.get(&id));
                let item = match (x, y) {
                    (Some(x), Some(y)) if x.hash == y.hash && x.path == y.path => continue,
                    (Some(x), Some(y)) => {
                        let fields = if x.hash.is_none() || y.hash.is_none() { vec!["(deleted in one run)".to_owned()] } else { field_diff(x, y) };
                        let mut f = vec![("id", Json::from(id)), ("path", Json::from(x.path.as_str()))];
                        if x.path != y.path {
                            f.push(("path_b", Json::from(y.path.as_str())));
                        }
                        f.push(("fields", Json::arr(fields.iter().map(|s| Json::from(s.as_str())))));
                        f.push(("a", Json::from(x.hash.map(hex))));
                        f.push(("b", Json::from(y.hash.map(hex))));
                        Json::obj(f)
                    }
                    (Some(m), None) | (None, Some(m)) => {
                        let side = if x.is_some() { "a" } else { "b" };
                        Json::obj([
                            ("id", Json::from(id)),
                            ("path", Json::from(m.path.as_str())),
                            ("changed_only_in", Json::from(side)),
                            ("deleted", Json::from(m.hash.is_none())),
                        ])
                    }
                    (None, None) => continue,
                };
                differing += 1;
                if entities.len() < MAX_NAMED {
                    entities.push(item);
                }
            }
            for e in &entities {
                let path = e.get("path").and_then(Json::as_str).unwrap_or("?");
                let fields: Vec<&str> = e.get("fields").and_then(Json::as_array).unwrap_or_default().iter().filter_map(Json::as_str).collect();
                components.push(if fields.is_empty() { format!("entity {path}") } else { format!("entity {path}: {}", fields.join(", ")) });
            }
            out.push(("entities", Json::Arr(entities)));
            out.push(("entities_differing", Json::from(differing)));
            out.push(("entity_lists_complete", Json::from(complete)));
            if differing == 0 {
                out.push((
                    "note",
                    Json::from(if complete {
                        "the world hashes differ but no entity changed differently going into this tick: the difference is in the \
                         environment, the id counter, or state that bypassed the journal (compare against 'replay')"
                    } else {
                        "the world hashes differ, but one trace's entity lists don't reach back to this tick (journal evicted): \
                         record shorter traces, or compare closer to the start"
                    }),
                ));
            }
        }
        for s in &subsystems {
            components.push(format!("subsystem {}", s.get("name").and_then(Json::as_str).unwrap_or("?")));
        }
        // Different module code is the likely cause: named first.
        match module_differences(a, b, t) {
            Some(diffs) => {
                let named: Vec<String> = diffs.iter().map(|(w, _)| w.clone()).collect();
                out.push(("modules", Json::arr(diffs.into_iter().map(|(_, j)| j))));
                components.splice(0..0, named);
            }
            None => out.push(("modules_known", Json::from(false))),
        }
        out.push(("components", Json::arr(components.into_iter().take(MAX_NAMED).map(Json::from))));
        out.push(("common_ticks", Json::from(common.len())));
        return Ok(Json::obj(out));
    }
    let last = *common.last().expect("not empty");
    let mut out = vec![
        ("match", Json::from(true)),
        ("common_ticks", Json::from(common.len())),
        ("from_tick", Json::from(common[0])),
        ("to_tick", Json::from(last)),
    ];
    // Rule 8: different builds are reported even when the runs agree.
    if let Some(diffs) = module_differences(a, b, last).filter(|d| !d.is_empty()) {
        out.push(("module_builds_differ", Json::arr(diffs.into_iter().map(|(_, j)| j))));
    }
    Ok(Json::obj(out))
}

/// This engine against its own journal: replays it from the base tick by tick and compares with
/// the recorded hashes; then names live entities whose state differs from the full replay.
pub fn against_replay(log: &TickLog, journal: &Journal, live: &World, from_tick: Option<u64>) -> Json {
    against_replay_within(log, journal, live, from_tick, REPLAY_BUDGET)
}

/// [`against_replay`] with `budget` entity hashes for this call.
fn against_replay_within(log: &TickLog, journal: &Journal, live: &World, from_tick: Option<u64>, budget: usize) -> Json {
    if !journal.replayable() {
        return Json::obj([
            ("replayable", Json::from(false)),
            ("message", Json::from("older journal entries were evicted (memory limit), so it can't replay from its base")),
        ]);
    }
    let mut w = journal.base().clone();
    w.set_recording(crate::world::Recording::Off);
    let entries: Vec<&crate::journal::Entry> = journal.entries().collect();
    let mut next = 0;
    let mut first = None;
    let mut compared = 0;
    // (hash, next_id) of the last hash taken, valid until the replay moves on.
    let mut cached: Option<(u64, u64)> = None;
    let mut spent = 0usize;
    let mut resume = None;
    for r in log.records() {
        let mut moved = false;
        while next < entries.len() && entries[next].tick < r.tick {
            let e = entries[next];
            for c in &e.changes {
                w.apply_after(c, &e.by);
            }
            w.replay_next_id(e.next_id);
            next += 1;
            moved = true;
        }
        if moved {
            cached = None;
        }
        if from_tick.is_some_and(|f| r.tick < f) {
            continue; // compared by an earlier call
        }
        // Rejected commands use ids without an entry: the recorded counter is the truth.
        w.replay_next_id(r.next_id);
        let h = match cached {
            Some((h, id)) if id == w.next_id() => h,
            _ => {
                // Bounded work per call: hashing costs about one step per entity.
                let cost = w.len().max(1);
                if compared > 0 && spent + cost > budget {
                    resume = Some(r.tick);
                    break;
                }
                spent += cost;
                w.state_hash()
            }
        };
        cached = Some((h, w.next_id()));
        compared += 1;
        if h != r.world {
            first = Some(Json::obj([("tick", Json::from(r.tick)), ("recorded", Json::from(hex(r.world))), ("replayed", Json::from(hex(h)))]));
            break;
        }
    }
    if let Some(t) = resume {
        return Json::obj([
            ("replayable", Json::from(true)),
            ("done", Json::from(false)),
            ("ticks_compared", Json::from(compared)),
            ("from_tick", Json::from(from_tick)),
            ("next_from_tick", Json::from(t)),
            ("message", Json::from("no divergence so far; call again with from_tick: next_from_tick to go on (each call does bounded work)")),
        ]);
    }
    let full = journal.replay().expect("replayable");
    let mut differing = Vec::new();
    let mut count = 0;
    let ids: BTreeSet<u64> = live.entities().filter(|e| !e.transient).map(|e| e.id).chain(full.entities().map(|e| e.id)).collect();
    for id in ids {
        let (a, b) = (live.get(id).filter(|e| !e.transient), full.get(id));
        let same = match (a, b) {
            (Some(a), Some(b)) => a.content_hash() == b.content_hash(),
            (None, None) => true,
            _ => false,
        };
        if !same {
            count += 1;
            if differing.len() < MAX_NAMED {
                let path = if a.is_some() { live.path_of(id) } else { full.path_of(id) };
                let fields = match (a, b) {
                    (Some(a), Some(b)) => crate::conflict::changed_fields(&b.to_json(&full.path_of(id)), &a.to_json(&path)),
                    _ => vec![if a.is_some() { "(only live)".to_owned() } else { "(only in the replay)".to_owned() }],
                };
                differing.push(Json::obj([("id", Json::from(id)), ("path", Json::from(path)), ("fields", Json::arr(fields.into_iter().map(Json::from)))]));
            }
        }
    }
    let matches = first.is_none() && count == 0 && full.state_hash() == live.state_hash();
    Json::obj([
        ("replayable", Json::from(true)),
        ("done", Json::from(true)),
        ("match", Json::from(matches)),
        ("ticks_compared", Json::from(compared)),
        ("first_divergent_tick", first.unwrap_or(Json::Null)),
        ("entities_differing", Json::from(count)),
        ("entities", Json::Arr(differing)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{self, Kind, Writer};
    use crate::world::{EntityKey, Recording, Shape, SpawnSpec, UpdateSpec};
    use specula_core::math::Vec3;

    /// A tiny deterministic "run": an entity moved every tick, a subsystem hash; `bump` moves it
    /// differently from tick `at` on.
    fn run(ticks: u64, bump: Option<(u64, f32)>, sub_bump: Option<u64>) -> (TickLog, Journal, World) {
        let mut w = World::default();
        w.set_recording(Recording::Full);
        let mut j = Journal::default();
        let mut log = TickLog::default();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("mover".into());
        journal::commit(&mut w, &mut j, 0, Writer::from("game"), Kind::Spawn, None, |w| w.spawn(s, "game").map(|_| ())).unwrap();
        for t in 0..ticks {
            let x = t as f32 + bump.filter(|(at, _)| t >= *at).map_or(0.0, |(_, d)| d);
            let u = UpdateSpec { position: Some(Vec3::new(x, 0.0, 0.0)), ..Default::default() };
            journal::commit(&mut w, &mut j, t, Writer::from("game"), Kind::Set, None, |w| w.update(&EntityKey::Path("/mover".into()), u, "game").map(|_| ()))
                .unwrap();
            log.scratch.push(("sim", if sub_bump.is_some_and(|b| t + 1 >= b) { 99 } else { t + 1 }));
            log.record(t + 1, w.state_hash(), w.next_id());
        }
        (log, j, w)
    }

    #[test]
    fn identical_runs_match_and_a_divergence_is_found_at_its_first_tick_and_field() {
        let (la, ja, _) = run(20, None, None);
        let (lb, jb, _) = run(20, Some((12, 0.5)), None);
        let a = Trace::live(&la, &ja, Some("g"), 30, None, None);
        let b = Trace::live(&lb, &jb, Some("g"), 30, None, None);
        assert_eq!(compare(&a, &a).unwrap().get("match"), Some(&Json::from(true)));
        let r = compare(&a, &b).unwrap();
        assert_eq!(r.get("first_divergent_tick"), Some(&Json::from(13)), "tick 12's move shows in tick 13's hash");
        assert_eq!(r.get("last_matching_tick"), Some(&Json::from(12)));
        let e = &r.get("entities").unwrap().as_array().unwrap()[0];
        assert_eq!((e.get("path"), e.get("fields")), (Some(&Json::from("/mover")), Some(&Json::arr([Json::from("position")]))));
        assert_eq!(r.get("components"), Some(&Json::arr([Json::from("entity /mover: position")])));
        // Through the file format and back.
        let text = b.render();
        let parsed = Trace::parse(&text).unwrap();
        assert_eq!(parsed, b);
        assert_eq!(compare(&a, &parsed).unwrap().get("first_divergent_tick"), Some(&Json::from(13)));
    }

    #[test]
    fn a_subsystem_divergence_is_named_even_when_the_world_agrees() {
        let (la, ja, _) = run(10, None, None);
        let (lb, jb, _) = run(10, None, Some(6));
        let r = compare(&Trace::live(&la, &ja, None, 30, None, None), &Trace::live(&lb, &jb, None, 30, None, None)).unwrap();
        assert_eq!(r.get("first_divergent_tick"), Some(&Json::from(6)));
        assert_eq!(r.get("world").and_then(|w| w.get("differs")), Some(&Json::from(false)));
        assert_eq!(r.get("components"), Some(&Json::arr([Json::from("subsystem sim")])));
        // Disjoint ranges: refused with both ranges.
        let only = Trace::live(&la, &ja, None, 30, Some(1), Some(3));
        let late = Trace::live(&lb, &jb, None, 30, Some(5), None);
        assert_eq!(compare(&only, &late).unwrap_err().code, "no_common_ticks");
    }

    #[test]
    fn the_live_world_is_checked_against_its_own_replay() {
        let (log, j, w) = run(8, None, None);
        let r = against_replay(&log, &j, &w, None);
        assert_eq!((r.get("match"), r.get("ticks_compared")), (Some(&Json::from(true)), Some(&Json::from(8))));
        // A change that bypasses the journal is caught and named.
        let mut w = w;
        w.set_recording(Recording::Off);
        w.update(&EntityKey::Path("/mover".into()), UpdateSpec { scale: Some(Vec3::ONE * 2.0), ..Default::default() }, "sneaky").unwrap();
        let r = against_replay(&log, &j, &w, None);
        assert_eq!(r.get("match"), Some(&Json::from(false)));
        let e = &r.get("entities").unwrap().as_array().unwrap()[0];
        assert_eq!(e.get("fields"), Some(&Json::arr([Json::from("scale")])));
    }

    /// Review round 3: a big world over many ticks stalled the engine thread (1.1 s); each call
    /// is bounded and hands back a cursor, and the calls together compare every tick.
    #[test]
    fn replay_checks_are_bounded_and_resume() {
        let (log, j, w) = run(40, None, None);
        let (mut from, mut calls, mut total) = (None, 0, 0);
        loop {
            let r = against_replay_within(&log, &j, &w, from, 10);
            calls += 1;
            total += r.get("ticks_compared").and_then(Json::as_u64).unwrap();
            if r.get("done") == Some(&Json::from(true)) {
                assert_eq!(r.get("match"), Some(&Json::from(true)));
                break;
            }
            from = r.get("next_from_tick").and_then(Json::as_u64);
            assert!(calls < 10);
        }
        assert_eq!((total, calls), (40, 4), "10 ticks per call");
    }

    fn ev(tick: u64, module: &str, event: &'static str, build: u64) -> ModuleEvent {
        ModuleEvent { tick, module: module.into(), event, generation: 1, build_hash: build, previous_build: None }
    }

    /// Two runs of the same moves whose module builds differ from tick 12: the divergence names
    /// the module and both builds, first among the components; the module history survives the
    /// trace file format; runs that agree despite different builds say so.
    #[test]
    fn a_divergence_from_different_module_code_names_the_module() {
        let (mut la, ja, _) = run(20, None, None);
        let (mut lb, jb, _) = run(20, Some((12, 0.5)), None);
        for log in [&mut la, &mut lb] {
            log.note_module(&ev(0, "ambulo", "load", 0xaaaa));
        }
        lb.note_module(&ev(12, "ambulo", "reload", 0xbbbb));
        let a = Trace::live(&la, &ja, None, 30, None, None);
        let b = Trace::live(&lb, &jb, None, 30, None, None);
        let r = compare(&a, &b).unwrap();
        assert_eq!(r.get("first_divergent_tick"), Some(&Json::from(13)));
        let m = &r.get("modules").unwrap().as_array().unwrap()[0];
        assert_eq!(m.get("module").and_then(Json::as_str), Some("ambulo"));
        assert_eq!(m.get("a").and_then(|x| x.get("build_hash")).and_then(Json::as_str), Some("000000000000aaaa"));
        assert_eq!(m.get("b").and_then(|x| x.get("loaded_at_tick")).and_then(Json::as_u64), Some(12));
        let c = r.get("components").unwrap().as_array().unwrap();
        assert_eq!(c[0].as_str(), Some("module ambulo: build 000000000000aaaa in a, 000000000000bbbb in b"));
        assert_eq!(c[1].as_str(), Some("entity /mover: position"));
        // The file format keeps the history.
        let parsed = Trace::parse(&b.render()).unwrap();
        assert_eq!(parsed.modules, b.modules);
        assert_eq!(compare(&a, &parsed).unwrap().get("modules"), r.get("modules"));
        // Same moves, different builds: a match, with the builds reported.
        let (lc, jc, _) = run(20, None, None);
        let mut lc = lc;
        lc.note_module(&ev(0, "ambulo", "load", 0xcccc));
        let c = Trace::live(&lc, &jc, None, 30, None, None);
        let r = compare(&a, &c).unwrap();
        assert_eq!(r.get("match"), Some(&Json::from(true)));
        assert_eq!(r.get("module_builds_differ").and_then(Json::as_array).map(<[Json]>::len), Some(1));
        // Same builds: nothing about modules.
        assert!(compare(&a, &a).unwrap().get("module_builds_differ").is_none());
        // A trace from before module history: the comparison says it can't tell.
        let mut old = b.clone();
        old.modules = None;
        assert_eq!(compare(&a, &old).unwrap().get("modules_known"), Some(&Json::from(false)));
        assert!(Trace::parse(&old.render()).unwrap().modules.is_none());
    }

    #[test]
    fn module_history_is_bounded_and_folds_into_its_base() {
        let mut log = TickLog::default();
        log.note_module(&ev(1, "a", "load", 1));
        log.note_module(&ev(2, "b", "load", 2));
        log.note_module(&ev(3, "b", "unload", 2));
        for t in 0..MAX_MODULE_EVENTS as u64 {
            log.note_module(&ev(10 + t, "c", "reload", 100 + t));
        }
        let m = log.modules();
        assert_eq!(m.events.len(), MAX_MODULE_EVENTS);
        assert_eq!(m.base.get("a"), Some(&(1, 1)), "a's load folded into the base");
        assert!(!m.base.contains_key("b"), "b was unloaded");
        let at = m.builds_at(u64::MAX);
        assert_eq!(at.get("c").map(|x| x.0), Some(100 + MAX_MODULE_EVENTS as u64 - 1));
        assert_eq!(m.builds_at(2).get("a"), Some(&(1, 1)));
        // A disabled module isn't running.
        log.note_module(&ev(9999, "a", "disabled", 1));
        assert!(!log.modules().builds_at(u64::MAX).contains_key("a"));
    }

    #[test]
    fn the_tick_log_is_bounded_and_reuses_its_records() {
        let mut log = TickLog::default();
        for t in 0..(MAX_TICKS as u64 + 10) {
            log.scratch.push(("sim", t));
            log.record(t, t, 0);
        }
        assert_eq!(log.range(), Some((10, MAX_TICKS as u64 + 9)));
        assert!(log.records().all(|r| r.subsystems.len() == 1));
    }
}
