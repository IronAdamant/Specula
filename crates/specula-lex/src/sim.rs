//! The deterministic runtime: compiled [`Rules`] stepped one fixed tick at a time.
//!
//! Integers only, entities in id order (ids in creation order), named seeded RNG streams
//! (`Rng::named(seed, stream)`), ordered containers: the same rules, seed and commands give the
//! same [`Sim::hash`] on every tick and every machine. The phase order of a tick is documented
//! in `docs/design/LEX.md` and in [`Sim::step`].

use std::collections::BTreeMap;
use std::sync::Arc;

use specula_core::hash::StableHasher;
use specula_core::rng::Rng;
use specula_core::Json;

use crate::expr::{arith, compare, Arith, Ex, WaveQ, Who};
use crate::rules::*;
use crate::units::isqrt;

#[path = "sim_snapshot.rs"]
mod snapshot;
pub use snapshot::{SnapshotError, SNAPSHOT_VERSION};

/// Most events dispatched to rules in one tick before dispatch stops (`event_storm`).
pub const MAX_EVENTS_PER_TICK: usize = 100_000;
/// How deeply action lists may nest before `recursion_limit` stops them.
pub const MAX_ACTION_DEPTH: u32 = 32;
/// Entity checks (eligibility, splash, chase, count) per tick before the tick's remaining
/// behaviours are skipped (`work_limit`): a deterministic cap on one tick's cost.
pub const MAX_WORK_PER_TICK: u64 = 5_000_000;
/// Most effect instances on one entity (new ones beyond are skipped, `effect_limit`).
pub const MAX_EFFECTS_PER_ENTITY: usize = 8;
/// The run's state budget in cells, one per entity plus one per attribute and behaviour it
/// carries. A spawn past it fails (`state_limit`). With the caps on effects per entity and
/// modifiers per effect, this bounds a run's memory to a few hundred MB at worst.
pub const MAX_STATE_CELLS: u64 = 2_000_000;
/// Most spawns waiting in the wave queue at once (a wave that would pass it isn't scheduled).
pub const MAX_PENDING_SPAWNS: usize = 200_000;
/// Most entities one spawn action creates.
pub const MAX_SPAWN_COUNT: i64 = 10_000;
/// Most live entities; spawns beyond it are refused (`entity_limit`).
pub const MAX_ENTITIES: usize = 100_000;
/// Run-time diagnostics kept with their message (counts are always kept).
const MAX_DIAG_MESSAGES: usize = 20;

/// A value at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Val {
    Int(i64),
    Bool(bool),
    Ent(Option<u32>),
    Arch(Option<u16>),
    Site(u16),
}

impl Val {
    fn int(self) -> i64 {
        match self {
            Val::Int(v) => v,
            _ => 0,
        }
    }

    fn bool(self) -> bool {
        matches!(self, Val::Bool(true))
    }
}

/// A run-time error: aborts the action list it happened in (and rolls back a command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RtErr {
    pub code: &'static str,
    pub message: String,
}

fn rt(code: &'static str, message: impl Into<String>) -> RtErr {
    RtErr { code, message: message.into() }
}

/// Why a command was refused. Refused commands change nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Rejection {
    pub code: String,
    pub message: String,
    /// Machine-readable details (`need`, `have`, `arg`).
    pub details: Vec<(String, Json)>,
}

impl Rejection {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Rejection {
        Rejection { code: code.into(), message: message.into(), details: Vec::new() }
    }

    pub fn with(mut self, k: &str, v: Json) -> Rejection {
        self.details.push((k.to_owned(), v));
        self
    }

    pub fn to_json(&self) -> Json {
        let mut pairs = vec![("code".to_owned(), Json::from(self.code.as_str())), ("message".to_owned(), Json::from(self.message.as_str()))];
        pairs.extend(self.details.iter().cloned());
        Json::Obj(pairs)
    }
}

/// Something that happened, for rules, presentation and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: u16,
    pub subject: Option<u32>,
    pub other: Option<u32>,
    pub value: i64,
}

/// A command with its arguments resolved against the rules (entities are found when applied).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandCall {
    pub cmd: u16,
    pub args: Vec<ArgVal>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgVal {
    Site(u16),
    Arch(u16),
    Entity(u32),
    /// The lowest-id live entity on the site matching the arg's tags.
    EntityAt(u16),
    Num(i64),
}

/// An effect on an entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Active {
    pub effect: u16,
    pub magnitude: i64,
    pub applied: u32,
    /// `u32::MAX`: infinite.
    pub expires: u32,
    pub next_period: u32,
    pub stacks: u16,
    pub seq: u32,
    pub source: Option<u32>,
    /// Each modifier's value, evaluated when applied.
    pub mods: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proj {
    pub target: Option<u32>,
    pub aim: P3,
    /// Per payload entry: (magnitude or damage, duration override or -1).
    pub payload: Vec<(i64, i64)>,
    /// The firing attack: (archetype, behaviour index).
    pub attack: (u16, u16),
    pub source: Option<u32>,
    pub carry: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BehState {
    None,
    /// Path walkers (follow_path, patrol); `fwd` for pingpong.
    Path { path: Option<u16>, seg: u32, progress: i64, carry: i64, fwd: bool, done: bool },
    Chase { target: Option<u32>, carry: i64, reached: bool },
    Attack { ready_at: u32 },
    Projectile(Box<Proj>),
    Spawner { next: u32 },
    Lifetime { expires: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    pub id: u32,
    pub arch: u16,
    pub alive: bool,
    pub pos: P3,
    /// Base attribute values, parallel to the archetype's `attrs`.
    pub base: Vec<i64>,
    pub effects: Vec<Active>,
    pub beh: Vec<BehState>,
    /// Machine state and the tick it was entered.
    pub state: Option<(u16, u32)>,
    pub wave: Option<u16>,
    pub site: Option<u16>,
    pub spawn_path: Option<u16>,
    pub path_remaining: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaveStats {
    pub started_at: Option<u32>,
    pub cleared_at: Option<u32>,
    pub spawned: u32,
    pub died: u32,
    pub removed: u32,
    pub pending: u32,
    pub alive: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pending {
    tick: u32,
    seq: u32,
    arch: u16,
    wave: u16,
    at: SpawnAt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimerState {
    next: Option<u32>,
    fired: u32,
}

/// Run-time diagnostics: counts by code plus the first messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RtDiag {
    pub counts: BTreeMap<&'static str, u64>,
    pub first: Vec<String>,
}

impl RtDiag {
    fn note(&mut self, tick: u32, code: &'static str, msg: &str) {
        self.note_with(tick, code, || msg.to_owned());
    }

    /// Counts the problem; formats its message only while messages are still kept.
    fn note_with(&mut self, tick: u32, code: &'static str, msg: impl FnOnce() -> String) {
        *self.counts.entry(code).or_insert(0) += 1;
        if self.first.len() < MAX_DIAG_MESSAGES {
            self.first.push(format!("tick {tick}: [{code}] {}", msg()));
        }
    }
}

/// A native (module) hook: the escape hatch. See [`NativeCtx`] for what it may touch.
pub trait NativeHook: Send + Sync {
    fn run(&self, cx: &mut NativeCtx<'_, '_>) -> Result<(), String>;
}

/// Hook name → implementation.
pub type Natives = BTreeMap<String, Arc<dyn NativeHook>>;

/// Everything that changes during a run. Cloning it is a snapshot (sessions, what-ifs, rollbacks).
#[derive(Clone, Debug)]
pub struct State {
    pub tick: u32,
    pub seed: u64,
    /// The result and the tick it was decided.
    pub outcome: Option<(String, u32)>,
    pub res: Vec<i64>,
    rngs: Vec<Rng>,
    next_id: u32,
    pub ents: Vec<Entity>,
    pub wave: u16,
    pub next_wave_tick: Option<u32>,
    pub wave_stats: Vec<WaveStats>,
    pending: Vec<Pending>,
    next_seq: u32,
    timers: Vec<TimerState>,
    queue: Vec<Event>,
    /// This tick's events (for presentation and tests).
    pub events: Vec<Event>,
    /// Events since the start, by kind.
    pub event_counts: Vec<u64>,
    pub diag: RtDiag,
    pub native_calls: Vec<u64>,
    /// Something an outcome could read changed (cheap outcomes are due).
    dirty: bool,
    /// Since the last full check (outcomes that scan entities are due).
    dirty_full: bool,
    effect_seq: u32,
    /// This tick: events past the cap, and entity checks done (see [`MAX_WORK_PER_TICK`]),
    /// separately for commands and for the phases.
    dropped: u64,
    work: u64,
    command_work: u64,
    /// The state budget in use (see [`MAX_STATE_CELLS`]).
    pub cells: u64,
    /// A wall-clock deadline cut a tick short: the state is no longer a deterministic result.
    pub interrupted: bool,
    dispatched: usize,
}

impl State {
    /// Something outcomes could read changed.
    fn mark(&mut self) {
        self.dirty = true;
        self.dirty_full = true;
    }
}

/// A running rule set.
#[derive(Clone)]
pub struct Sim {
    rules: Arc<Rules>,
    natives: Vec<Option<Arc<dyn NativeHook>>>,
    st: State,
    deadline: Option<std::time::Instant>,
}

impl std::fmt::Debug for Sim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sim").field("rules", &self.rules.name).field("tick", &self.st.tick).finish()
    }
}

impl Sim {
    /// A fresh run. Fails if the rules use a native hook `natives` doesn't provide.
    pub fn new(rules: Arc<Rules>, seed: u64, natives: &Natives) -> Result<Sim, String> {
        let mut bound = Vec::new();
        for n in &rules.natives {
            match natives.get(&n.hook) {
                Some(h) => bound.push(Some(Arc::clone(h))),
                None => {
                    return Err(format!(
                        "native hook '{}' (item '{}') has no in-process implementation for this headless run (module hooks run in the engine's live tick, not in simulations): register a Natives entry, or replace the native with Lex building blocks",
                        n.hook, n.id
                    ))
                }
            }
        }
        let st = State {
            tick: 0,
            seed,
            outcome: None,
            res: rules.resources.iter().map(|r| r.start).collect(),
            rngs: rules.streams.iter().map(|s| Rng::named(seed, s)).collect(),
            next_id: 1,
            ents: Vec::new(),
            wave: 0,
            next_wave_tick: rules.waves.as_ref().map(|w| w.start),
            wave_stats: vec![WaveStats::default(); rules.waves.as_ref().map_or(0, |w| w.waves.len())],
            pending: Vec::new(),
            next_seq: 0,
            timers: rules.timers.iter().map(|t| TimerState { next: Some(t.start), fired: 0 }).collect(),
            queue: Vec::new(),
            events: Vec::new(),
            event_counts: vec![0; rules.events.len()],
            diag: RtDiag::default(),
            native_calls: vec![0; rules.natives.len()],
            dirty: true,
            dirty_full: true,
            effect_seq: 0,
            dropped: 0,
            work: 0,
            command_work: 0,
            cells: 0,
            interrupted: false,
            dispatched: 0,
        };
        Ok(Sim { rules, natives: bound, st, deadline: None })
    }

    pub fn rules(&self) -> &Arc<Rules> {
        &self.rules
    }

    pub fn state(&self) -> &State {
        &self.st
    }

    pub fn tick(&self) -> u32 {
        self.st.tick
    }

    pub fn outcome(&self) -> Option<(&str, u32)> {
        self.st.outcome.as_ref().map(|(s, t)| (s.as_str(), *t))
    }

    /// Live entities (and those that died this tick, until cleanup), by id.
    pub fn entities(&self) -> &[Entity] {
        &self.st.ents
    }

    pub fn entity(&self, id: u32) -> Option<&Entity> {
        self.st.ents.binary_search_by_key(&id, |e| e.id).ok().map(|i| &self.st.ents[i])
    }

    /// An entity's current attribute value (with effects), by attribute name.
    pub fn attr(&self, id: u32, name: &str) -> Option<i64> {
        let a = self.rules.attrs.iter().position(|x| x.name == name)? as u16;
        let e = self.entity(id)?;
        attr_value(&self.rules, &self.st, e, a)
    }

    /// An entity's current attribute value by attribute index (see [`Rules::attrs`]).
    pub fn attr_at(&self, id: u32, attr: u16) -> Option<i64> {
        attr_value(&self.rules, &self.st, self.entity(id)?, attr)
    }

    pub fn resource(&self, id: &str) -> Option<i64> {
        self.rules.resource(id).map(|r| self.st.res[usize::from(r)])
    }

    /// This tick's events.
    pub fn events(&self) -> &[Event] {
        &self.st.events
    }

    /// Resolves a command by name, with JSON args: sites and archetypes by name, entities by id or
    /// by a site name (its occupant), numbers in the arg's human unit.
    pub fn parse_command(&self, name: &str, args: &Json) -> Result<CommandCall, Rejection> {
        parse_command(&self.rules, name, args)
    }

    /// Applies a command now (before the tick's phases). Atomic: a refused or failing command
    /// changes nothing.
    ///
    /// Called between steps, it starts a fresh event list and work budget (like a step does);
    /// `events()` then shows the command's events.
    pub fn apply(&mut self, call: &CommandCall) -> Result<(), Rejection> {
        self.begin();
        self.command(call)
    }

    /// Resets the per-tick event list, dispatch count and work budget.
    fn begin(&mut self) {
        self.st.events.clear();
        self.st.dispatched = 0;
        self.st.dropped = 0;
        self.st.work = 0;
        self.st.command_work = 0;
    }

    fn command(&mut self, call: &CommandCall) -> Result<(), Rejection> {
        let mut c = Core { r: &self.rules, st: &mut self.st, natives: &self.natives, depth: 0, deadline: self.deadline, in_command: true };
        c.command(call)
    }

    /// Applies `commands` (each atomic; refusals are returned with their index), then advances
    /// one tick. Phases: waves, spawns, timers, spawners, movement, attacks, projectiles,
    /// effects and machines, cleanup; events reach rules after each phase.
    pub fn step(&mut self, commands: &[CommandCall]) -> Vec<(usize, Rejection)> {
        self.begin();
        let mut refused = Vec::new();
        for (i, c) in commands.iter().enumerate() {
            if let Err(e) = self.command(c) {
                refused.push((i, e));
            }
        }
        let mut c = Core { r: &self.rules, st: &mut self.st, natives: &self.natives, depth: 0, deadline: self.deadline, in_command: false };
        c.tick();
        refused
    }

    /// Steps until `tick` (exclusive) or an outcome.
    pub fn run_to(&mut self, tick: u32) {
        while self.st.tick < tick && self.st.outcome.is_none() {
            self.step(&[]);
        }
    }

    /// A wall-clock limit for the next steps: past it, the current tick stops early and the run
    /// is [`Sim::interrupted`] (its state is no longer a deterministic result).
    pub fn set_deadline(&mut self, deadline: Option<std::time::Instant>) {
        self.deadline = deadline;
    }

    /// A deadline cut a tick short: don't compare, save or continue this run.
    pub fn interrupted(&self) -> bool {
        self.st.interrupted
    }

    /// A 64-bit hash of the whole state (with the rules' hash), in a fixed order.
    pub fn hash(&self) -> u64 {
        state_hash(&self.rules, &self.st)
    }

    /// Evaluates a compiled global expression (no self) without changing anything: evaluation
    /// only touches the RNG streams, the diagnostics and the work counter, which are restored.
    pub fn query(&mut self, ex: &Ex) -> Result<Val, RtErr> {
        let (rngs, diag, work) = (self.st.rngs.clone(), self.st.diag.clone(), self.st.work);
        let mut c = Core { r: &self.rules, st: &mut self.st, natives: &self.natives, depth: 0, deadline: self.deadline, in_command: false };
        let v = c.eval(ex, &Frame::default());
        (self.st.rngs, self.st.diag, self.st.work) = (rngs, diag, work);
        v
    }
}

/// Evaluation context.
#[derive(Clone, Copy, Debug, Default)]
struct Frame<'a> {
    me: Option<u32>,
    other: Option<u32>,
    target: Option<u32>,
    magnitude: i64,
    event: i64,
    args: &'a [Val],
}

struct Core<'a> {
    r: &'a Rules,
    st: &'a mut State,
    natives: &'a [Option<Arc<dyn NativeHook>>],
    /// Nested action lists (on_spawn spawning, on_apply applying, on_enter switching...).
    depth: u32,
    /// A wall-clock limit, checked as work is spent; passing it interrupts the run.
    deadline: Option<std::time::Instant>,
    /// Applying a command: work is charged to the commands' budget, so commands can never
    /// starve the tick's phases (nor the phases the commands).
    in_command: bool,
}

fn idx_of(st: &State, id: u32) -> Option<usize> {
    st.ents.binary_search_by_key(&id, |e| e.id).ok()
}

fn effect_continuous(def: &EffectDef) -> bool {
    !matches!(def.duration, EffDur::Instant) && def.period.is_none()
}

/// An entity's current attribute value: base, then the active effects' adds, muls and sets.
fn attr_value(r: &Rules, st: &State, e: &Entity, attr: u16) -> Option<i64> {
    let slot = r.archetypes[usize::from(e.arch)].slot_of(attr)?;
    let mut v = e.base[slot];
    if e.effects.is_empty() {
        return Some(v);
    }
    let live = |a: &&Active| st.tick < a.expires && effect_continuous(&r.effects[usize::from(a.effect)]);
    let mut add = 0i64;
    for a in e.effects.iter().filter(live) {
        for (m, val) in r.effects[usize::from(a.effect)].modifiers.iter().zip(&a.mods) {
            if m.attr == attr && m.op == ModOp::Add {
                add = add.saturating_add(val.saturating_mul(i64::from(a.stacks)));
            }
        }
    }
    v = v.saturating_add(add);
    for a in e.effects.iter().filter(live) {
        for (m, val) in r.effects[usize::from(a.effect)].modifiers.iter().zip(&a.mods) {
            if m.attr == attr && m.op == ModOp::Mul {
                for _ in 0..a.stacks {
                    v = arith(Arith::MulRatio, v, *val).unwrap_or(v);
                }
            }
        }
    }
    for a in e.effects.iter().filter(live) {
        for (m, val) in r.effects[usize::from(a.effect)].modifiers.iter().zip(&a.mods) {
            if m.attr == attr && m.op == ModOp::Set {
                v = *val;
            }
        }
    }
    Some(v)
}

fn has_tag(r: &Rules, st: &State, e: &Entity, tag: u16) -> bool {
    if tag == u16::MAX {
        return false;
    }
    if r.archetypes[usize::from(e.arch)].tags.binary_search(&tag).is_ok() {
        return true;
    }
    e.effects.iter().any(|a| st.tick < a.expires && r.effects[usize::from(a.effect)].grant_tags.contains(&tag))
}

fn matches(r: &Rules, st: &State, e: &Entity, f: &TagFilter) -> bool {
    f.all.iter().all(|t| has_tag(r, st, e, *t)) && !f.none.iter().any(|t| has_tag(r, st, e, *t))
}

fn dist2(a: P3, b: P3) -> i128 {
    (0..3).fold(0i128, |acc, i| {
        let d = i128::from(a[i]) - i128::from(b[i]);
        acc.saturating_add(d.checked_mul(d).unwrap_or(i128::MAX))
    })
}

fn dist(a: P3, b: P3) -> i64 {
    isqrt(dist2(a, b) as u128).min(i64::MAX as u128) as i64
}

/// `a + (b - a) * num / den`, truncating toward zero; `den <= 0` gives `b`.
fn lerp(a: P3, b: P3, num: i64, den: i64) -> P3 {
    if den <= 0 {
        return b;
    }
    let f = |a: i64, b: i64| {
        let step = (i128::from(b) - i128::from(a)).saturating_mul(i128::from(num)) / i128::from(den);
        (i128::from(a) + step).clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    };
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

fn step_toward(from: P3, to: P3, d: i64) -> (P3, bool) {
    let len = dist(from, to);
    if len <= d {
        (to, true)
    } else {
        (lerp(from, to, d, len), false)
    }
}

impl Core<'_> {
    /// Adds `n` entity checks to this tick's work; true once the tick is over budget (the
    /// remaining behaviours of the tick are skipped, once noted).
    fn spend(&mut self, n: usize) -> bool {
        let counter = if self.in_command { &mut self.st.command_work } else { &mut self.st.work };
        let before = *counter;
        *counter = counter.saturating_add(n as u64);
        let now = *counter;
        if now > MAX_WORK_PER_TICK && before <= MAX_WORK_PER_TICK {
            if self.in_command {
                self.note("work_limit", "this tick's commands used more than 5 million units of work (many commands over a large state?); the rest were refused, and the tick's phases ran in full");
            } else {
                self.note("work_limit", "more than 5 million units of work in one tick (attacks or counts over many entities?); the rest of the tick's behaviours were skipped");
            }
        }
        // The wall clock is read every 16k units of work, so a deadline interrupts within a tick.
        if let Some(d) = self.deadline {
            if before >> 14 != now >> 14 && std::time::Instant::now() > d {
                self.st.interrupted = true;
            }
        }
        self.over_budget()
    }

    fn over_budget(&self) -> bool {
        let used = if self.in_command { self.st.command_work } else { self.st.work };
        used > MAX_WORK_PER_TICK || self.st.interrupted
    }

    /// What checking `e` against a tag filter costs: each tag looks through the entity's effects.
    fn check_cost(e: &Entity, f: &TagFilter) -> usize {
        1 + (f.all.len() + f.none.len()) * (1 + e.effects.len())
    }

    fn note(&mut self, code: &'static str, msg: &str) {
        let t = self.st.tick;
        self.st.diag.note(t, code, msg);
    }

    fn note_with(&mut self, code: &'static str, msg: impl FnOnce() -> String) {
        let t = self.st.tick;
        self.st.diag.note_with(t, code, msg);
    }

    fn emit(&mut self, kind: u16, subject: Option<u32>, other: Option<u32>, value: i64) {
        if let Some(c) = self.st.event_counts.get_mut(usize::from(kind)) {
            *c += 1;
        }
        if self.st.events.len() >= MAX_EVENTS_PER_TICK {
            if self.st.dropped == 0 {
                self.note("event_overflow", "more than 100000 events in one tick: later ones are counted but neither kept nor dispatched");
            }
            self.st.dropped += 1;
            return;
        }
        let e = Event { kind, subject, other, value };
        self.st.events.push(e);
        self.st.queue.push(e);
    }

    fn ent(&self, id: Option<u32>, what: &str) -> Result<usize, RtErr> {
        let id = id.ok_or_else(|| rt("no_entity", format!("{what} is none")))?;
        idx_of(self.st, id).ok_or_else(|| rt("no_entity", format!("{what} (entity {id}) no longer exists")))
    }

    /// Like [`Core::ent`], but the entity must also be alive (for changes to it).
    fn ent_alive(&self, id: Option<u32>, what: &str) -> Result<usize, RtErr> {
        let i = self.ent(id, what)?;
        if !self.st.ents[i].alive {
            return Err(rt("no_entity", format!("{what} (entity {}) has died or been removed this tick", self.st.ents[i].id)));
        }
        Ok(i)
    }

    // ---- expressions ----

    fn eval(&mut self, ex: &Ex, f: &Frame) -> Result<Val, RtErr> {
        Ok(match ex {
            Ex::Int(v) => Val::Int(*v),
            Ex::Bool(b) => Val::Bool(*b),
            Ex::Arch(a) => Val::Arch(Some(*a)),
            Ex::Site(s) => Val::Site(*s),
            Ex::Tick => Val::Int(i64::from(self.st.tick)),
            Ex::Res(r) => Val::Int(self.st.res[usize::from(*r)]),
            Ex::Magnitude => Val::Int(f.magnitude),
            Ex::EventValue => Val::Int(f.event),
            Ex::Waves(q) => {
                let total = self.r.waves.as_ref().map_or(0, |w| w.waves.len()) as i64;
                match q {
                    WaveQ::Current => Val::Int(i64::from(self.st.wave)),
                    WaveQ::Total => Val::Int(total),
                    WaveQ::Remaining => Val::Int(total - i64::from(self.st.wave)),
                    WaveQ::TicksLeft => Val::Int(self.st.next_wave_tick.map_or(0, |t| i64::from(t.saturating_sub(self.st.tick)))),
                    WaveQ::AllCleared => Val::Bool(self.all_cleared()),
                }
            }
            Ex::Who(w) => Val::Ent(match w {
                Who::Me => f.me,
                Who::Other => f.other,
                Who::Target => f.target,
            }),
            Ex::Arg(i) => f.args.get(usize::from(*i)).copied().unwrap_or(Val::Ent(None)),
            Ex::Attr(e, a) => {
                let id = self.eval_ent(e, f)?;
                let i = self.ent(id, "the entity")?;
                let ent = &self.st.ents[i];
                match attr_value(self.r, self.st, ent, *a) {
                    Some(v) => Val::Int(v),
                    None => {
                        return Err(rt(
                            "no_attribute",
                            format!("entity {} ({}) has no attribute '{}'", ent.id, self.r.archetypes[usize::from(ent.arch)].id, self.r.attrs[usize::from(*a)].name),
                        ))
                    }
                }
            }
            Ex::Alive(e) => {
                let id = self.eval_ent(e, f)?;
                Val::Bool(id.and_then(|id| idx_of(self.st, id)).is_some_and(|i| self.st.ents[i].alive))
            }
            Ex::PathRemaining(e) => {
                let id = self.eval_ent(e, f)?;
                let i = self.ent(id, "the entity")?;
                Val::Int(self.st.ents[i].path_remaining)
            }
            Ex::ArchOf(e) => {
                let id = self.eval_ent(e, f)?;
                let i = self.ent(id, "the entity")?;
                Val::Arch(Some(self.st.ents[i].arch))
            }
            Ex::Next(a) => match self.eval(a, f)? {
                Val::Arch(Some(a)) => Val::Arch(self.r.archetypes[usize::from(a)].next),
                _ => Val::Arch(None),
            },
            Ex::ArchAttr(a, attr) => match self.eval(a, f)? {
                Val::Arch(Some(a)) => {
                    let def = &self.r.archetypes[usize::from(a)];
                    match def.slot_of(*attr) {
                        Some(s) => Val::Int(def.attrs[s].1),
                        None => return Err(rt("no_attribute", format!("archetype '{}' has no attribute '{}'", def.id, self.r.attrs[usize::from(*attr)].name))),
                    }
                }
                _ => return Err(rt("no_archetype", "the archetype is none (e.g. no next tier)")),
            },
            Ex::Neg(e) => Val::Int(self.eval(e, f)?.int().saturating_neg()),
            Ex::Not(e) => Val::Bool(!self.eval(e, f)?.bool()),
            Ex::Arith(op, a, b) => {
                let (x, y) = (self.eval(a, f)?.int(), self.eval(b, f)?.int());
                match arith(*op, x, y) {
                    Some(v) => Val::Int(v),
                    None => {
                        self.note("division_by_zero", "a division by zero gave 0");
                        Val::Int(0)
                    }
                }
            }
            Ex::Cmp(op, a, b) => {
                let (x, y) = (self.eval(a, f)?, self.eval(b, f)?);
                Val::Bool(match (x, y) {
                    (Val::Int(x), Val::Int(y)) => compare(*op, x, y),
                    (x, y) => {
                        let eq = x == y;
                        if matches!(op, crate::expr::Cmp::Ne) {
                            !eq
                        } else {
                            eq
                        }
                    }
                })
            }
            Ex::And(a, b) => Val::Bool(self.eval(a, f)?.bool() && self.eval(b, f)?.bool()),
            Ex::Or(a, b) => Val::Bool(self.eval(a, f)?.bool() || self.eval(b, f)?.bool()),
            Ex::If(c, a, b) => {
                if self.eval(c, f)?.bool() {
                    self.eval(a, f)?
                } else {
                    self.eval(b, f)?
                }
            }
            Ex::Min(a, b) => Val::Int(self.eval(a, f)?.int().min(self.eval(b, f)?.int())),
            Ex::Max(a, b) => Val::Int(self.eval(a, f)?.int().max(self.eval(b, f)?.int())),
            Ex::Clamp(x, lo, hi) => {
                let (x, lo, hi) = (self.eval(x, f)?.int(), self.eval(lo, f)?.int(), self.eval(hi, f)?.int());
                Val::Int(x.max(lo).min(hi))
            }
            Ex::Abs(x) => Val::Int(self.eval(x, f)?.int().saturating_abs()),
            Ex::Exists(x) => Val::Bool(match self.eval(x, f)? {
                Val::Ent(Some(id)) => idx_of(self.st, id).is_some_and(|i| self.st.ents[i].alive),
                Val::Arch(a) => a.is_some(),
                _ => false,
            }),
            Ex::Has(e, t) => {
                let id = self.eval_ent(e, f)?;
                let cost = id.and_then(|id| idx_of(self.st, id)).map_or(1, |i| 1 + self.st.ents[i].effects.len());
                if self.spend(cost) {
                    return Err(rt("work_limit", "has() skipped: the tick's work budget is spent"));
                }
                Val::Bool(id.and_then(|id| idx_of(self.st, id)).is_some_and(|i| has_tag(self.r, self.st, &self.st.ents[i], *t)))
            }
            Ex::Count(t) => {
                let mut n = 0i64;
                for j in 0..self.st.ents.len() {
                    if self.spend(1 + self.st.ents[j].effects.len()) {
                        return Err(rt("work_limit", "count() stopped: the tick's work budget is spent"));
                    }
                    let e = &self.st.ents[j];
                    if e.alive && has_tag(self.r, self.st, e, *t) {
                        n += 1;
                    }
                }
                Val::Int(n)
            }
            Ex::Dist(a, b) => {
                let (x, y) = (self.eval_ent(a, f)?, self.eval_ent(b, f)?);
                let (i, j) = (self.ent(x, "the first entity")?, self.ent(y, "the second entity")?);
                Val::Int(dist(self.st.ents[i].pos, self.st.ents[j].pos))
            }
            Ex::Occupied(s, t) => {
                let Val::Site(site) = self.eval(s, f)? else { return Err(rt("no_site", "the site is none")) };
                let mut found = false;
                for j in 0..self.st.ents.len() {
                    if self.spend(1 + self.st.ents[j].effects.len()) {
                        return Err(rt("work_limit", "occupied() stopped: the tick's work budget is spent"));
                    }
                    let e = &self.st.ents[j];
                    if e.alive && e.site == Some(site) && has_tag(self.r, self.st, e, *t) {
                        found = true;
                        break;
                    }
                }
                Val::Bool(found)
            }
            Ex::Rand(s, lo, hi) => {
                let (lo, hi) = (self.eval(lo, f)?.int(), self.eval(hi, f)?.int());
                let (lo, hi) = if lo <= hi { (lo, hi) } else {
                    self.note("rand_range", "rand(lo, hi) with lo > hi; swapped");
                    (hi, lo)
                };
                let rng = &mut self.st.rngs[usize::from(*s)];
                match (i32::try_from(lo), i32::try_from(hi)) {
                    (Ok(l), Ok(h)) => Val::Int(i64::from(rng.range_i32(l, h))),
                    _ => {
                        let span = (i128::from(hi) - i128::from(lo) + 1) as u128;
                        Val::Int((i128::from(lo) + (u128::from(rng.next_u64()) % span) as i128) as i64)
                    }
                }
            }
            Ex::Chance(s, p) => {
                let p = self.eval(p, f)?.int().clamp(0, 1000) as u32;
                Val::Bool(self.st.rngs[usize::from(*s)].chance_permille(p))
            }
        })
    }

    fn eval_ent(&mut self, e: &Ex, f: &Frame) -> Result<Option<u32>, RtErr> {
        match self.eval(e, f)? {
            Val::Ent(x) => Ok(x),
            _ => Ok(None),
        }
    }

    fn int(&mut self, e: &Ex, f: &Frame) -> Result<i64, RtErr> {
        Ok(self.eval(e, f)?.int())
    }

    /// An expression in a behaviour or effect, where a run-time error just yields `default`
    /// (and is reported).
    fn int_or(&mut self, e: &Ex, f: &Frame, default: i64, what: &str) -> i64 {
        match self.eval(e, f) {
            Ok(v) => v.int(),
            Err(err) => {
                let m = format!("{what}: {}", err.message);
                self.note(err.code, &m);
                default
            }
        }
    }

    // ---- the tick ----

    fn tick(&mut self) {
        if self.st.outcome.is_some() {
            return;
        }
        // Each phase ends with its events reaching rules and a full outcome check; a decided
        // run skips the rest of the tick (but not cleanup).
        let phases: [fn(&mut Self); 8] = [
            Self::phase_waves,
            Self::phase_spawns,
            Self::phase_timers,
            |c| c.phase_behaviours(Phase::Spawn),
            |c| c.phase_behaviours(Phase::Move),
            |c| c.phase_behaviours(Phase::Act),
            Self::phase_projectiles,
            Self::phase_effects,
        ];
        for phase in phases {
            phase(self);
            if self.phase_end() {
                break;
            }
        }
        self.cleanup();
        self.st.tick += 1;
    }

    /// The end of a phase: queued events reach rules and machines, then every outcome is
    /// checked. True once the run is decided.
    fn phase_end(&mut self) -> bool {
        self.dispatch();
        self.check_outcomes(true);
        self.st.outcome.is_some()
    }

    /// After one entity's step: the outcomes that don't scan entities (resources, waves, the
    /// tick) are checked at once, so a loss stops the tick right after the step that caused
    /// it. True once the run is decided.
    fn decided(&mut self) -> bool {
        self.check_outcomes(false);
        self.st.outcome.is_some()
    }

    fn check_outcomes(&mut self, full: bool) {
        if self.st.outcome.is_some() || !(self.st.dirty || (full && self.st.dirty_full)) {
            return;
        }
        self.st.dirty = false;
        if full {
            self.st.dirty_full = false;
        }
        for o in &self.r.outcomes {
            if !full && !o.cheap {
                continue;
            }
            match self.eval(&o.when, &Frame::default()) {
                Ok(Val::Bool(true)) => {
                    self.st.outcome = Some((o.result.clone(), self.st.tick));
                    return;
                }
                Ok(_) => {}
                Err(e) => {
                    let m = format!("outcome '{}': {}", o.id, e.message);
                    self.note(e.code, &m);
                }
            }
        }
    }

    fn all_cleared(&self) -> bool {
        let Some(w) = &self.r.waves else { return false };
        usize::from(self.st.wave) == w.waves.len() && self.st.wave_stats.iter().all(|s| s.cleared_at.is_some())
    }

    fn phase_waves(&mut self) {
        if self.st.next_wave_tick.is_some_and(|t| self.st.tick >= t) {
            if let Err(e) = self.start_wave() {
                self.note(e.code, &e.message);
            }
        }
    }

    fn start_wave(&mut self) -> Result<(), RtErr> {
        let r = self.r;
        let Some(waves) = &r.waves else { return Err(rt("no_waves", "there is no waves item")) };
        let w = usize::from(self.st.wave);
        let Some(def) = waves.waves.get(w) else { return Err(rt("no_more_waves", "every wave has started")) };
        self.st.wave += 1;
        self.st.mark();
        let number = self.st.wave;
        self.emit(ev::WAVE_STARTED, None, None, i64::from(number));
        if let Err(e) = self.actions(&def.on_start, &Frame::default()) {
            let m = format!("wave {number} on_start: {}", e.message);
            self.note(e.code, &m);
        }
        // Started either way, so a wave skipped by the queue cap still clears (nothing pending).
        self.st.wave_stats[w].started_at = Some(self.st.tick);
        let total: usize = def.groups.iter().map(|g| g.count as usize).sum();
        if self.st.pending.len().saturating_add(total) > MAX_PENDING_SPAWNS {
            self.note("spawn_limit", &format!("wave {number} would queue {total} spawns on top of {} pending (most {MAX_PENDING_SPAWNS}); its groups were skipped", self.st.pending.len()));
        } else {
            self.schedule_wave(w, number);
        }
        self.st.next_wave_tick = def.duration.filter(|_| w + 1 < waves.waves.len()).map(|d| self.st.tick.saturating_add(d));
        Ok(())
    }

    /// Queues every spawn of wave `w`, keeping `pending` sorted latest first (merged, not
    /// re-sorted, so starting many waves stays linear).
    fn schedule_wave(&mut self, w: usize, number: u16) {
        let waves = self.r.waves.as_ref().expect("scheduling a wave");
        let def = &waves.waves[w];
        let mut fresh = Vec::new();
        for g in &def.groups {
            for k in 0..g.count {
                let pick = self.st.rngs[usize::from(waves.stream)].below(g.at.len() as u32) as usize;
                let tick = self.st.tick.saturating_add(g.start).saturating_add(k.saturating_mul(g.every));
                let seq = self.st.next_seq;
                self.st.next_seq = self.st.next_seq.saturating_add(1);
                fresh.push(Pending { tick, seq, arch: g.arch, wave: number, at: g.at[pick] });
                self.st.wave_stats[w].pending = self.st.wave_stats[w].pending.saturating_add(1);
            }
        }
        // Latest first, so the next spawn is always at the back.
        let key = |p: &Pending| std::cmp::Reverse((p.tick, p.seq));
        fresh.sort_by_key(key);
        let old = std::mem::take(&mut self.st.pending);
        let mut merged = Vec::with_capacity(old.len() + fresh.len());
        let (mut a, mut b) = (old.into_iter().peekable(), fresh.into_iter().peekable());
        while let (Some(x), Some(y)) = (a.peek(), b.peek()) {
            if key(x) <= key(y) {
                merged.push(a.next().expect("peeked"));
            } else {
                merged.push(b.next().expect("peeked"));
            }
        }
        merged.extend(a);
        merged.extend(b);
        self.st.pending = merged;
    }

    fn phase_spawns(&mut self) {
        while self.st.pending.last().is_some_and(|p| p.tick <= self.st.tick) {
            let p = self.st.pending.pop().expect("checked");
            let w = usize::from(p.wave) - 1;
            self.st.wave_stats[w].pending -= 1;
            let (pos, path, site) = match p.at {
                SpawnAt::Path(i) => (self.r.paths[usize::from(i)].points[0], Some((i, 0, 0)), None),
                SpawnAt::Site(s) => (self.r.sites[usize::from(s)].pos, None, Some(s)),
            };
            let scale = &self.r.waves.as_ref().expect("pending spawns come from waves").waves[w].scale;
            if let Err(e) = self.spawn(p.arch, pos, path, site, Some(p.wave), scale, &[]) {
                self.note(e.code, &e.message);
            }
            if self.decided() {
                return;
            }
        }
    }

    fn phase_timers(&mut self) {
        for i in 0..self.r.timers.len() {
            let def = &self.r.timers[i];
            let t = self.st.timers[i];
            let Some(next) = t.next else { continue };
            if self.st.tick < next {
                continue;
            }
            let go = match &def.when {
                None => true,
                Some(c) => self.eval(c, &Frame::default()).map(Val::bool).unwrap_or(false),
            };
            let mut fired = t.fired;
            if go {
                fired += 1;
                if let Err(e) = self.actions(&def.actions, &Frame::default()) {
                    let m = format!("timer '{}': {}", def.id, e.message);
                    self.note(e.code, &m);
                }
            }
            let more = def.count.is_none_or(|c| fired < c);
            self.st.timers[i] = TimerState { next: def.every.filter(|_| more).map(|e| next.saturating_add(e)), fired };
            if self.decided() {
                return;
            }
        }
    }

    fn beh_active(&self, i: usize, b: usize) -> bool {
        let e = &self.st.ents[i];
        let def = &self.r.archetypes[usize::from(e.arch)].behaviours[b];
        match (&def.active_in, e.state) {
            (None, _) => true,
            (Some(states), Some((s, _))) => states.contains(&s),
            (Some(_), None) => false,
        }
    }

    fn phase_behaviours(&mut self, phase: Phase) {
        let n = self.st.ents.len();
        for i in 0..n {
            if self.over_budget() {
                return;
            }
            if !self.st.ents[i].alive {
                continue;
            }
            let arch = usize::from(self.st.ents[i].arch);
            let nb = self.r.archetypes[arch].behaviours.len();
            for b in 0..nb {
                if !self.st.ents[i].alive || usize::from(self.st.ents[i].arch) != arch {
                    break;
                }
                if !self.beh_active(i, b) {
                    continue;
                }
                let kind = &self.r.archetypes[arch].behaviours[b].kind;
                let res = match (phase, kind) {
                    (Phase::Spawn, BehKind::Spawner { .. }) => self.spawner(i, b),
                    (Phase::Move, BehKind::FollowPath { .. } | BehKind::Patrol { .. }) => self.walk(i, b),
                    (Phase::Move, BehKind::Chase { .. }) => self.chase(i, b),
                    (Phase::Act, BehKind::Attack(_)) => self.attack(i, b),
                    (p, BehKind::Native { hook, args }) if self.r.natives[usize::from(*hook)].phase == p => {
                        let id = self.st.ents[i].id;
                        self.native(*hook, args, &Frame { me: Some(id), ..Frame::default() })
                    }
                    _ => Ok(()),
                };
                if let Err(e) = res {
                    let m = format!("entity {} ({}) {}: {}", self.st.ents[i].id, self.r.archetypes[arch].id, self.r.archetypes[arch].behaviours[b].name, e.message);
                    self.note(e.code, &m);
                }
            }
            if self.decided() {
                return;
            }
        }
    }

    fn spawner(&mut self, i: usize, b: usize) -> Result<(), RtErr> {
        let arch = usize::from(self.st.ents[i].arch);
        let BehKind::Spawner { arch: what, every, count, .. } = &self.r.archetypes[arch].behaviours[b].kind else { return Ok(()) };
        let BehState::Spawner { next } = self.st.ents[i].beh[b] else { return Ok(()) };
        if self.st.tick < next {
            return Ok(());
        }
        let id = self.st.ents[i].id;
        let f = Frame { me: Some(id), ..Frame::default() };
        let every = self.int(every, &f)?.clamp(1, i64::from(u32::MAX / 2)) as u32;
        let count = self.int(count, &f)?.clamp(0, MAX_SPAWN_COUNT);
        self.st.ents[i].beh[b] = BehState::Spawner { next: self.st.tick.saturating_add(every) };
        let (pos, path, wave) = self.continue_from(i);
        for _ in 0..count {
            self.spawn(*what, pos, path, None, wave, &[], &[])?;
        }
        Ok(())
    }

    /// Where an entity spawned "at" entity `i` starts: its position, continuing its path.
    fn continue_from(&self, i: usize) -> (P3, Option<(u16, u32, i64)>, Option<u16>) {
        let e = &self.st.ents[i];
        let path = e.beh.iter().find_map(|s| match s {
            BehState::Path { path: Some(p), seg, progress, done: false, .. } => Some((*p, *seg, *progress)),
            _ => None,
        });
        (e.pos, path, e.wave)
    }

    fn budget(&mut self, speed: i64, scale: i64, carry: &mut i64) -> i64 {
        let den = i64::from(self.r.tick_hz) * 1000;
        let num = speed.max(0).saturating_mul(scale.max(0)).saturating_add(*carry);
        *carry = num % den;
        num / den
    }

    fn walk(&mut self, i: usize, b: usize) -> Result<(), RtErr> {
        let arch = usize::from(self.st.ents[i].arch);
        let (speed_ex, scale_ex, pingpong, patrol) = match &self.r.archetypes[arch].behaviours[b].kind {
            BehKind::FollowPath { speed, scale, .. } => (speed, scale, false, false),
            BehKind::Patrol { speed, scale, pingpong, .. } => (speed, scale, *pingpong, true),
            _ => return Ok(()),
        };
        let BehState::Path { path, mut seg, mut progress, mut carry, mut fwd, done } = self.st.ents[i].beh[b].clone() else { return Ok(()) };
        let Some(p) = path else {
            return Err(rt("no_path", "follow_path with @spawn, but the entity wasn't spawned on a path"));
        };
        if done {
            return Ok(());
        }
        let id = self.st.ents[i].id;
        let f = Frame { me: Some(id), ..Frame::default() };
        let speed = self.int(speed_ex, &f)?;
        let scale = match scale_ex {
            Some(s) => self.int(s, &f)?,
            None => 1000,
        };
        let mut left = self.budget(speed, scale, &mut carry);
        let path_def = &self.r.paths[usize::from(p)];
        let nseg = path_def.seg.len() as u32;
        let mut arrived = false;
        // A few segments per tick at most; the bound only guards against degenerate paths.
        for _ in 0..64 {
            let len = path_def.seg[seg as usize];
            if fwd {
                if progress < len {
                    let adv = left.min(len - progress);
                    progress += adv;
                    left -= adv;
                    if progress < len {
                        break;
                    }
                }
                if seg + 1 < nseg {
                    seg += 1;
                    progress = 0;
                } else if patrol && pingpong {
                    fwd = false;
                } else if patrol || path_def.looped {
                    seg = 0;
                    progress = 0;
                } else {
                    arrived = true;
                    break;
                }
            } else {
                if progress > 0 {
                    let adv = left.min(progress);
                    progress -= adv;
                    left -= adv;
                    if progress > 0 {
                        break;
                    }
                }
                if seg > 0 {
                    seg -= 1;
                    progress = path_def.seg[seg as usize];
                } else {
                    fwd = true;
                }
            }
            if left == 0 {
                break;
            }
        }
        let len = path_def.seg[seg as usize];
        let pos = lerp(path_def.points[seg as usize], path_def.points[(seg as usize + 1) % path_def.points.len()], progress, len);
        let remaining = path_def.after[seg as usize].saturating_add(len.saturating_sub(progress));
        let e = &mut self.st.ents[i];
        e.pos = pos;
        e.path_remaining = if arrived { 0 } else { remaining };
        e.beh[b] = BehState::Path { path, seg, progress, carry, fwd, done: arrived };
        if arrived {
            self.emit(ev::ARRIVED, Some(id), None, 0);
            if let BehKind::FollowPath { on_arrive, .. } = &self.r.archetypes[arch].behaviours[b].kind {
                self.actions(on_arrive, &f)?;
            }
        }
        Ok(())
    }

    fn chase(&mut self, i: usize, b: usize) -> Result<(), RtErr> {
        let arch = usize::from(self.st.ents[i].arch);
        let BehKind::Chase { targets, range, speed, scale, reach, on_reach } = &self.r.archetypes[arch].behaviours[b].kind else { return Ok(()) };
        let BehState::Chase { target, mut carry, reached } = self.st.ents[i].beh[b] else { return Ok(()) };
        let id = self.st.ents[i].id;
        let f = Frame { me: Some(id), ..Frame::default() };
        let range = self.int(range, &f)?;
        let me_pos = self.st.ents[i].pos;
        let r2 = i128::from(range).saturating_mul(i128::from(range));
        let valid = |c: &Core, t: u32| idx_of(c.st, t).is_some_and(|j| {
            let e = &c.st.ents[j];
            e.alive && e.id != id && matches(c.r, c.st, e, targets) && dist2(e.pos, me_pos) <= r2
        });
        let mut target = target.filter(|t| valid(self, *t));
        if target.is_none() {
            let mut best: Option<(i128, u32)> = None;
            for j in 0..self.st.ents.len() {
                if self.spend(Self::check_cost(&self.st.ents[j], targets)) {
                    break;
                }
                let e = &self.st.ents[j];
                if e.alive && e.id != id && matches(self.r, self.st, e, targets) {
                    let d = dist2(e.pos, me_pos);
                    if d <= r2 && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, e.id));
                    }
                }
            }
            target = best.map(|(_, t)| t);
        }
        let Some(t) = target else {
            self.st.ents[i].beh[b] = BehState::Chase { target: None, carry: 0, reached: false };
            return Ok(());
        };
        let tpos = self.st.ents[idx_of(self.st, t).expect("valid")].pos;
        let speed = self.int(speed, &f)?;
        let scale = match scale {
            Some(s) => self.int(s, &f)?,
            None => 1000,
        };
        let reach = self.int(reach, &f)?;
        let step = self.budget(speed, scale, &mut carry);
        let to_go = dist(me_pos, tpos).saturating_sub(reach).max(0);
        let (pos, _) = step_toward(me_pos, tpos, step.min(to_go));
        self.st.ents[i].pos = pos;
        let now_reached = dist(pos, tpos) <= reach;
        self.st.ents[i].beh[b] = BehState::Chase { target: Some(t), carry, reached: now_reached };
        if now_reached && !reached {
            self.actions(on_reach, &Frame { target: Some(t), ..f })?;
        }
        Ok(())
    }

    /// Alive, not `me`, matching and in range; charged to the work budget (false once spent).
    fn eligible(&mut self, j: usize, me: u32, filter: &TagFilter, from: P3, r2: i128) -> bool {
        let e = &self.st.ents[j];
        if !e.alive || e.id == me {
            return false;
        }
        if self.spend(Self::check_cost(e, filter)) {
            return false;
        }
        let e = &self.st.ents[j];
        matches(self.r, self.st, e, filter) && dist2(from, e.pos) <= r2
    }

    fn attack(&mut self, i: usize, b: usize) -> Result<(), RtErr> {
        let arch = self.st.ents[i].arch;
        let BehKind::Attack(def) = &self.r.archetypes[usize::from(arch)].behaviours[b].kind else { return Ok(()) };
        let BehState::Attack { ready_at } = self.st.ents[i].beh[b] else { return Ok(()) };
        if self.st.tick < ready_at {
            return Ok(());
        }
        if def.blocked_by.iter().any(|t| has_tag(self.r, self.st, &self.st.ents[i], *t)) {
            return Ok(());
        }
        let id = self.st.ents[i].id;
        let f = Frame { me: Some(id), ..Frame::default() };
        let range = self.int(&def.range, &f)?;
        let r2 = i128::from(range).saturating_mul(i128::from(range));
        let from = self.st.ents[i].pos;
        let n = self.st.ents.len();
        if self.over_budget() {
            return Ok(());
        }
        let fired = match def.delivery {
            Delivery::Pulse => {
                let mut hit = false;
                for j in 0..n {
                    if self.over_budget() {
                        break;
                    }
                    if self.eligible(j, id, &def.targets, from, r2) {
                        hit = true;
                        let t = self.st.ents[j].id;
                        let vals = self.payload_values(def, &Frame { target: Some(t), ..f })?;
                        self.deliver(def, &vals, t, Some(id));
                    }
                }
                hit
            }
            Delivery::Instant | Delivery::Projectile => {
                let mut best: Option<(i128, i128, usize)> = None;
                for j in 0..n {
                    if self.over_budget() {
                        break;
                    }
                    if !self.eligible(j, id, &def.targets, from, r2) {
                        continue;
                    }
                    let e = &self.st.ents[j];
                    let rem = i128::from(e.path_remaining);
                    let hp = || arch_health(self.r, self.st, e);
                    let key = match def.policy {
                        Policy::First => (rem, 0),
                        Policy::Last => (-rem, 0),
                        Policy::Closest => (dist2(from, e.pos), 0),
                        Policy::Strongest => (-i128::from(hp()), rem),
                        Policy::Weakest => (i128::from(hp()), rem),
                    };
                    // Entities are in id order, so a strict `<` gives ties to the lowest id.
                    if best.is_none_or(|(k0, k1, _)| (key.0, key.1) < (k0, k1)) {
                        best = Some((key.0, key.1, j));
                    }
                }
                match best {
                    None => false,
                    Some((_, _, j)) => {
                        let t = self.st.ents[j].id;
                        let tf = Frame { target: Some(t), ..f };
                        let vals = self.payload_values(def, &tf)?;
                        if def.delivery == Delivery::Instant {
                            self.deliver(def, &vals, t, Some(id));
                        } else {
                            let (parch, with) = def.projectile.as_ref().expect("compiled with a projectile");
                            let mut with_vals = Vec::with_capacity(with.len());
                            for (a, e) in with {
                                with_vals.push((*a, self.int(e, &tf)?));
                            }
                            let origin = [from[0].saturating_add(def.muzzle[0]), from[1].saturating_add(def.muzzle[1]), from[2].saturating_add(def.muzzle[2])];
                            let aim = self.st.ents[j].pos;
                            let pid = self.spawn(*parch, origin, None, None, None, &[], &with_vals)?;
                            if let Some(pi) = idx_of(self.st, pid) {
                                if let Some(slot) = self.st.ents[pi].beh.iter_mut().find(|s| matches!(s, BehState::Projectile(_))) {
                                    *slot = BehState::Projectile(Box::new(Proj { target: Some(t), aim, payload: vals, attack: (arch, b as u16), source: Some(id), carry: 0 }));
                                }
                            }
                        }
                        true
                    }
                }
            }
        };
        if fired {
            let cd = self.int(&def.cooldown, &f)?.clamp(0, i64::from(u32::MAX / 2)) as u32;
            if let Some(i) = idx_of(self.st, id) {
                if let BehState::Attack { ready_at } = &mut self.st.ents[i].beh[b] {
                    *ready_at = self.st.tick.saturating_add(cd);
                }
            }
        }
        Ok(())
    }

    /// The payload's numbers, evaluated now in the attacker's frame.
    fn payload_values(&mut self, def: &AttackDef, f: &Frame) -> Result<Vec<(i64, i64)>, RtErr> {
        let mut out = Vec::with_capacity(def.payload.len());
        for p in &def.payload {
            out.push(match p {
                Payload::Damage(e) => (self.int(e, f)?, -1),
                Payload::Effect { effect, magnitude, duration } => {
                    let m = match magnitude {
                        Some(m) => self.int(m, f)?,
                        None => default_magnitude(&self.r.effects[usize::from(*effect)]),
                    };
                    let d = match duration {
                        Some(d) => self.int(d, f)?.max(0),
                        None => -1,
                    };
                    (m, d)
                }
            });
        }
        Ok(out)
    }

    /// One payload hit on `target`.
    fn deliver(&mut self, def: &AttackDef, vals: &[(i64, i64)], target: u32, source: Option<u32>) {
        if self.spend(1 + def.payload.len()) {
            return;
        }
        self.emit(ev::HIT, Some(target), source, 0);
        for (p, (v, d)) in def.payload.iter().zip(vals) {
            let res = match p {
                Payload::Damage(_) => self.damage(target, *v, source),
                Payload::Effect { effect, .. } => self.apply_effect(target, *effect, *v, (*d >= 0).then_some(*d), source),
            };
            if let Err(e) = res {
                self.note(e.code, &e.message);
            }
        }
    }

    fn phase_projectiles(&mut self) {
        let n = self.st.ents.len();
        for i in 0..n {
            if self.over_budget() {
                return;
            }
            if !self.st.ents[i].alive {
                continue;
            }
            let arch = usize::from(self.st.ents[i].arch);
            let Some(b) = self.r.archetypes[arch].behaviours.iter().position(|x| matches!(x.kind, BehKind::Projectile { .. })) else { continue };
            if let Err(e) = self.fly(i, b) {
                let m = format!("projectile {}: {}", self.st.ents[i].id, e.message);
                self.note(e.code, &m);
            }
            if self.decided() {
                return;
            }
        }
    }

    fn fly(&mut self, i: usize, b: usize) -> Result<(), RtErr> {
        let arch = usize::from(self.st.ents[i].arch);
        let BehKind::Projectile { speed, homing, splash } = &self.r.archetypes[arch].behaviours[b].kind else { return Ok(()) };
        let BehState::Projectile(p) = &self.st.ents[i].beh[b] else { return Ok(()) };
        let mut p = (**p).clone();
        let id = self.st.ents[i].id;
        if *homing {
            if let Some(t) = p.target {
                match idx_of(self.st, t) {
                    Some(j) if self.st.ents[j].alive => p.aim = self.st.ents[j].pos,
                    _ => p.target = None,
                }
            }
        }
        let f = Frame { me: Some(id), ..Frame::default() };
        let speed = self.int(speed, &f)?;
        let step = self.budget(speed, 1000, &mut p.carry);
        let (pos, arrived) = step_toward(self.st.ents[i].pos, p.aim, step);
        self.st.ents[i].pos = pos;
        if !arrived {
            self.st.ents[i].beh[b] = BehState::Projectile(Box::new(p));
            return Ok(());
        }
        // Impact: the projectile is gone either way.
        self.st.ents[i].alive = false;
        self.st.mark();
        let splash = self.int(splash, &f)?;
        let (sarch, sb) = p.attack;
        let BehKind::Attack(def) = &self.r.archetypes[usize::from(sarch)].behaviours[usize::from(sb)].kind else { return Ok(()) };
        if splash > 0 {
            let r2 = i128::from(splash).saturating_mul(i128::from(splash));
            let n = self.st.ents.len();
            for j in 0..n {
                if self.spend(Self::check_cost(&self.st.ents[j], &def.targets)) {
                    break;
                }
                let e = &self.st.ents[j];
                if e.alive && matches(self.r, self.st, e, &def.targets) && dist2(e.pos, p.aim) <= r2 {
                    let t = e.id;
                    self.deliver(def, &p.payload, t, p.source);
                }
            }
        } else if let Some(t) = p.target.filter(|t| idx_of(self.st, *t).is_some_and(|j| self.st.ents[j].alive)) {
            self.deliver(def, &p.payload, t, p.source);
        }
        Ok(())
    }

    fn phase_effects(&mut self) {
        // Native behaviours declared for this phase run first.
        if self.r.natives.iter().any(|n| n.phase == Phase::Effects) {
            self.phase_behaviours(Phase::Effects);
            if self.decided() {
                return;
            }
        }
        let n = self.st.ents.len();
        for i in 0..n {
            if self.over_budget() {
                return;
            }
            if !self.st.ents[i].alive {
                continue;
            }
            self.effects_of(i);
            if self.st.ents[i].alive {
                self.lifetimes(i);
            }
            if self.st.ents[i].alive {
                self.transitions(i, None);
            }
            if self.decided() {
                return;
            }
        }
    }

    fn effects_of(&mut self, i: usize) {
        let id = self.st.ents[i].id;
        let mut k = 0;
        while k < self.st.ents[i].effects.len() {
            let a = self.st.ents[i].effects[k].clone();
            let def = &self.r.effects[usize::from(a.effect)];
            let f = Frame { me: Some(id), other: a.source, magnitude: a.magnitude, ..Frame::default() };
            if let Some(per) = &def.period {
                if self.st.tick < a.expires && self.st.tick >= a.next_period {
                    let period = self.int_or(per, &f, 1, "effect period").max(1) as u32;
                    self.st.ents[i].effects[k].next_period = a.next_period.saturating_add(period);
                    for (m, v) in def.modifiers.iter().zip(&a.mods) {
                        for _ in 0..a.stacks {
                            self.modify_base(i, m.attr, m.op, *v, a.source);
                        }
                    }
                    if let Err(e) = self.actions(&def.on_period, &f) {
                        self.note(e.code, &e.message);
                    }
                    if !self.st.ents.get(i).is_some_and(|e| e.alive && e.id == id) {
                        return;
                    }
                }
            }
            if self.st.tick >= a.expires {
                let gone = self.st.ents[i].effects.remove(k);
                self.st.cells = self.st.cells.saturating_sub(effect_cells(gone.mods.len()));
                self.emit(ev::EFFECT_EXPIRED, Some(id), a.source, i64::from(a.effect));
                if let Err(e) = self.actions(&def.on_expire, &f) {
                    self.note(e.code, &e.message);
                }
                if !self.st.ents.get(i).is_some_and(|e| e.alive && e.id == id) {
                    return;
                }
                continue;
            }
            k += 1;
        }
    }

    fn lifetimes(&mut self, i: usize) {
        let arch = usize::from(self.st.ents[i].arch);
        let id = self.st.ents[i].id;
        for b in 0..self.st.ents[i].beh.len() {
            if let BehState::Lifetime { expires } = self.st.ents[i].beh[b] {
                if self.st.tick >= expires && self.beh_active(i, b) {
                    if let BehKind::Lifetime { on_expire, .. } = &self.r.archetypes[arch].behaviours[b].kind {
                        if let Err(e) = self.actions(on_expire, &Frame { me: Some(id), ..Frame::default() }) {
                            self.note(e.code, &e.message);
                        }
                    }
                    if let Some(j) = idx_of(self.st, id) {
                        self.despawn(j);
                    }
                    return;
                }
            }
        }
    }

    /// Polls the entity's machine (`event`: dispatching that event to it instead).
    fn transitions(&mut self, i: usize, event: Option<u16>) {
        let arch = usize::from(self.st.ents[i].arch);
        let (Some(m), Some((state, entered))) = (self.r.archetypes[arch].machine, self.st.ents[i].state) else { return };
        let mdef = &self.r.machines[usize::from(m)];
        let id = self.st.ents[i].id;
        let f = Frame { me: Some(id), ..Frame::default() };
        for t in &mdef.states[usize::from(state)].transitions {
            if t.on != event {
                continue;
            }
            if let Some(after) = &t.after {
                let need = self.int_or(after, &f, 0, "transition after");
                if i64::from(self.st.tick.saturating_sub(entered)) < need {
                    continue;
                }
            }
            if let Some(w) = &t.when {
                match self.eval(w, &f) {
                    Ok(Val::Bool(true)) => {}
                    Ok(_) => continue,
                    Err(e) => {
                        self.note(e.code, &e.message);
                        continue;
                    }
                }
            }
            self.goto(i, m, t.to);
            return;
        }
    }

    fn goto(&mut self, i: usize, m: u16, to: u16) {
        let id = self.st.ents[i].id;
        let mdef = &self.r.machines[usize::from(m)];
        let f = Frame { me: Some(id), ..Frame::default() };
        if let Some((from, _)) = self.st.ents[i].state {
            if let Err(e) = self.actions(&mdef.states[usize::from(from)].on_exit, &f) {
                self.note(e.code, &e.message);
            }
        }
        let Some(i) = idx_of(self.st, id) else { return };
        self.st.ents[i].state = Some((to, self.st.tick));
        self.emit(ev::STATE_ENTERED, Some(id), None, i64::from(to));
        if let Err(e) = self.actions(&mdef.states[usize::from(to)].on_enter, &f) {
            self.note(e.code, &e.message);
        }
    }

    /// Delivers queued events to rules and `on` transitions, oldest first.
    fn dispatch(&mut self) {
        while !self.st.queue.is_empty() {
            if self.over_budget() {
                // The budget note was made when it ran out; the queued events are dropped.
                self.st.queue.clear();
                return;
            }
            let batch = std::mem::take(&mut self.st.queue);
            for e in batch {
                self.st.dispatched += 1;
                if self.st.dispatched > MAX_EVENTS_PER_TICK {
                    self.note("event_storm", "more than 100000 events in one tick; the rest were not dispatched (rules raising events in a loop?)");
                    self.st.queue.clear();
                    return;
                }
                for rule in &self.r.rules {
                    if rule.on != e.kind {
                        continue;
                    }
                    let subject_ok = match e.subject.and_then(|s| idx_of(self.st, s)) {
                        Some(j) => {
                            if self.spend(Self::check_cost(&self.st.ents[j], &rule.tags)) {
                                self.st.queue.clear();
                                return;
                            }
                            matches(self.r, self.st, &self.st.ents[j], &rule.tags)
                        }
                        None => rule.tags.all.is_empty() && rule.tags.none.is_empty(),
                    };
                    if !subject_ok {
                        continue;
                    }
                    let f = Frame { me: e.subject, other: e.other, event: e.value, ..Frame::default() };
                    if let Some(w) = &rule.when {
                        match self.eval(w, &f) {
                            Ok(Val::Bool(true)) => {}
                            Ok(_) => continue,
                            Err(err) => {
                                let m = format!("rule '{}': {}", rule.id, err.message);
                                self.note(err.code, &m);
                                continue;
                            }
                        }
                    }
                    if let Err(err) = self.actions(&rule.actions, &f) {
                        let m = format!("rule '{}': {}", rule.id, err.message);
                        self.note(err.code, &m);
                    }
                }
                if let Some(j) = e.subject.and_then(|s| idx_of(self.st, s)) {
                    if self.st.ents[j].alive {
                        self.transitions(j, Some(e.kind));
                    }
                }
                self.st.mark();
            }
        }
    }

    fn cleanup(&mut self) {
        // Rules hear about this tick's deaths while the dead can still be read.
        self.dispatch();
        let freed: u64 = self
            .st
            .ents
            .iter()
            .filter(|e| !e.alive)
            .map(|e| entity_cells(&e.base, &e.beh) + e.effects.iter().map(|a| effect_cells(a.mods.len())).sum::<u64>())
            .sum();
        self.st.cells = self.st.cells.saturating_sub(freed);
        self.st.ents.retain(|e| e.alive);
        if self.st.outcome.is_some() {
            return;
        }
        for w in 0..self.st.wave_stats.len() {
            let s = self.st.wave_stats[w];
            if s.started_at.is_some() && s.cleared_at.is_none() && s.pending == 0 && s.alive == 0 {
                self.st.wave_stats[w].cleared_at = Some(self.st.tick);
                self.st.mark();
                self.emit(ev::WAVE_CLEARED, None, None, w as i64 + 1);
            }
        }
        self.dispatch();
        self.st.mark();
        self.check_outcomes(true);
    }

    // ---- entities ----

    #[allow(clippy::too_many_arguments)]
    fn spawn(&mut self, arch: u16, pos: P3, path: Option<(u16, u32, i64)>, site: Option<u16>, wave: Option<u16>, scale: &[(u16, i64)], with: &[(u16, i64)]) -> Result<u32, RtErr> {
        let def = &self.r.archetypes[usize::from(arch)];
        if def.abstract_ {
            return Err(rt("abstract", format!("archetype '{}' is abstract and can't be spawned", def.id)));
        }
        if self.st.ents.len() >= MAX_ENTITIES {
            return Err(rt("entity_limit", format!("{MAX_ENTITIES} entities are alive; spawn refused")));
        }
        let mut base: Vec<i64> = def.attrs.iter().map(|(_, b)| *b).collect();
        for (a, r) in scale {
            if let Some(s) = def.slot_of(*a) {
                base[s] = arith(Arith::MulRatio, base[s], *r).unwrap_or(base[s]);
            }
        }
        for (a, v) in with {
            if let Some(s) = def.slot_of(*a) {
                base[s] = *v;
            }
        }
        let id = self.st.next_id;
        self.st.next_id += 1;
        let tick = self.st.tick;
        let beh: Vec<BehState> = def.behaviours.iter().map(|b| initial_state(b, tick, path, false)).collect();
        let spawn_path = path.map(|(p, _, _)| p);
        let path_remaining = path.map_or(0, |(p, seg, progress)| {
            let pd = &self.r.paths[usize::from(p)];
            pd.after[seg as usize].saturating_add(pd.seg[seg as usize].saturating_sub(progress))
        });
        let state = def.machine.map(|m| (self.r.machines[usize::from(m)].initial, tick));
        let cells = entity_cells(&base, &beh);
        if self.st.cells.saturating_add(cells) > MAX_STATE_CELLS {
            self.st.next_id -= 1;
            return Err(rt("state_limit", format!("the run's state budget ({MAX_STATE_CELLS} cells: entities x their attributes and behaviours) is spent; spawn refused")));
        }
        self.st.cells += cells;
        let mut e = Entity { id, arch, alive: true, pos, base, effects: Vec::new(), beh, state, wave, site, spawn_path, path_remaining };
        // A walker spawned mid-path stands where its path says.
        if let Some((p, seg, progress)) = path {
            let pd = &self.r.paths[usize::from(p)];
            e.pos = lerp(pd.points[seg as usize], pd.points[(seg as usize + 1) % pd.points.len()], progress, pd.seg[seg as usize]);
        }
        self.st.ents.push(e);
        self.st.mark();
        if let Some(w) = wave {
            let s = &mut self.st.wave_stats[usize::from(w) - 1];
            s.spawned += 1;
            s.alive += 1;
        }
        let i = self.st.ents.len() - 1;
        self.init_timed(i);
        let f = Frame { me: Some(id), ..Frame::default() };
        self.emit(ev::SPAWNED, Some(id), None, 0);
        self.actions(&def.on_spawn, &f)?;
        if let (Some(m), Some(i)) = (def.machine, idx_of(self.st, id)) {
            let initial = self.r.machines[usize::from(m)].initial;
            self.emit(ev::STATE_ENTERED, Some(id), None, i64::from(initial));
            let _ = i;
            self.actions(&self.r.machines[usize::from(m)].states[usize::from(initial)].on_enter, &f)?;
        }
        Ok(id)
    }

    /// Spawner and lifetime timers, which need the entity to exist to evaluate.
    fn init_timed(&mut self, i: usize) {
        let arch = usize::from(self.st.ents[i].arch);
        let id = self.st.ents[i].id;
        let f = Frame { me: Some(id), ..Frame::default() };
        for b in 0..self.r.archetypes[arch].behaviours.len() {
            let fresh = matches!(self.st.ents[i].beh[b], BehState::Spawner { next: u32::MAX } | BehState::Lifetime { expires: u32::MAX });
            if !fresh {
                continue;
            }
            match &self.r.archetypes[arch].behaviours[b].kind {
                BehKind::Spawner { start, .. } => {
                    let s = self.int_or(start, &f, 0, "spawner start").clamp(0, i64::from(u32::MAX / 2)) as u32;
                    self.st.ents[i].beh[b] = BehState::Spawner { next: self.st.tick.saturating_add(s) };
                }
                BehKind::Lifetime { duration, .. } => {
                    let d = self.int_or(duration, &f, 0, "lifetime duration").clamp(0, i64::from(u32::MAX / 2)) as u32;
                    self.st.ents[i].beh[b] = BehState::Lifetime { expires: self.st.tick.saturating_add(d) };
                }
                _ => {}
            }
        }
    }

    fn die(&mut self, i: usize, killer: Option<u32>) {
        let e = &mut self.st.ents[i];
        if !e.alive {
            return;
        }
        e.alive = false;
        let (id, arch, wave) = (e.id, e.arch, e.wave);
        self.st.mark();
        if let Some(w) = wave {
            let s = &mut self.st.wave_stats[usize::from(w) - 1];
            s.died += 1;
            s.alive -= 1;
        }
        let f = Frame { me: Some(id), other: killer, ..Frame::default() };
        for b in &self.r.archetypes[usize::from(arch)].behaviours {
            if let BehKind::Health { on_death, .. } = &b.kind {
                if let Err(e) = self.actions(on_death, &f) {
                    self.note(e.code, &e.message);
                }
            }
        }
        self.emit(ev::DIED, Some(id), killer, 0);
    }

    fn despawn(&mut self, i: usize) {
        let e = &mut self.st.ents[i];
        if !e.alive {
            return;
        }
        e.alive = false;
        let (id, wave) = (e.id, e.wave);
        self.st.mark();
        if let Some(w) = wave {
            let s = &mut self.st.wave_stats[usize::from(w) - 1];
            s.removed += 1;
            s.alive -= 1;
        }
        self.emit(ev::REMOVED, Some(id), None, 0);
    }

    fn damage(&mut self, target: u32, amount: i64, source: Option<u32>) -> Result<(), RtErr> {
        let Some(i) = idx_of(self.st, target) else { return Ok(()) };
        if amount <= 0 || !self.st.ents[i].alive {
            return Ok(());
        }
        let arch = &self.r.archetypes[usize::from(self.st.ents[i].arch)];
        let Some(h) = arch.health else {
            return Err(rt("no_health", format!("entity {target} ({}) has no health behaviour, so it can't be damaged", arch.id)));
        };
        let slot = arch.slot_of(h).expect("health attribute is the archetype's own");
        let e = &mut self.st.ents[i];
        e.base[slot] = e.base[slot].saturating_sub(amount);
        self.emit(ev::DAMAGED, Some(target), source, amount);
        self.health_check(i, source);
        Ok(())
    }

    fn health_check(&mut self, i: usize, killer: Option<u32>) {
        let e = &self.st.ents[i];
        if !e.alive {
            return;
        }
        let Some(h) = self.r.archetypes[usize::from(e.arch)].health else { return };
        if attr_value(self.r, self.st, e, h).is_some_and(|v| v <= 0) {
            self.die(i, killer);
        }
    }

    fn modify_base(&mut self, i: usize, attr: u16, op: ModOp, v: i64, source: Option<u32>) {
        let arch = &self.r.archetypes[usize::from(self.st.ents[i].arch)];
        let Some(slot) = arch.slot_of(attr) else { return };
        let is_health = arch.health == Some(attr);
        let e = &mut self.st.ents[i];
        let before = e.base[slot];
        e.base[slot] = match op {
            ModOp::Add => before.saturating_add(v),
            ModOp::Mul => arith(Arith::MulRatio, before, v).unwrap_or(before),
            ModOp::Set => v,
        };
        self.st.mark();
        if is_health {
            let after = self.st.ents[i].base[slot];
            if after < before {
                let id = self.st.ents[i].id;
                self.emit(ev::DAMAGED, Some(id), source, before.saturating_sub(after));
            }
            self.health_check(i, source);
        }
    }

    fn apply_effect(&mut self, target: u32, effect: u16, magnitude: i64, duration: Option<i64>, source: Option<u32>) -> Result<(), RtErr> {
        let Some(i) = idx_of(self.st, target) else { return Ok(()) };
        if !self.st.ents[i].alive {
            return Ok(());
        }
        let def = &self.r.effects[usize::from(effect)];
        // Applying looks through the target's effects and evaluates each modifier.
        if self.spend(1 + def.modifiers.len() + self.st.ents[i].effects.len() * (1 + def.block_tags.len())) {
            return Ok(());
        }
        if def.block_tags.iter().any(|t| has_tag(self.r, self.st, &self.st.ents[i], *t)) {
            return Ok(());
        }
        let f = Frame { me: Some(target), other: source, magnitude, ..Frame::default() };
        let mut mods = Vec::with_capacity(def.modifiers.len());
        for m in &def.modifiers {
            mods.push(self.int(&m.value, &f)?);
        }
        let tick = self.st.tick;
        let ticks = match (&def.duration, duration) {
            (EffDur::Instant, _) => {
                for (m, v) in def.modifiers.iter().zip(&mods) {
                    self.modify_base(i, m.attr, m.op, *v, source);
                }
                self.emit(ev::EFFECT_APPLIED, Some(target), source, magnitude);
                return self.actions(&def.on_apply, &f);
            }
            (_, Some(d)) => d.clamp(0, i64::from(u32::MAX / 2)) as u32,
            (EffDur::Infinite, None) => u32::MAX,
            (EffDur::Ticks(e), None) => self.int(e, &f)?.clamp(0, i64::from(u32::MAX / 2)) as u32,
        };
        let expires = if ticks == u32::MAX { u32::MAX } else { tick.saturating_add(ticks) };
        let period = match &def.period {
            Some(p) => self.int(p, &f)?.clamp(1, i64::from(u32::MAX / 2)) as u32,
            None => 0,
        };
        // Expired instances are gone for stacking purposes.
        let mut freed = 0;
        self.st.ents[i].effects.retain(|a| {
            let keep = a.effect != effect || tick < a.expires;
            if !keep {
                freed += effect_cells(a.mods.len());
            }
            keep
        });
        self.st.cells = self.st.cells.saturating_sub(freed);
        let existing = self.st.ents[i].effects.iter().position(|a| a.effect == effect);
        let applied = match (def.stacking, existing) {
            (Stacking::Strongest, Some(k)) => {
                let a = &mut self.st.ents[i].effects[k];
                if magnitude >= a.magnitude {
                    a.magnitude = magnitude;
                    a.expires = a.expires.max(expires);
                    a.mods = mods;
                    true
                } else {
                    false
                }
            }
            (Stacking::Refresh, Some(k)) => {
                let a = &mut self.st.ents[i].effects[k];
                a.magnitude = magnitude;
                a.expires = expires;
                a.mods = mods;
                true
            }
            (Stacking::Stack, Some(k)) => {
                let a = &mut self.st.ents[i].effects[k];
                a.stacks = (a.stacks + 1).min(def.max_stacks);
                a.expires = expires;
                a.mods = mods;
                true
            }
            (Stacking::Ignore, Some(_)) => false,
            (Stacking::Independent, Some(_)) => {
                let count = self.st.ents[i].effects.iter().filter(|a| a.effect == effect).count();
                if count >= usize::from(def.max_stacks) {
                    if let Some(oldest) = self.st.ents[i].effects.iter().position(|a| a.effect == effect) {
                        let gone = self.st.ents[i].effects.remove(oldest);
                        self.st.cells = self.st.cells.saturating_sub(effect_cells(gone.mods.len()));
                    }
                }
                self.push_effect(i, effect, magnitude, expires, period, source, mods);
                true
            }
            (_, None) => {
                self.push_effect(i, effect, magnitude, expires, period, source, mods);
                true
            }
        };
        if applied {
            self.st.mark();
            self.emit(ev::EFFECT_APPLIED, Some(target), source, magnitude);
            self.actions(&def.on_apply, &f)?;
            if let Some(i) = idx_of(self.st, target) {
                self.health_check(i, source);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn push_effect(&mut self, i: usize, effect: u16, magnitude: i64, expires: u32, period: u32, source: Option<u32>, mods: Vec<i64>) {
        self.spend(1);
        if self.st.ents[i].effects.len() >= MAX_EFFECTS_PER_ENTITY {
            let (id, name) = (self.st.ents[i].id, &self.r.effects[usize::from(effect)].id);
            self.st.diag.note_with(self.st.tick, "effect_limit", || format!("entity {id} already carries {MAX_EFFECTS_PER_ENTITY} effects; '{name}' was not applied"));
            return;
        }
        let cells = effect_cells(mods.len());
        if self.st.cells.saturating_add(cells) > MAX_STATE_CELLS {
            self.note_with("state_limit", || "the run's state budget is spent; an effect was not applied".to_owned());
            return;
        }
        self.st.cells += cells;
        let seq = self.st.effect_seq;
        self.st.effect_seq += 1;
        let tick = self.st.tick;
        self.st.ents[i].effects.push(Active { effect, magnitude, applied: tick, expires, next_period: tick.saturating_add(period), stacks: 1, seq, source, mods });
    }

    fn become_(&mut self, i: usize, to: u16, keep: &[u16]) -> Result<(), RtErr> {
        let new = &self.r.archetypes[usize::from(to)];
        if new.abstract_ {
            return Err(rt("abstract", format!("archetype '{}' is abstract", new.id)));
        }
        let old_arch = self.st.ents[i].arch;
        let old = &self.r.archetypes[usize::from(old_arch)];
        let e = &self.st.ents[i];
        let mut base: Vec<i64> = new.attrs.iter().map(|(_, b)| *b).collect();
        for a in keep {
            if let (Some(os), Some(ns)) = (old.slot_of(*a), new.slot_of(*a)) {
                base[ns] = e.base[os];
            }
        }
        let tick = self.st.tick;
        let path = e.spawn_path.map(|p| (p, 0, 0));
        let beh: Vec<BehState> = new
            .behaviours
            .iter()
            .map(|nb| {
                match old.behaviours.iter().position(|ob| ob.name == nb.name && ob.kind.type_name() == nb.kind.type_name()) {
                    Some(k) => e.beh[k].clone(),
                    None => initial_state(nb, tick, path, false),
                }
            })
            .collect();
        let state = if new.machine == old.machine { e.state } else { new.machine.map(|m| (self.r.machines[usize::from(m)].initial, tick)) };
        let e = &mut self.st.ents[i];
        let (old_cells, new_cells) = (entity_cells(&e.base, &e.beh), entity_cells(&base, &beh));
        self.st.cells = self.st.cells.saturating_sub(old_cells).saturating_add(new_cells);
        e.arch = to;
        e.base = base;
        e.beh = beh;
        e.state = state;
        self.st.mark();
        self.init_timed(i);
        self.health_check(i, None);
        Ok(())
    }

    fn native(&mut self, hook: u16, args: &[(String, Ex)], f: &Frame) -> Result<(), RtErr> {
        let mut vals = Vec::with_capacity(args.len());
        for (k, e) in args {
            vals.push((k.clone(), self.int(e, f)?));
        }
        self.st.native_calls[usize::from(hook)] += 1;
        let Some(h) = self.natives.get(usize::from(hook)).and_then(|h| h.clone()) else {
            return Err(rt("native_unbound", format!("native hook '{}' is not provided", self.r.natives[usize::from(hook)].hook)));
        };
        let mut cx = NativeCtx { core: self, entity: f.me, args: &vals };
        h.run(&mut cx).map_err(|m| rt("native_failed", m))
    }

    // ---- actions and commands ----

    fn actions(&mut self, acts: &[Action], f: &Frame) -> Result<(), RtErr> {
        if acts.is_empty() {
            return Ok(());
        }
        if self.spend(acts.len()) {
            return Err(rt("work_limit", "the tick's work budget is spent; this action list was stopped"));
        }
        if self.depth >= MAX_ACTION_DEPTH {
            return Err(rt(
                "recursion_limit",
                format!("actions nested {MAX_ACTION_DEPTH} deep (an on_spawn that spawns itself, an on_apply that reapplies its effect, an on_enter that re-enters its state?); this action list was stopped"),
            ));
        }
        self.depth += 1;
        let r = acts.iter().try_for_each(|a| self.action(a, f));
        self.depth -= 1;
        r
    }

    fn set_res(&mut self, r: u16, v: i64) {
        let def = &self.r.resources[usize::from(r)];
        let v = v.clamp(def.min, def.max);
        let slot = &mut self.st.res[usize::from(r)];
        if *slot != v {
            *slot = v;
            self.st.mark();
        }
    }

    fn action(&mut self, a: &Action, f: &Frame) -> Result<(), RtErr> {
        match a {
            Action::AddRes { res, amount } => {
                let v = self.int(amount, f)?;
                let cur = self.st.res[usize::from(*res)];
                self.set_res(*res, cur.saturating_add(v));
            }
            Action::SetRes { res, value } => {
                let v = self.int(value, f)?;
                self.set_res(*res, v);
            }
            Action::Change { who, attr, op, value } => {
                let id = self.eval_ent(who, f)?;
                let i = self.ent_alive(id, "the entity to change")?;
                let v = self.int(value, f)?;
                let arch = &self.r.archetypes[usize::from(self.st.ents[i].arch)];
                let Some(_) = arch.slot_of(*attr) else {
                    return Err(rt("no_attribute", format!("entity {} ({}) has no attribute '{}'", self.st.ents[i].id, arch.id, self.r.attrs[usize::from(*attr)].name)));
                };
                let (mop, val) = match op {
                    ChangeOp::By => (ModOp::Add, v),
                    ChangeOp::To => (ModOp::Set, v),
                };
                self.modify_base(i, *attr, mop, val, f.me);
            }
            Action::Spawn { arch, at, count, with } => {
                let Val::Arch(Some(arch)) = self.eval(arch, f)? else { return Err(rt("no_archetype", "the archetype to spawn is none")) };
                let count = self.int(count, f)?.clamp(0, MAX_SPAWN_COUNT);
                let mut wv = Vec::with_capacity(with.len());
                for (a, e) in with {
                    wv.push((*a, self.int(e, f)?));
                }
                let inherit_wave = f.me.and_then(|m| idx_of(self.st, m)).and_then(|i| self.st.ents[i].wave);
                let (pos, path, site, wave) = match at {
                    At::Path(p) => (self.r.paths[usize::from(*p)].points[0], Some((*p, 0, 0)), None, inherit_wave),
                    At::Site(s) => {
                        let Val::Site(s) = self.eval(s, f)? else { return Err(rt("no_site", "the site is none")) };
                        (self.r.sites[usize::from(s)].pos, None, Some(s), inherit_wave)
                    }
                    At::Entity(e) => {
                        let id = self.eval_ent(e, f)?;
                        let i = self.ent(id, "the spawn position's entity")?;
                        let (pos, path, wave) = self.continue_from(i);
                        (pos, path, None, wave.or(inherit_wave))
                    }
                };
                for _ in 0..count {
                    self.spawn(arch, pos, path, site, wave, &[], &wv)?;
                }
            }
            Action::Despawn(e) => {
                let id = self.eval_ent(e, f)?;
                let i = self.ent(id, "the entity to despawn")?;
                self.despawn(i);
            }
            Action::Kill(e) => {
                let id = self.eval_ent(e, f)?;
                let i = self.ent(id, "the entity to kill")?;
                self.die(i, f.me);
            }
            Action::Damage { to, amount } => {
                let id = self.eval_ent(to, f)?;
                let v = self.int(amount, f)?;
                let id = id.ok_or_else(|| rt("no_entity", "the entity to damage is none"))?;
                self.damage(id, v, f.me)?;
            }
            Action::Apply { effect, to, magnitude, duration } => {
                let id = self.eval_ent(to, f)?.ok_or_else(|| rt("no_entity", "the entity to apply the effect to is none"))?;
                let m = match magnitude {
                    Some(m) => self.int(m, f)?,
                    None => default_magnitude(&self.r.effects[usize::from(*effect)]),
                };
                let d = match duration {
                    Some(d) => Some(self.int(d, f)?.max(0)),
                    None => None,
                };
                self.apply_effect(id, *effect, m, d, f.me)?;
            }
            Action::Emit { event, subject, value } => {
                let s = match subject {
                    Some(s) => self.eval_ent(s, f)?,
                    None => f.me,
                };
                let v = match value {
                    Some(v) => self.int(v, f)?,
                    None => 0,
                };
                self.emit(*event, s, f.me, v);
            }
            Action::Become { who, arch, keep } => {
                let id = self.eval_ent(who, f)?;
                let i = self.ent_alive(id, "the entity to change")?;
                let Val::Arch(Some(to)) = self.eval(arch, f)? else { return Err(rt("no_archetype", "the archetype to become is none (e.g. no next tier)")) };
                self.become_(i, to, keep)?;
            }
            Action::NextWave => self.start_wave()?,
            Action::Goto { who, machine, state } => {
                let id = self.eval_ent(who, f)?;
                let i = self.ent_alive(id, "the entity")?;
                if self.r.archetypes[usize::from(self.st.ents[i].arch)].machine != Some(*machine) {
                    return Err(rt("no_machine", format!("entity {} doesn't run machine '{}'", self.st.ents[i].id, self.r.machines[usize::from(*machine)].id)));
                }
                self.goto(i, *machine, *state);
            }
            Action::Outcome(result) => {
                if self.st.outcome.is_none() {
                    self.st.outcome = Some((result.clone(), self.st.tick));
                }
            }
            Action::If { cond, then, otherwise } => {
                if self.eval(cond, f)?.bool() {
                    self.actions(then, f)?;
                } else {
                    self.actions(otherwise, f)?;
                }
            }
            Action::Native { hook, args } => self.native(*hook, args, f)?,
        }
        Ok(())
    }

    fn command(&mut self, call: &CommandCall) -> Result<(), Rejection> {
        if let Some((r, t)) = &self.st.outcome {
            return Err(Rejection::new("game_over", format!("the run ended ({r} at tick {t}); every command is refused")));
        }
        let Some(def) = self.r.commands.get(usize::from(call.cmd)) else {
            return Err(Rejection::new("unknown_command", "no such command"));
        };
        let mut args = Vec::with_capacity(call.args.len());
        for (a, v) in def.args.iter().zip(&call.args) {
            args.push(match (v, &a.ty) {
                (ArgVal::Site(s), _) => Val::Site(*s),
                (ArgVal::Arch(x), _) => Val::Arch(Some(*x)),
                (ArgVal::Num(n), _) => Val::Int(*n),
                (ArgVal::Entity(id), ArgTy::Entity(filter)) => {
                    let ok = idx_of(self.st, *id).is_some_and(|i| self.st.ents[i].alive && matches(self.r, self.st, &self.st.ents[i], filter));
                    if !ok {
                        return Err(Rejection::new("no_entity", format!("arg '{}': entity {id} doesn't exist or doesn't match", a.name)).with("arg", Json::from(a.name.as_str())));
                    }
                    Val::Ent(Some(*id))
                }
                (ArgVal::EntityAt(s), ArgTy::Entity(filter)) => {
                    let found = self.st.ents.iter().find(|e| e.alive && e.site == Some(*s) && matches(self.r, self.st, e, filter)).map(|e| e.id);
                    match found {
                        Some(id) => Val::Ent(Some(id)),
                        None => {
                            return Err(Rejection::new("no_entity", format!("arg '{}': nothing matching stands on site '{}'", a.name, self.r.sites[usize::from(*s)].name))
                                .with("arg", Json::from(a.name.as_str())))
                        }
                    }
                }
                (other, _) => return Err(Rejection::new("wrong_type", format!("arg '{}' has the wrong kind of value: {other:?}", a.name))),
            });
        }
        let f = Frame { args: &args, ..Frame::default() };
        let mut costs = Vec::with_capacity(def.cost.len());
        for (res, e) in &def.cost {
            let need = self.int(e, &f).map_err(|e| Rejection::new(e.code, e.message))?;
            let have = self.st.res[usize::from(*res)];
            let name = &self.r.resources[usize::from(*res)].id;
            if need < 0 {
                return Err(Rejection::new("invalid_cost", format!("the {name} cost of '{}' came to {need}; a cost can't be negative (use an action to pay out)", def.id)).with("cost", Json::from(need)));
            }
            if have < need {
                return Err(Rejection::new(format!("not_enough_{name}"), format!("needs {need} {name}, has {have}"))
                    .with("need", Json::from(need))
                    .with("have", Json::from(have)));
            }
            costs.push((*res, need));
        }
        for req in &def.require {
            match self.eval(&req.cond, &f) {
                Ok(Val::Bool(true)) => {}
                Ok(_) => return Err(Rejection::new(req.code.as_str(), req.message.as_str())),
                Err(e) => return Err(Rejection::new(req.code.as_str(), format!("{} ({})", req.message, e.message))),
            }
        }
        // The rollback snapshot copies the state, so it costs that much, from the commands' own
        // budget (the tick's phases keep theirs).
        let cost = self.st.cells + self.st.pending.len() as u64 + self.st.ents.len() as u64;
        if self.spend(cost as usize) {
            return Err(Rejection::new("work_limit", "this tick's command budget is spent (many commands over a large state); send the rest next tick"));
        }
        let snapshot = self.st.clone();
        for (res, need) in costs {
            let cur = self.st.res[usize::from(res)];
            self.set_res(res, cur.saturating_sub(need));
        }
        if let Err(e) = self.actions(&def.actions, &f) {
            *self.st = snapshot;
            return Err(Rejection::new("runtime_error", format!("command '{}' failed and was rolled back: {}", def.id, e.message)).with("cause", Json::from(e.code)));
        }
        self.dispatch();
        self.check_outcomes(true);
        Ok(())
    }
}

/// An entity's share of the state budget.
fn entity_cells(base: &[i64], beh: &[BehState]) -> u64 {
    1 + base.len() as u64 + beh.len() as u64
}

/// An effect instance's share of the state budget (it is about as big as an entity's few cells).
fn effect_cells(mods: usize) -> u64 {
    4 + mods as u64
}

fn arch_health(r: &Rules, st: &State, e: &Entity) -> i64 {
    r.archetypes[usize::from(e.arch)].health.and_then(|h| attr_value(r, st, e, h)).unwrap_or(0)
}

fn default_magnitude(def: &EffectDef) -> i64 {
    if def.magnitude_unit == crate::units::Unit::Ratio {
        1000
    } else {
        0
    }
}

fn initial_state(b: &Beh, _tick: u32, path: Option<(u16, u32, i64)>, _fresh: bool) -> BehState {
    match &b.kind {
        BehKind::FollowPath { path: sel, .. } => {
            let (p, seg, progress) = match (sel, path) {
                (PathSel::Spawn, Some((p, s, pr))) => (Some(p), s, pr),
                (PathSel::Spawn, None) => (None, 0, 0),
                (PathSel::Fixed(p), Some((sp, s, pr))) if *p == sp => (Some(*p), s, pr),
                (PathSel::Fixed(p), _) => (Some(*p), 0, 0),
            };
            BehState::Path { path: p, seg, progress, carry: 0, fwd: true, done: false }
        }
        BehKind::Patrol { path, .. } => BehState::Path { path: Some(*path), seg: 0, progress: 0, carry: 0, fwd: true, done: false },
        BehKind::Chase { .. } => BehState::Chase { target: None, carry: 0, reached: false },
        BehKind::Attack(_) => BehState::Attack { ready_at: _tick },
        BehKind::Spawner { .. } => BehState::Spawner { next: u32::MAX },
        BehKind::Lifetime { .. } => BehState::Lifetime { expires: u32::MAX },
        BehKind::Projectile { .. } => BehState::Projectile(Box::new(Proj { target: None, aim: [0; 3], payload: Vec::new(), attack: (0, 0), source: None, carry: 0 })),
        BehKind::Health { .. } | BehKind::Native { .. } => BehState::None,
    }
}

/// Resolves a command's JSON arguments.
pub fn parse_command(r: &Rules, name: &str, args: &Json) -> Result<CommandCall, Rejection> {
    let Some(cmd) = r.command(name) else {
        let names: Vec<&str> = r.commands.iter().map(|c| c.id.as_str()).collect();
        return Err(Rejection::new("unknown_command", format!("there is no command '{name}'"))
            .with("commands", Json::arr(names.iter().map(|n| Json::from(*n))))
            .with("suggestion", Json::from(crate::diag::did_you_mean(name, names.iter().copied()).unwrap_or_default())));
    };
    let def = &r.commands[usize::from(cmd)];
    let empty = Vec::new();
    let pairs = match args {
        Json::Null => &empty,
        Json::Obj(p) => p,
        _ => return Err(Rejection::new("wrong_type", "command args must be an object")),
    };
    for (k, _) in pairs {
        if !def.args.iter().any(|a| &a.name == k) {
            let names: Vec<&str> = def.args.iter().map(|a| a.name.as_str()).collect();
            return Err(Rejection::new("unknown_arg", format!("command '{name}' has no arg '{k}'"))
                .with("args", Json::arr(names.iter().map(|n| Json::from(*n))))
                .with("suggestion", Json::from(crate::diag::did_you_mean(k, names.iter().copied()).unwrap_or_default())));
        }
    }
    let mut out = Vec::with_capacity(def.args.len());
    for a in &def.args {
        let Some(v) = args.get(&a.name) else {
            return Err(Rejection::new("missing_arg", format!("command '{name}' needs arg '{}'", a.name)).with("arg", Json::from(a.name.as_str())));
        };
        let bad = |what: &str| Rejection::new("wrong_type", format!("arg '{}' must be {what}", a.name)).with("arg", Json::from(a.name.as_str()));
        out.push(match &a.ty {
            ArgTy::Site => {
                let s = v.as_str().ok_or_else(|| bad("a site name"))?;
                ArgVal::Site(r.site(s).ok_or_else(|| {
                    Rejection::new("unknown_site", format!("there is no site '{s}'"))
                        .with("arg", Json::from(a.name.as_str()))
                        .with("suggestion", Json::from(crate::diag::did_you_mean(s, r.sites.iter().map(|x| x.name.as_str())).unwrap_or_default()))
                })?)
            }
            ArgTy::Archetype(filter) => {
                let s = v.as_str().ok_or_else(|| bad("an archetype id"))?;
                let Some(x) = r.archetype(s) else {
                    return Err(Rejection::new("unknown_archetype", format!("there is no archetype '{s}'"))
                        .with("arg", Json::from(a.name.as_str()))
                        .with("suggestion", Json::from(crate::diag::did_you_mean(s, r.archetypes.iter().map(|x| x.id.as_str())).unwrap_or_default())));
                };
                let def = &r.archetypes[usize::from(x)];
                let tags_ok = filter.all.iter().all(|t| def.tags.binary_search(t).is_ok()) && !filter.none.iter().any(|t| def.tags.binary_search(t).is_ok());
                if !tags_ok || def.abstract_ {
                    return Err(Rejection::new("unknown_archetype", format!("archetype '{s}' isn't allowed for arg '{}'", a.name)).with("arg", Json::from(a.name.as_str())));
                }
                ArgVal::Arch(x)
            }
            ArgTy::Entity(_) => match v {
                Json::Num(_) => ArgVal::Entity(v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| bad("an entity id or a site name"))?),
                Json::Str(s) => ArgVal::EntityAt(r.site(s).ok_or_else(|| {
                    Rejection::new("unknown_site", format!("there is no site '{s}'"))
                        .with("arg", Json::from(a.name.as_str()))
                        .with("suggestion", Json::from(crate::diag::did_you_mean(s, r.sites.iter().map(|x| x.name.as_str())).unwrap_or_default()))
                })?),
                _ => return Err(bad("an entity id or a site name")),
            },
            ArgTy::Num(unit) => {
                let d = match v {
                    Json::Num(n) => crate::units::dec_from_f64(*n),
                    _ => None,
                }
                .ok_or_else(|| bad("a number"))?;
                let s = crate::units::scale(d, unit.per_human(r.tick_hz), 1).ok_or_else(|| bad("a smaller number"))?;
                ArgVal::Num(s.value)
            }
        });
    }
    Ok(CommandCall { cmd, args: out })
}

/// What a native hook may touch: deterministic, narrow, no clock or I/O.
pub struct NativeCtx<'c, 'a> {
    core: &'c mut Core<'a>,
    entity: Option<u32>,
    args: &'c [(String, i64)],
}

impl NativeCtx<'_, '_> {
    pub fn tick(&self) -> u32 {
        self.core.st.tick
    }

    pub fn tick_hz(&self) -> u32 {
        self.core.r.tick_hz
    }

    /// The entity the hook runs for (behaviours; actions run for their `self`).
    pub fn entity(&self) -> Option<u32> {
        self.entity
    }

    pub fn arg(&self, name: &str) -> Option<i64> {
        self.args.iter().find(|(k, _)| k == name).map(|(_, v)| *v)
    }

    fn attr_id(&self, name: &str) -> Option<u16> {
        self.core.r.attrs.iter().position(|a| a.name == name).map(|i| i as u16)
    }

    /// An entity's current attribute value (stored units).
    pub fn attr(&self, id: u32, name: &str) -> Option<i64> {
        let a = self.attr_id(name)?;
        let i = idx_of(self.core.st, id)?;
        attr_value(self.core.r, self.core.st, &self.core.st.ents[i], a)
    }

    /// Sets an entity's base attribute value.
    pub fn set_attr(&mut self, id: u32, name: &str, value: i64) -> Result<(), String> {
        let a = self.attr_id(name).ok_or_else(|| format!("no attribute '{name}'"))?;
        let i = idx_of(self.core.st, id).ok_or_else(|| format!("no entity {id}"))?;
        if self.core.r.archetypes[usize::from(self.core.st.ents[i].arch)].slot_of(a).is_none() {
            return Err(format!("entity {id} has no attribute '{name}'"));
        }
        self.core.modify_base(i, a, ModOp::Set, value, self.entity);
        Ok(())
    }

    pub fn pos(&self, id: u32) -> Option<P3> {
        idx_of(self.core.st, id).map(|i| self.core.st.ents[i].pos)
    }

    pub fn set_pos(&mut self, id: u32, pos: P3) -> Result<(), String> {
        let i = idx_of(self.core.st, id).ok_or_else(|| format!("no entity {id}"))?;
        self.core.st.ents[i].pos = pos;
        Ok(())
    }

    /// Sets the distance left that targeting policies (first, last) sort by.
    pub fn set_path_remaining(&mut self, id: u32, mm: i64) -> Result<(), String> {
        let i = idx_of(self.core.st, id).ok_or_else(|| format!("no entity {id}"))?;
        self.core.st.ents[i].path_remaining = mm;
        Ok(())
    }

    pub fn has_tag(&self, id: u32, tag: &str) -> bool {
        let (Some(t), Some(i)) = (self.core.r.tag(tag), idx_of(self.core.st, id)) else { return false };
        has_tag(self.core.r, self.core.st, &self.core.st.ents[i], t)
    }

    pub fn resource(&self, name: &str) -> Option<i64> {
        self.core.r.resource(name).map(|r| self.core.st.res[usize::from(r)])
    }

    pub fn add_resource(&mut self, name: &str, amount: i64) -> Result<(), String> {
        let r = self.core.r.resource(name).ok_or_else(|| format!("no resource '{name}'"))?;
        let cur = self.core.st.res[usize::from(r)];
        self.core.set_res(r, cur.saturating_add(amount));
        Ok(())
    }

    /// Uniform in `lo..=hi` from a named stream the rules declared (by using it).
    pub fn rand(&mut self, stream: &str, lo: i32, hi: i32) -> Result<i32, String> {
        let s = self.core.r.streams.iter().position(|x| x == stream).ok_or_else(|| format!("no random stream '{stream}' (streams exist once an expression uses them)"))?;
        Ok(self.core.st.rngs[s].range_i32(lo.min(hi), lo.max(hi)))
    }

    pub fn spawn(&mut self, archetype: &str, pos: P3) -> Result<u32, String> {
        let a = self.core.r.archetype(archetype).ok_or_else(|| format!("no archetype '{archetype}'"))?;
        self.core.spawn(a, pos, None, None, None, &[], &[]).map_err(|e| e.message)
    }

    pub fn despawn(&mut self, id: u32) -> Result<(), String> {
        let i = idx_of(self.core.st, id).ok_or_else(|| format!("no entity {id}"))?;
        self.core.despawn(i);
        Ok(())
    }

    pub fn kill(&mut self, id: u32) -> Result<(), String> {
        let i = idx_of(self.core.st, id).ok_or_else(|| format!("no entity {id}"))?;
        self.core.die(i, self.entity);
        Ok(())
    }

    pub fn emit(&mut self, event: &str, subject: Option<u32>, value: i64) -> Result<(), String> {
        let e = self.core.r.event(event).ok_or_else(|| format!("no event '{event}'"))?;
        self.core.emit(e, subject, self.entity, value);
        Ok(())
    }

    /// Live entities with a tag, by id.
    pub fn with_tag(&self, tag: &str) -> Vec<u32> {
        let Some(t) = self.core.r.tag(tag) else { return Vec::new() };
        self.core.st.ents.iter().filter(|e| e.alive && has_tag(self.core.r, self.core.st, e, t)).map(|e| e.id).collect()
    }
}

/// The state hash: a fixed-order walk of everything that changes.
fn state_hash(r: &Rules, st: &State) -> u64 {
    let mut h = StableHasher::with_seed(0x1e8_5ade);
    h.write_u64(r.hash);
    h.write_u64(st.seed);
    h.write_u32(st.tick);
    match &st.outcome {
        Some((s, t)) => {
            h.write_bool(true);
            h.write_str(s);
            h.write_u32(*t);
        }
        None => h.write_bool(false),
    }
    for v in &st.res {
        h.write_i64(*v);
    }
    for g in &st.rngs {
        let (a, b) = g.to_parts();
        h.write_u64(a);
        h.write_u64(b);
    }
    h.write_u32(st.next_id);
    h.write_u32(u32::from(st.wave));
    h.write_bool(st.next_wave_tick.is_some());
    h.write_u32(st.next_wave_tick.unwrap_or(0));
    for s in &st.wave_stats {
        for v in [s.started_at, s.cleared_at] {
            h.write_bool(v.is_some());
            h.write_u32(v.unwrap_or(0));
        }
        for v in [s.spawned, s.died, s.removed, s.pending, s.alive] {
            h.write_u32(v);
        }
    }
    h.write_u64(st.pending.len() as u64);
    for p in &st.pending {
        h.write_u32(p.tick);
        h.write_u32(p.seq);
        h.write_u32(u32::from(p.arch));
        h.write_u32(u32::from(p.wave));
        match p.at {
            SpawnAt::Path(x) => {
                h.write_u8(0);
                h.write_u32(u32::from(x));
            }
            SpawnAt::Site(x) => {
                h.write_u8(1);
                h.write_u32(u32::from(x));
            }
        }
    }
    h.write_u32(st.next_seq);
    for t in &st.timers {
        h.write_bool(t.next.is_some());
        h.write_u32(t.next.unwrap_or(0));
        h.write_u32(t.fired);
    }
    h.write_u32(st.effect_seq);
    h.write_u64(st.ents.len() as u64);
    for e in &st.ents {
        h.write_u32(e.id);
        h.write_u32(u32::from(e.arch));
        h.write_bool(e.alive);
        for c in e.pos {
            h.write_i64(c);
        }
        h.write_u64(e.base.len() as u64);
        for b in &e.base {
            h.write_i64(*b);
        }
        h.write_u64(e.effects.len() as u64);
        for a in &e.effects {
            h.write_u32(u32::from(a.effect));
            h.write_i64(a.magnitude);
            h.write_u32(a.applied);
            h.write_u32(a.expires);
            h.write_u32(a.next_period);
            h.write_u32(u32::from(a.stacks));
            h.write_u32(a.seq);
            h.write_u32(a.source.unwrap_or(0));
            for m in &a.mods {
                h.write_i64(*m);
            }
        }
        for s in &e.beh {
            match s {
                BehState::None => h.write_u8(0),
                BehState::Path { path, seg, progress, carry, fwd, done } => {
                    h.write_u8(1);
                    h.write_u32(path.map_or(u32::MAX, u32::from));
                    h.write_u32(*seg);
                    h.write_i64(*progress);
                    h.write_i64(*carry);
                    h.write_bool(*fwd);
                    h.write_bool(*done);
                }
                BehState::Chase { target, carry, reached } => {
                    h.write_u8(2);
                    h.write_u32(target.unwrap_or(0));
                    h.write_i64(*carry);
                    h.write_bool(*reached);
                }
                BehState::Attack { ready_at } => {
                    h.write_u8(3);
                    h.write_u32(*ready_at);
                }
                BehState::Projectile(p) => {
                    h.write_u8(4);
                    h.write_u32(p.target.unwrap_or(0));
                    for c in p.aim {
                        h.write_i64(c);
                    }
                    for (a, b) in &p.payload {
                        h.write_i64(*a);
                        h.write_i64(*b);
                    }
                    h.write_u32(u32::from(p.attack.0));
                    h.write_u32(u32::from(p.attack.1));
                    h.write_u32(p.source.unwrap_or(0));
                    h.write_i64(p.carry);
                }
                BehState::Spawner { next } => {
                    h.write_u8(5);
                    h.write_u32(*next);
                }
                BehState::Lifetime { expires } => {
                    h.write_u8(6);
                    h.write_u32(*expires);
                }
            }
        }
        match e.state {
            Some((s, t)) => {
                h.write_u32(u32::from(s));
                h.write_u32(t);
            }
            None => h.write_u32(u32::MAX),
        }
        h.write_u32(e.wave.map_or(0, u32::from));
        h.write_u32(e.site.map_or(u32::MAX, u32::from));
        h.write_u32(e.spawn_path.map_or(u32::MAX, u32::from));
        h.write_i64(e.path_remaining);
    }
    h.finish()
}
