//! The module host: game logic as hot-reloaded Rust libraries (see `docs/design/MODULES.md`).
//!
//! The engine owns all state. A module is code the [`LogicHost`] calls through the SDK's C ABI
//! (`specula_logic::ModuleCode`) at tick boundaries:
//! - **load / reload** ([`LogicHost::load`]) is a transaction: the new build is described, the
//!   old one stops, the new one starts, and the result is committed (claims checked, journaled).
//!   If the new build fails to start or a claim refuses its changes, everything is rolled back
//!   and the old build keeps running: a broken build never replaces a working one. The world,
//!   the module's state store and its RNG positions carry over, because none of them live in the
//!   module.
//! - **tick** ([`LogicHost::tick`]): every running module in `(order, name)` order, each with its
//!   input actions for the tick and the events of the previous one.
//! - **failure:** a call that fails changes nothing (world changes, state writes, RNG draws and
//!   events are rolled back; the ids it used are noted, never reused); a panic or a runaway
//!   (too many requests in one call) also disables the module, with a structured error.
//! - **hooks** ([`LogicHost::call_hook`]): named native behaviours modules declare, the escape
//!   hatch for Lex; every use is logged with its caller and reason.
//!
//! Each call's world changes go through a [`Commit`]: the engine's ticks journal them as
//! `module:<name>` (like a hosted game's, exempt from claims: modules are the game's rules);
//! loads, unloads and hooks an agent asks for are checked against other agents' claims as that
//! agent's (see `tools::handle`). Each load, reload, unload and disable is recorded with its tick,
//! generation and build hash (DETERMINISM.md rule 8) as a [`ModuleEvent`] the engine journals
//! ([`LogicHost::take_module_events`]).
//!
//! **Floating-point environment (rule 6).** Every call is guarded (`ModuleCode::call`): a module
//! that changes the rounding mode or flush-to-zero gets it put back before the engine answers its
//! requests and when it returns, and the change is counted against it ([`LogicHost::fp_log`],
//! `logic_status`'s `fp_restored`).

pub mod character;
mod handler;
pub mod input;
pub mod proposals;
mod save;
pub use save::LogicSnapshot;
pub mod tools;

use crate::write_policy::WorldWrite;
use crate::fpguard::FpLog;
use crate::journal::ModuleEvent;
use crate::world::{Mark, WorldError};
use character::{CharacterMover, Kinematic};
use handler::ModuleHandler;
use input::ModuleInput;
use specula_core::rng::Rng;
use specula_core::Json;
use specula_logic::abi::{entry, status};
use specula_logic::ModuleCode;
use specula_platform::Event;
use specula_sdk::Value;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Most modules loaded at once.
pub const MAX_MODULES: usize = 32;
/// Most requests one module call may make. A call that makes more fails and disables its module
/// (a deterministic cutoff, no wall clock): at roughly 2-4 µs a request, a runaway costs one frame
/// at most a few tens of milliseconds, once. ambulo makes about 8 a tick.
pub const MAX_REQUESTS_PER_CALL: u64 = 10_000;
/// Log lines, loads, hook uses and events kept for `logic_status`.
pub(crate) const LOG_LINES: usize = 256;
const LOADS_KEPT: usize = 64;
const HOOK_USES_KEPT: usize = 256;
const EVENTS_KEPT: usize = 64;
/// Most events waiting for the next tick (emits past it are refused: `too_many_events`).
pub(crate) const MAX_PENDING_EVENTS: usize = 1024;
/// Event data shown by `logic_status` up to this many encoded bytes (else only its size).
const SHOWN_EVENT_BYTES: usize = 256;

/// What a module call was, for journaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Start,
    Tick,
    Hook,
    Stop,
}

/// Commits the world changes a module call made since `mark` (`agent` is `module:<name>`):
/// journals them, after checking them against claims where an agent asked for the call. `Err`
/// refuses them; the world must then be back at `mark` (the host makes sure) and the host undoes
/// the rest of the call. Called after a failed call too, with nothing pending, so the journal
/// learns about ids the failed call used.
pub type Commit<'a> = dyn FnMut(&mut dyn WorldWrite, Mark, &str, Phase) -> Result<(), WorldError> + 'a;

/// Where a module came from: rebuilt with `logic_build`, reloaded with `logic_reload`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Origin {
    /// The crate folder (`logic_build`'s `path`).
    pub dir: Option<PathBuf>,
    /// The library loaded (the original, not the versioned copy).
    pub library: Option<PathBuf>,
}

/// A native hook a module declared.
#[derive(Clone, Debug, PartialEq)]
pub struct HookInfo {
    pub module: String,
    pub name: String,
    pub doc: String,
    /// JSON Schema of the arguments (`null` if none given).
    pub args: Json,
}

impl HookInfo {
    /// `module/name`: how callers name it.
    pub fn qualified(&self) -> String {
        format!("{}/{}", self.module, self.name)
    }

    pub fn to_json(&self) -> Json {
        Json::obj([("hook", Json::from(self.qualified())), ("doc", Json::from(self.doc.as_str())), ("args", self.args.clone())])
    }
}

/// What a module said about itself (`describe`).
#[derive(Clone, Debug)]
struct Info {
    name: String,
    version: String,
    description: String,
    order: i32,
    sdk: String,
    hooks: Vec<HookInfo>,
}

/// The engine's event a module hears for each press of one of its actions since the last tick,
/// in order (`action`: the action's name).
pub const PRESS_EVENT: &str = "press";

#[derive(Clone, Debug)]
pub(crate) struct EventRec {
    pub name: String,
    pub data: Value,
    pub from: String,
    pub tick: u64,
}

impl EventRec {
    fn to_value(&self) -> Value {
        Value::map().with("name", self.name.as_str()).with("data", self.data.clone()).with("from", self.from.as_str()).with("tick", self.tick)
    }

    /// For `logic_status`: small data in full, big data as its size.
    fn to_status_json(&self) -> Json {
        let bytes = self.data.encode().len();
        let data = if bytes <= SHOWN_EVENT_BYTES { self.data.to_json() } else { Json::obj([("omitted_bytes", Json::from(bytes))]) };
        Json::obj([("name", Json::from(self.name.as_str())), ("data", data), ("from", Json::from(self.from.as_str())), ("tick", Json::from(self.tick))])
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LogLine {
    pub tick: u64,
    pub module: String,
    pub message: String,
}

/// A sound a module asked for ([`specula_sdk::Cx::sound`]): presentation only, played by the
/// engine after the call commits (`crate::module_sounds`). Never part of the simulation's state:
/// not hashed, not saved, never delivered to modules.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundCue {
    pub module: String,
    /// `[a-z][a-z0-9_]{0,63}`: a recipe `sounds/<name>.json`.
    pub name: String,
    /// World position (spatial), or flat.
    pub at: Option<[f32; 3]>,
    /// 0..4.
    pub gain: f32,
    pub tick: u64,
}

/// Sounds asked for and not yet taken by the engine: at most this many (more are dropped and
/// counted; a sound is presentation, so dropping one never changes a run).
pub const MAX_PENDING_SOUNDS: usize = 64;

/// What the engine keeps for a module across reloads (and unloads, unless forgotten).
#[derive(Clone, Default)]
pub(crate) struct Shared {
    pub store: BTreeMap<String, Value>,
    /// Encoded size of `store` (keys + values).
    pub store_bytes: usize,
    /// RNG streams by name (at most [`MAX_RNG_STREAMS`]). Changed only through
    /// [`Shared::draw`], [`Shared::put_rng`] and [`Shared::clear_rngs`], which keep `rng_sum`.
    rngs: BTreeMap<String, Rng>,
    /// The streams' hash, kept as they change: the wrapping sum of each stream's own hash
    /// ([`stream_hash`]), so the desync detector's per-tick hash never walks every stream.
    rng_sum: u64,
    pub seed: u64,
    /// Loads so far: the next load's generation is this + 1.
    pub loads: u64,
    /// The store's hash for the desync detector, until the store changes (see `save`).
    pub store_hash: std::cell::Cell<Option<u64>>,
}

/// Most parameters one run is given.
pub const MAX_PARAMS: usize = 32;

/// Most RNG streams one module keeps: a new name past it is refused with `rng_full`. Save games
/// write at most this many (the handler allows no more) and read at most this many, so a save
/// always loads.
pub const MAX_RNG_STREAMS: usize = 1024;

/// One stream's share of [`Shared`]'s `rng_sum`.
fn stream_hash(stream: &str, rng: &Rng) -> u64 {
    let (a, b) = rng.to_parts();
    let mut h = specula_core::hash::StableHasher::with_seed(0x6c6f_6769_6333);
    h.write_str(stream);
    h.write_u64(a);
    h.write_u64(b);
    h.finish()
}

impl Shared {
    /// The streams, by name.
    pub fn rngs(&self) -> &BTreeMap<String, Rng> {
        &self.rngs
    }

    /// Runs `f` on stream `stream` (made with `make` if it is new), keeping `rng_sum`.
    pub fn draw<R>(&mut self, stream: &str, make: impl FnOnce() -> Rng, f: impl FnOnce(&mut Rng) -> R) -> R {
        let before = self.rngs.get(stream).map_or(0, |r| stream_hash(stream, r));
        let rng = match self.rngs.get_mut(stream) {
            Some(r) => r,
            None => self.rngs.entry(stream.to_owned()).or_insert_with(make),
        };
        let out = f(rng);
        let after = stream_hash(stream, rng);
        self.rng_sum = self.rng_sum.wrapping_sub(before).wrapping_add(after);
        out
    }

    /// Sets (`Some`) or removes (`None`) a stream (undo, save games), keeping `rng_sum`.
    pub fn put_rng(&mut self, stream: String, rng: Option<Rng>) {
        let old = match rng {
            Some(r) => {
                self.rng_sum = self.rng_sum.wrapping_add(stream_hash(&stream, &r));
                self.rngs.insert(stream.clone(), r)
            }
            None => self.rngs.remove(&stream),
        };
        if let Some(o) = old {
            self.rng_sum = self.rng_sum.wrapping_sub(stream_hash(&stream, &o));
        }
    }

    /// Removes every stream (a load with a new seed).
    pub fn clear_rngs(&mut self) {
        self.rngs.clear();
        self.rng_sum = 0;
    }

    /// Sets (`Some`) or removes (`None`) a key, keeping `store_bytes`; returns the old value.
    pub fn put(&mut self, key: &str, value: Option<Value>) -> Option<Value> {
        self.store_hash.set(None);
        let size = |v: &Value| key.len() + v.encode().len();
        if let Some(v) = &value {
            self.store_bytes += size(v);
        }
        let old = match value {
            Some(v) => self.store.insert(key.to_owned(), v),
            None => self.store.remove(key),
        };
        if let Some(o) = &old {
            self.store_bytes -= size(o);
        }
        old
    }
}

struct Slot {
    info: Info,
    code: ModuleCode,
    origin: Origin,
    generation: u64,
    loaded_tick: u64,
    /// Why the module stopped running (a panic or a runaway), if it did.
    disabled: Option<Json>,
    ticks: u64,
    failures: u64,
    last_error: Option<Json>,
    /// The first failure since the module was loaded: where a module that fails every tick began.
    first_error: Option<Json>,
    tick_total: Duration,
    tick_max: Duration,
    input: ModuleInput,
}

/// A module call that failed: its status and a bounded error (see [`failure_json`]).
#[derive(Clone, Debug)]
pub(crate) struct Failure {
    pub status: u32,
    pub json: Json,
}

impl Failure {
    /// Panics and runaways stop the module: calling it again would do the same.
    fn disables(&self) -> bool {
        self.status == status::PANIC || self.json.get("code").and_then(Json::as_str) == Some("too_many_requests")
    }
}

/// Why a call didn't happen: the module failed, or its changes were refused (claims).
#[derive(Debug)]
pub(crate) enum CallError {
    Module(Failure),
    Refused(WorldError),
}

/// A successful call's result and what to undo if its commit is refused.
struct Done {
    reply: Value,
    events: Vec<EventRec>,
    sounds: Vec<SoundCue>,
    state_undo: Vec<(String, Option<Value>)>,
    rng_undo: Vec<(String, Option<Rng>)>,
}

/// Puts the state store and RNG streams back as they were before a call.
fn undo_shared(shared: &mut Shared, state_undo: Vec<(String, Option<Value>)>, rng_undo: Vec<(String, Option<Rng>)>) {
    for (key, before) in state_undo.into_iter().rev() {
        shared.put(&key, before);
    }
    for (stream, before) in rng_undo.into_iter().rev() {
        shared.put_rng(stream, before);
    }
}

/// The parts of the host one call needs (split from [`LogicHost`] so a call can run code that
/// isn't in a slot yet).
struct CallParts<'a> {
    shared: &'a mut Shared,
    mover: &'a mut dyn CharacterMover,
    log: &'a mut VecDeque<LogLine>,
    pending_events: usize,
    fp: &'a mut FpLog,
    params: &'a BTreeMap<String, Param>,
}

/// Runs one call of `code` as module `name`, with the world recorded in full. On failure the
/// world (back to where the call began), state and RNG are rolled back here.
#[allow(clippy::too_many_arguments)]
fn invoke(code: &ModuleCode, name: &str, parts: CallParts<'_>, call: u32, input: &Value, world: &mut dyn WorldWrite, tick: u64, what: &str) -> Result<Done, Failure> {
    let agent = agent_of(name);
    let mark = world.mark();
    let mut h = ModuleHandler {
        world,
        module: name,
        agent: &agent,
        tick,
        shared: parts.shared,
        mover: parts.mover,
        events: Vec::new(),
        pending_events: parts.pending_events,
        sounds: Vec::new(),
        log: parts.log,
        params: parts.params,
        state_undo: Vec::new(),
        rng_undo: Vec::new(),
        traced: 0,
    };
    let out = code.call(call, input, &mut h, MAX_REQUESTS_PER_CALL);
    if let Some(d) = out.fp_before {
        parts.fp.note(tick, "before_module_call", d);
    }
    if let Some(d) = out.fp_changed {
        parts.fp.note(tick, &agent, d);
    }
    if out.ok() {
        Ok(Done { reply: out.reply, events: h.events, sounds: h.sounds, state_undo: h.state_undo, rng_undo: h.rng_undo })
    } else {
        undo_shared(h.shared, std::mem::take(&mut h.state_undo), std::mem::take(&mut h.rng_undo));
        world.rollback_to(mark);
        Err(Failure { status: out.status, json: failure_json(&out, name, what, tick) })
    }
}

/// Staged hook calls: the events queued before them, and each call's state and RNG undo.
struct Staged {
    events_at: usize,
    sounds_at: usize,
    undo: Vec<(String, Done)>,
}

/// See the module docs.
pub struct LogicHost {
    slots: BTreeMap<String, Slot>,
    shared: BTreeMap<String, Shared>,
    /// Emitted this tick, delivered next tick.
    pending_events: Vec<EventRec>,
    recent_events: VecDeque<EventRec>,
    log: VecDeque<LogLine>,
    loads: VecDeque<Json>,
    hook_uses: VecDeque<Json>,
    hook_counts: BTreeMap<String, u64>,
    mover: Box<dyn CharacterMover>,
    /// Gamepads for module actions (opened at the first window frame that needs them; `None`
    /// inside when XInput is unavailable).
    pads: Option<Option<specula_platform::gamepad::Gamepads>>,
    /// Pads an agent's `ui_act` frames hold instead of the polled ones (see
    /// [`LogicHost::set_pad_override`]).
    pad_override: Option<Vec<specula_platform::gamepad::GamepadState>>,
    /// Hook calls staged by a caller that may still refuse them (a live Lex command): what to
    /// undo if it does. See [`LogicHost::stage`].
    staged: Option<Staged>,
    /// Reused by [`LogicHost::state_hash`] (no allocation per tick once warm).
    hash_scratch: Vec<u8>,
    /// Loads, reloads, unloads and disables not yet journaled ([`LogicHost::take_module_events`]).
    module_events: Vec<ModuleEvent>,
    /// Floating-point environment changes found around module calls (and restored).
    fp: FpLog,
    /// Sounds committed calls asked for, for the engine to play ([`LogicHost::take_sounds`]).
    sounds: Vec<SoundCue>,
    /// Sounds dropped because [`MAX_PENDING_SOUNDS`] were waiting.
    sounds_dropped: u64,
    /// Builders' modules waiting for the owner or a granted lead ([`proposals`]).
    pub(crate) proposals: proposals::Proposals,
    /// Numbers a run was given for its rules to read (`specula run --set friction=0.3`;
    /// `Cx::param`): the run's, as its script and its seed are, set once before a module loads
    /// and never changed by one. A window's game has none, and every `param` answers its default.
    params: BTreeMap<String, Param>,
}

/// A number a run was given, and whether any rule has asked for it: a name nothing reads is a
/// name mistyped (`--set bord=3`), and the run would play as written and say nothing.
#[derive(Debug, Default)]
pub struct Param {
    pub value: f64,
    read: std::sync::atomic::AtomicBool,
}

impl Param {
    /// The number, noted as read.
    pub fn read(&self) -> f64 {
        self.read.store(true, std::sync::atomic::Ordering::Relaxed);
        self.value
    }
}

impl Default for LogicHost {
    fn default() -> LogicHost {
        LogicHost {
            slots: BTreeMap::new(),
            shared: BTreeMap::new(),
            pending_events: Vec::new(),
            recent_events: VecDeque::new(),
            log: VecDeque::new(),
            loads: VecDeque::new(),
            hook_uses: VecDeque::new(),
            hook_counts: BTreeMap::new(),
            mover: Box::new(Kinematic),
            pads: None,
            pad_override: None,
            staged: None,
            params: BTreeMap::new(),
            hash_scratch: Vec::new(),
            module_events: Vec::new(),
            fp: FpLog::default(),
            sounds: Vec::new(),
            sounds_dropped: 0,
            proposals: proposals::Proposals::default(),
        }
    }
}

/// `module:<name>`: attribution of a module's world changes.
pub fn agent_of(module: &str) -> String {
    format!("{}{module}", crate::swarm::MODULE_PREFIX)
}

/// The path a module's things live under by convention, and the one a claim protects it by:
/// `/<name>`.
pub fn root_of(module: &str) -> String {
    format!("/{module}")
}

fn push_bounded<T>(q: &mut VecDeque<T>, item: T, max: usize) {
    if q.len() == max {
        q.pop_front();
    }
    q.push_back(item);
}

/// Cuts a string to `max` characters (marking the cut).
fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push_str("...");
        t
    }
}

/// A call's failure as bounded data: the module's `code`, `message` (≤ 1,000 characters) and
/// `location`, up to 16 more small fields of its error, and what failed.
fn failure_json(out: &specula_logic::CallOutcome, module: &str, what: &str, tick: u64) -> Json {
    let kind = match out.status {
        status::PANIC => "panic",
        status::ERROR => "error",
        status::BAD_INPUT => "bad_input",
        status::UNKNOWN_ENTRY => "unknown_entry",
        _ => "unknown_status",
    };
    let mut f: Vec<(String, Json)> = Vec::new();
    match &out.reply {
        Value::Map(m) => {
            for (k, v) in m {
                let j = match (k.as_str(), v) {
                    ("message", Value::Str(s)) => Json::from(cut(s, 1000)),
                    (_, Value::Str(s)) => Json::from(cut(s, 300)),
                    _ => v.to_json(),
                };
                if f.len() < 19 && (matches!(k.as_str(), "code" | "message" | "location") || j.to_string().len() <= 256) {
                    f.push((k.clone(), j));
                }
            }
        }
        other => {
            let j = other.to_json();
            if j.to_string().len() <= 256 {
                f.push(("detail".into(), j));
            }
        }
    }
    let mut j = Json::Obj(f);
    j.set("failure", Json::from(kind));
    j.set("module", Json::from(module));
    j.set("call", Json::from(what));
    j.set("tick", Json::from(tick));
    j
}

/// Answers nothing: `describe` has no world access.
struct NoWorld;

impl specula_logic::Handler for NoWorld {
    fn request(&mut self, _req: &Value) -> Result<Value, Value> {
        Err(specula_logic::loader::error("no_world", "describe can't make requests; do that in start"))
    }
}

/// `describe`, checked.
fn describe(code: &ModuleCode, tick: u64) -> Result<(Info, ModuleInput), WorldError> {
    let described = code.call(entry::DESCRIBE, &Value::Null, &mut NoWorld, 0);
    if !described.ok() {
        let f = failure_json(&described, "?", "describe", tick);
        return Err(WorldError::new("describe_failed", "the module failed to describe itself; nothing was replaced").with("failure", f));
    }
    let info = parse_info(&described.reply, code.sdk_version())?;
    let input = ModuleInput::new(described.reply.get("actions").and_then(Value::as_str).unwrap_or_default()).map_err(|e| {
        WorldError::new("bad_actions", format!("the module's actions are not a valid specula.input.v1 map: {e}"))
            .with("module", info.name.as_str())
    })?;
    Ok((info, input))
}

impl LogicHost {
    /// The numbers this run's rules may read (`Cx::param`). Set before a module loads.
    pub fn set_params(&mut self, params: impl IntoIterator<Item = (String, f64)>) {
        self.params = params.into_iter().take(MAX_PARAMS).map(|(name, value)| (name, Param { value, read: Default::default() })).collect();
    }

    /// How many numbers the run was given.
    pub fn params_len(&self) -> usize {
        self.params.len()
    }

    /// The run's numbers no rule has asked for so far, by name.
    pub fn params_unread(&self) -> Vec<&str> {
        self.params.iter().filter(|(_, p)| !p.read.load(std::sync::atomic::Ordering::Relaxed)).map(|(name, _)| name.as_str()).collect()
    }

    pub fn new() -> LogicHost {
        LogicHost::default()
    }

    /// Replaces the character controller behind `move_character` (the engine sets the physics
    /// P2 controller, [`crate::physics::mover::PhysicsMover`]).
    pub fn set_mover(&mut self, mover: Box<dyn CharacterMover>) {
        self.mover = mover;
    }

    /// The character controller (to lend it what it needs around calls).
    pub fn mover_mut(&mut self) -> &mut dyn CharacterMover {
        &mut *self.mover
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The module name a library describes itself with (to check claims before loading it).
    pub fn name_of(code: &ModuleCode) -> Result<String, WorldError> {
        describe(code, 0).map(|(info, _)| info.name)
    }

    /// Names of the loaded modules, in tick order.
    pub fn modules(&self) -> Vec<String> {
        let mut v: Vec<(&i32, &String)> = self.slots.values().map(|s| (&s.info.order, &s.info.name)).collect();
        v.sort();
        v.into_iter().map(|(_, n)| n.clone()).collect()
    }

    /// Whether module `name` is loaded and running (not disabled).
    pub fn running(&self, name: &str) -> bool {
        self.slots.get(name).is_some_and(|s| s.disabled.is_none())
    }

    /// Where module `name` was last loaded from.
    pub fn origin(&self, name: &str) -> Option<&Origin> {
        self.slots.get(name).map(|s| &s.origin)
    }

    #[allow(clippy::too_many_arguments)]
    fn parts<'a>(
        shared: &'a mut BTreeMap<String, Shared>,
        mover: &'a mut Box<dyn CharacterMover>,
        log: &'a mut VecDeque<LogLine>,
        fp: &'a mut FpLog,
        params: &'a BTreeMap<String, Param>,
        pending: usize,
        name: &str,
    ) -> CallParts<'a> {
        CallParts { shared: shared.entry(name.to_owned()).or_default(), mover: &mut **mover, log, pending_events: pending, fp, params }
    }

    /// An event from the engine itself (`from: "engine"`), delivered to every module with the
    /// next tick like one a module emitted, in the order pushed, and saved with the run like
    /// any waiting event. It is how what happens outside the simulation reaches it at a tick
    /// boundary, and only there: a press of one of the game's own buttons (`ui`), a setting the
    /// rules read (`setting`), the run's end (`outcome`); docs/design/FIRST_HOUR.md. `false`:
    /// not taken (no module is loaded to hear it, the name is not an event's, or
    /// [`MAX_PENDING_EVENTS`] are already waiting).
    /// How many events wait for the next tick.
    pub fn pending_events(&self) -> usize {
        self.pending_events.len()
    }

    pub fn push_engine_event(&mut self, name: &str, data: Value, tick: u64) -> bool {
        if self.slots.is_empty() || !specula_sdk::valid_name(name, 64) || self.pending_events.len() >= MAX_PENDING_EVENTS {
            return false;
        }
        self.pending_events.push(EventRec { name: name.to_owned(), data, from: "engine".to_owned(), tick });
        true
    }

    /// What a module keeps in its state store (`Cx::set_state`), to read: the interface shows
    /// it and the game's outcome is decided from it. `None`: no module of that name has kept
    /// anything.
    pub fn state_of(&self, module: &str) -> Option<&BTreeMap<String, Value>> {
        self.shared.get(module).map(|s| &s.store)
    }

    /// Every module that keeps state, with what it keeps, by name.
    pub fn states(&self) -> impl Iterator<Item = (&str, &BTreeMap<String, Value>)> {
        self.shared.iter().map(|(name, s)| (name.as_str(), &s.store))
    }

    /// Module loads, reloads, unloads and disables since the last call, oldest first, for the
    /// journal (kind `module`) and the desync detector's trace.
    pub fn take_module_events(&mut self) -> Vec<ModuleEvent> {
        std::mem::take(&mut self.module_events)
    }

    /// Queues committed calls' sounds (past [`MAX_PENDING_SOUNDS`] waiting, the rest are dropped
    /// and counted).
    fn queue_sounds(&mut self, sounds: Vec<SoundCue>) {
        for s in sounds {
            if self.sounds.len() >= MAX_PENDING_SOUNDS {
                self.sounds_dropped += 1;
            } else {
                self.sounds.push(s);
            }
        }
    }

    /// The sounds committed module calls asked for since the last take, oldest first, for the
    /// engine to play (`crate::module_sounds`). Presentation only: taking them changes nothing
    /// a module can see.
    pub fn take_sounds(&mut self) -> Vec<SoundCue> {
        if self.staged.is_some() {
            // A caller may still refuse the staged calls: their sounds wait for its answer.
            return Vec::new();
        }
        std::mem::take(&mut self.sounds)
    }

    /// Sounds dropped because too many were waiting.
    pub fn sounds_dropped(&self) -> u64 {
        self.sounds_dropped
    }

    /// The loaded modules' builds, by name (for replay files: a replay expects the same code).
    pub fn builds(&self) -> Vec<(String, u64)> {
        self.slots.iter().map(|(n, s)| (n.clone(), s.code.build_hash())).collect()
    }

    /// Floating-point environment changes found around module calls.
    pub fn fp_log(&self) -> &FpLog {
        &self.fp
    }

    /// One call into loaded module `name`, committed through `commit`; on failure or refusal
    /// nothing it did is kept.
    #[allow(clippy::too_many_arguments)]
    fn call(&mut self, name: &str, call: u32, input: &Value, world: &mut dyn WorldWrite, tick: u64, phase: Phase, commit: &mut Commit<'_>) -> Result<Value, CallError> {
        // Full changes, so a failed or refused call rolls back exactly.
        crate::write_policy::recording_full(world, |world| self.call_full(name, call, input, world, tick, phase, commit))
    }

    #[allow(clippy::too_many_arguments)]
    fn call_full(&mut self, name: &str, call: u32, input: &Value, world: &mut dyn WorldWrite, tick: u64, phase: Phase, commit: &mut Commit<'_>) -> Result<Value, CallError> {
        let slot = self.slots.get(name).expect("callers check the module exists");
        let agent = agent_of(name);
        let mark = world.mark();
        let what = match phase {
            Phase::Start => "start",
            Phase::Tick => "tick",
            Phase::Hook => "hook",
            Phase::Stop => "stop",
        };
        let parts = Self::parts(&mut self.shared, &mut self.mover, &mut self.log, &mut self.fp, &self.params, self.pending_events.len(), name);
        let result = match invoke(&slot.code, name, parts, call, input, world, tick, what) {
            Ok(mut done) => match commit(world, mark, &agent, phase) {
                Ok(()) => {
                    self.pending_events.extend(std::mem::take(&mut done.events));
                    self.queue_sounds(std::mem::take(&mut done.sounds));
                    let reply = std::mem::replace(&mut done.reply, Value::Null);
                    if let Some(s) = &mut self.staged {
                        s.undo.push((name.to_owned(), done));
                    }
                    Ok(reply)
                }
                Err(e) => {
                    world.rollback_to(mark);
                    let shared = self.shared.get_mut(name).expect("made by parts");
                    undo_shared(shared, done.state_undo, done.rng_undo);
                    Err(CallError::Refused(e))
                }
            },
            Err(f) => {
                // Nothing is pending, but ids the call used must reach the journal.
                let at = world.mark();
                let _ = commit(world, at, &agent, phase);
                Err(CallError::Module(f))
            }
        };
        result
    }

    /// Loads `code` at this tick boundary, as a new module or replacing the running build of the
    /// same name, as one transaction (see the module docs): `Err` leaves everything as it was,
    /// the old build running. `commit` checks and journals the stop and start changes.
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        &mut self,
        code: ModuleCode,
        origin: Origin,
        seed: Option<u64>,
        world: &mut dyn WorldWrite,
        tick: u64,
        tick_hz: u32,
        commit: &mut Commit<'_>,
    ) -> Result<Json, WorldError> {
        // Full changes, so a failed load rolls back exactly.
        crate::write_policy::recording_full(world, |world| self.load_full(code, origin, seed, world, tick, tick_hz, commit))
    }

    #[allow(clippy::too_many_arguments)]
    fn load_full(
        &mut self,
        code: ModuleCode,
        origin: Origin,
        seed: Option<u64>,
        world: &mut dyn WorldWrite,
        tick: u64,
        tick_hz: u32,
        commit: &mut Commit<'_>,
    ) -> Result<Json, WorldError> {
        let t0 = Instant::now();
        let (info, mut input) = describe(&code, tick)?;
        let name = info.name.clone();
        let agent = agent_of(&name);
        let reloaded = self.slots.contains_key(&name);
        if !reloaded && self.slots.len() >= MAX_MODULES {
            return Err(WorldError::new("too_many_modules", format!("at most {MAX_MODULES} modules at once; unload one first")));
        }
        let mark = world.mark();
        let saved = self.shared.get(&name).cloned();
        let mut events = Vec::new();
        let mut sounds = Vec::new();
        // 1. The old build stops (a disabled one isn't called again).
        let mut stop_error = None;
        if let Some(old) = self.slots.get(&name).filter(|s| s.disabled.is_none()) {
            let stop = Value::map().with("tick", tick).with("tick_hz", tick_hz).with("reason", "reload");
            let parts = Self::parts(&mut self.shared, &mut self.mover, &mut self.log, &mut self.fp, &self.params, self.pending_events.len(), &name);
            match invoke(&old.code, &name, parts, entry::STOP, &stop, world, tick, "stop") {
                Ok(done) => {
                    events.extend(done.events);
                    sounds.extend(done.sounds);
                }
                Err(f) => stop_error = Some(f.json),
            }
        }
        // 2. The new one starts.
        let shared = self.shared.entry(name.clone()).or_default();
        if let Some(s) = seed {
            shared.seed = s;
            shared.clear_rngs();
        }
        shared.loads += 1;
        let generation = shared.loads;
        let start = Value::map()
            .with("tick", tick)
            .with("tick_hz", tick_hz)
            .with("seed", shared.seed)
            .with("reloaded", reloaded)
            .with("generation", generation);
        let t1 = Instant::now();
        let parts = Self::parts(&mut self.shared, &mut self.mover, &mut self.log, &mut self.fp, &self.params, self.pending_events.len() + events.len(), &name);
        let started = invoke(&code, &name, parts, entry::START, &start, world, tick, "start");
        let start_ms = t1.elapsed().as_secs_f64() * 1000.0;
        // 3. Commit, or put everything back.
        let refused = match started {
            Err(f) => Some(
                WorldError::new(
                    "start_failed",
                    if reloaded {
                        format!("the new build of '{name}' failed to start; nothing changed and the running build stays")
                    } else {
                        format!("module '{name}' failed to start; it was not loaded")
                    },
                )
                .with("failure", f.json),
            ),
            Ok(done) => {
                events.extend(done.events);
                sounds.extend(done.sounds);
                commit(world, mark, &agent, Phase::Start).err()
            }
        };
        if let Some(e) = refused {
            world.rollback_to(mark);
            match saved {
                Some(s) => self.shared.insert(name.clone(), s),
                None => self.shared.remove(&name),
            };
            let at = world.mark();
            let _ = commit(world, at, &agent, Phase::Start);
            return Err(e.with("module", name.as_str()));
        }
        self.pending_events.extend(events);
        self.queue_sounds(sounds);
        let mut previous_build = None;
        if let Some(old) = self.slots.remove(&name) {
            previous_build = Some(old.code.build_hash());
            input.adopt(old.input);
            // `old` drops here: its library unloads.
        }
        self.module_events.push(ModuleEvent {
            tick,
            module: name.clone(),
            event: if reloaded { "reload" } else { "load" },
            generation,
            build_hash: code.build_hash(),
            previous_build,
        });
        let build_hash = format!("{:016x}", code.build_hash());
        let sdk = info.sdk.clone();
        let hooks: Vec<Json> = info.hooks.iter().map(|h| Json::from(h.qualified())).collect();
        let actions: Vec<Json> = input.names().map(Json::from).collect();
        self.slots.insert(
            name.clone(),
            Slot {
                info,
                code,
                origin,
                generation,
                loaded_tick: tick,
                disabled: None,
                ticks: 0,
                failures: 0,
                last_error: None,
                first_error: None,
                tick_total: Duration::ZERO,
                tick_max: Duration::ZERO,
                input,
            },
        );
        let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let event = if reloaded { "reload" } else { "load" };
        let record = Json::obj([
            ("tick", Json::from(tick)),
            ("module", Json::from(name.as_str())),
            ("event", Json::from(event)),
            ("generation", Json::from(generation)),
            ("build_hash", Json::from(build_hash.as_str())),
            ("ms", Json::from(total_ms)),
        ]);
        push_bounded(&mut self.loads, record, LOADS_KEPT);
        let mut report = vec![
            ("module", Json::from(name.as_str())),
            ("event", Json::from(event)),
            ("status", Json::from("running")),
            ("generation", Json::from(generation)),
            ("loaded_at_tick", Json::from(tick)),
            ("build_hash", Json::from(build_hash)),
            ("sdk", Json::from(sdk)),
            ("abi", Json::from(specula_logic::abi::ABI_VERSION)),
            ("actions", Json::arr(actions)),
            ("hooks", Json::arr(hooks)),
            ("timings_ms", Json::obj([("start", Json::from(start_ms)), ("load", Json::from(total_ms))])),
        ];
        if let Some(e) = stop_error {
            report.push(("old_stop_error", e));
        }
        Ok(Json::obj(report))
    }

    /// Stops and unloads module `name` (`stop` with reason `unload` runs first). `forget` also
    /// drops its state store and RNG streams; otherwise a later load continues from them. If
    /// `commit` refuses the stop's changes (another agent's claim), nothing changes and the
    /// module stays; if the stop itself fails, its changes are dropped and the module is
    /// unloaded anyway.
    pub fn unload(&mut self, name: &str, forget: bool, world: &mut dyn WorldWrite, tick: u64, tick_hz: u32, commit: &mut Commit<'_>) -> Result<Json, WorldError> {
        if !self.slots.contains_key(name) {
            return Err(self.no_module(name));
        }
        let stop = Value::map().with("tick", tick).with("tick_hz", tick_hz).with("reason", "unload");
        let mut stop_error = None;
        if self.slots[name].disabled.is_none() {
            match self.call(name, entry::STOP, &stop, world, tick, Phase::Stop, commit) {
                Ok(_) => {}
                Err(CallError::Module(f)) => stop_error = Some(f.json),
                Err(CallError::Refused(e)) => {
                    return Err(e.with("module", name).with("hint", "the module stays loaded: its stop would change what that claim protects"))
                }
            }
        }
        let slot = self.slots.remove(name).expect("checked");
        if forget {
            self.shared.remove(name);
        }
        self.module_events.push(ModuleEvent {
            tick,
            module: name.to_owned(),
            event: "unload",
            generation: slot.generation,
            build_hash: slot.code.build_hash(),
            previous_build: None,
        });
        push_bounded(
            &mut self.loads,
            Json::obj([
                ("tick", Json::from(tick)),
                ("module", Json::from(name)),
                ("event", Json::from("unload")),
                ("generation", Json::from(slot.generation)),
                ("build_hash", Json::from(format!("{:016x}", slot.code.build_hash()))),
            ]),
            LOADS_KEPT,
        );
        let mut out = vec![("unloaded", Json::from(name)), ("state_forgotten", Json::from(forget))];
        if let Some(e) = stop_error {
            out.push(("stop_error", e));
        }
        Ok(Json::obj(out))
    }

    fn no_module(&self, name: &str) -> WorldError {
        let loaded = self.modules();
        WorldError::new(
            "no_module",
            if loaded.is_empty() {
                format!("no module '{name}' is loaded (none are: build one with logic_build)")
            } else {
                format!("no module '{name}' is loaded (loaded: {})", loaded.join(", "))
            },
        )
        .with("module", name)
    }

    /// One window frame's input, for every module's actions (gamepads polled here). `held`: the
    /// game's clock is held (its title, a pause, a run that ended), and nothing pressed now acts
    /// in the game ([`ModuleInput::feed_held`]).
    pub fn feed_window(&mut self, events: &[Event], dt_s: f32, held: bool) {
        if self.slots.values().all(|s| s.input.is_empty()) {
            return;
        }
        let pads = self.pads.get_or_insert_with(|| specula_platform::gamepad::Gamepads::new().ok());
        let states: &[specula_platform::gamepad::GamepadState] = match pads {
            Some(p) => {
                p.poll();
                p.states()
            }
            None => &[],
        };
        for slot in self.slots.values_mut() {
            if held {
                slot.input.feed_held(events, states, dt_s);
            } else {
                slot.input.feed(events, states, dt_s);
            }
        }
    }

    /// A window frame whose input something else took (the engine's modal review screen): every
    /// module input sees no events and released pads, so nothing is pressed or held.
    pub fn feed_window_idle(&mut self, dt_s: f32) {
        for slot in self.slots.values_mut() {
            slot.input.feed(&[], &[], dt_s);
        }
    }

    /// The gamepads' states as last polled (opened at first use; empty without XInput), for
    /// other input readers in the same frame (live Lex runs), polling them if no module did.
    pub fn pad_states(&mut self, poll: bool) -> &[specula_platform::gamepad::GamepadState] {
        if let Some(p) = &self.pad_override {
            return p;
        }
        match self.pads.get_or_insert_with(|| specula_platform::gamepad::Gamepads::new().ok()) {
            Some(p) => {
                if poll {
                    p.poll();
                }
                p.states()
            }
            None => &[],
        }
    }

    /// While set, [`LogicHost::pad_states`] answers these pads instead of the polled ones: the
    /// frames an agent's `ui_act` synthesises for the engine's own screens (one frame each; the
    /// caller clears it after).
    pub fn set_pad_override(&mut self, pads: Option<Vec<specula_platform::gamepad::GamepadState>>) {
        self.pad_override = pads;
    }

    /// Whether any module reads input (and so polled the gamepads this frame).
    pub fn reads_input(&self) -> bool {
        self.slots.values().any(|s| !s.input.is_empty())
    }

    /// The actions among `actions` the human is using at the window in module `name` (a hold
    /// would take them over; see [`input::ModuleInput::human_using`]).
    pub fn human_using(&self, name: &str, actions: &Json) -> Vec<String> {
        self.slots.get(name).map_or_else(Vec::new, |s| s.input.human_using(actions))
    }

    /// `logic_input`: an agent (`by`, recorded on each hold) holds actions of module `name` for
    /// `ticks` ticks.
    pub fn hold(&mut self, name: &str, actions: &Json, ticks: u64, by: Option<crate::inputguard::Actor>) -> Result<Json, WorldError> {
        let Some(slot) = self.slots.get_mut(name) else { return Err(self.no_module(name)) };
        let held = slot.input.hold(actions, ticks, by).map_err(|m| WorldError::new("invalid_field", m).with("field", "actions"))?;
        Ok(Json::obj([("module", Json::from(name)), ("held", held)]))
    }

    /// Ends every hold the identity `key` placed, in every module (its session froze); how many.
    pub fn release_holds_of(&mut self, key: &str) -> usize {
        self.slots.values_mut().map(|s| s.input.release_holds_of(key)).sum()
    }

    /// Holds the window's human took back since the last call (their own input on a held action
    /// ends the hold): `(module, action, holder)`.
    pub fn take_holds_ended(&mut self) -> Vec<(String, String, Option<crate::inputguard::Actor>)> {
        let mut out = Vec::new();
        for (name, s) in &mut self.slots {
            for (action, by) in s.input.take_ended() {
                out.push((name.clone(), action, by));
            }
        }
        out
    }

    /// Stops calling a module after a panic or a runaway.
    fn disable(&mut self, name: &str, why: Json, tick: u64) {
        if let Some(slot) = self.slots.get_mut(name) {
            slot.disabled = Some(why.clone());
            self.module_events.push(ModuleEvent {
                tick,
                module: name.to_owned(),
                event: "disabled",
                generation: slot.generation,
                build_hash: slot.code.build_hash(),
                previous_build: None,
            });
        }
        push_bounded(
            &mut self.loads,
            Json::obj([("tick", Json::from(tick)), ("module", Json::from(name)), ("event", Json::from("disabled")), ("why", why.clone())]),
            LOADS_KEPT,
        );
        eprintln!("specula: module '{name}' was disabled at tick {tick}: {why}");
        crate::trace_engine::module_disabled(name, &why, tick);
    }

    /// One simulation tick of every running module (call after the tick's other systems; the
    /// tick number is the one being run). `commit` journals each module's changes.
    pub fn tick(&mut self, world: &mut dyn WorldWrite, tick: u64, tick_hz: u32, commit: &mut Commit<'_>) {
        if self.slots.is_empty() {
            self.pending_events.clear();
            return;
        }
        let delivering = std::mem::take(&mut self.pending_events);
        let events = Value::List(delivering.iter().map(EventRec::to_value).collect());
        for name in self.modules() {
            let slot = self.slots.get_mut(&name).expect("listed");
            if slot.disabled.is_some() {
                continue;
            }
            let (actions, presses) = slot.input.take_tick_and_presses();
            // Every press since the last tick, in order, repeats included, as events of the
            // engine's own to this module alone (`press`, with `action`): what `pressed` cannot
            // say, as two presses of one action between two ticks (a script's `1:down,1:down`).
            let events = if presses.is_empty() {
                events.clone()
            } else {
                let mut all = events.as_list().map(<[Value]>::to_vec).unwrap_or_default();
                all.extend(presses.iter().map(|action| EventRec { name: PRESS_EVENT.to_owned(), data: Value::map().with("action", action.as_str()), from: "engine".to_owned(), tick }.to_value()));
                Value::List(all)
            };
            let input = Value::map().with("tick", tick).with("tick_hz", tick_hz).with("actions", actions).with("events", events);
            let t = Instant::now();
            let result = self.call(&name, entry::TICK, &input, world, tick, Phase::Tick, commit);
            let took = t.elapsed();
            let slot = self.slots.get_mut(&name).expect("listed");
            slot.ticks += 1;
            slot.tick_total += took;
            slot.tick_max = slot.tick_max.max(took);
            match result {
                Ok(_) => {}
                Err(CallError::Module(f)) => {
                    slot.failures += 1;
                    slot.last_error = Some(f.json.clone());
                    slot.first_error.get_or_insert_with(|| f.json.clone());
                    if f.disables() {
                        self.disable(&name, f.json, tick);
                    }
                }
                Err(CallError::Refused(e)) => {
                    // Tick commits only journal; a refusal is an engine bug, but keep going.
                    slot.failures += 1;
                    slot.last_error = Some(e.to_json());
                    slot.first_error.get_or_insert_with(|| e.to_json());
                }
            }
        }
        for e in delivering {
            push_bounded(&mut self.recent_events, e, EVENTS_KEPT);
        }
    }

    /// Every native hook the running modules declare, by qualified name.
    pub fn hooks(&self) -> Vec<HookInfo> {
        self.slots.values().filter(|s| s.disabled.is_none()).flat_map(|s| s.info.hooks.iter().cloned()).collect()
    }

    /// Starts staging hook calls: until [`LogicHost::keep_staged`] or
    /// [`LogicHost::drop_staged`], each successful call's state, RNG draws and events can still
    /// be taken back (the caller rolls the world back itself).
    pub fn stage(&mut self) {
        self.staged = Some(Staged { events_at: self.pending_events.len(), sounds_at: self.sounds.len(), undo: Vec::new() });
    }

    /// A module's state store and RNG streams, and the events queued (tests).
    #[cfg(test)]
    pub(crate) fn test_snapshot(&self, module: &str) -> (BTreeMap<String, Value>, BTreeMap<String, Rng>, usize) {
        let s = self.shared.get(module).cloned().unwrap_or_default();
        (s.store, s.rngs, self.pending_events.len())
    }

    /// Keeps what the staged calls did.
    pub fn keep_staged(&mut self) {
        self.staged = None;
    }

    /// Takes back what the staged calls did to module state and RNG streams, and the events
    /// they queued (newest first).
    pub fn drop_staged(&mut self) {
        let Some(s) = self.staged.take() else { return };
        for (name, done) in s.undo.into_iter().rev() {
            if let Some(shared) = self.shared.get_mut(&name) {
                undo_shared(shared, done.state_undo, done.rng_undo);
            }
        }
        self.pending_events.truncate(s.events_at);
        self.sounds.truncate(s.sounds_at);
    }

    /// Calls native hook `hook` (`module/name`) with `args`, for `caller` (an agent, or `lex`)
    /// who says why in `reason` (required: every use is logged, so Lex can grow from evidence).
    /// The hook's world changes go through `commit`; a failure or refusal keeps none of them.
    #[allow(clippy::too_many_arguments)]
    pub fn call_hook(
        &mut self,
        hook: &str,
        args: &Json,
        reason: &str,
        caller: &str,
        world: &mut dyn WorldWrite,
        tick: u64,
        tick_hz: u32,
        commit: &mut Commit<'_>,
    ) -> Result<Json, WorldError> {
        let reason = reason.trim();
        if reason.is_empty() || reason.chars().count() > 300 {
            return Err(WorldError::new("invalid_field", "'reason' is required (1..300 characters): why a native hook instead of data")
                .with("field", "reason"));
        }
        let Some((module, name)) = hook.split_once('/') else {
            return Err(WorldError::new("invalid_field", format!("'{hook}' is not module/hook (e.g. ambulo/launch)")).with("field", "hook"));
        };
        let Some(slot) = self.slots.get(module) else { return Err(self.no_module(module)) };
        if !slot.info.hooks.iter().any(|h| h.name == name) {
            let have: Vec<String> = slot.info.hooks.iter().map(HookInfo::qualified).collect();
            return Err(WorldError::new("no_hook", format!("module '{module}' declares no hook '{name}' (it has: {})", if have.is_empty() { "none".into() } else { have.join(", ") }))
                .with("hook", hook));
        }
        if let Some(why) = &slot.disabled {
            return Err(WorldError::new("module_disabled", format!("module '{module}' is disabled; rebuild it")).with("why", why.clone()));
        }
        let input = Value::map()
            .with("tick", tick)
            .with("tick_hz", tick_hz)
            .with("hook", name)
            .with("args", Value::from_json(args))
            .with("reason", reason)
            .with("caller", caller);
        let result = self.call(module, entry::HOOK, &input, world, tick, Phase::Hook, commit);
        *self.hook_counts.entry(hook.to_owned()).or_default() += 1;
        push_bounded(
            &mut self.hook_uses,
            Json::obj([
                ("tick", Json::from(tick)),
                ("hook", Json::from(hook)),
                ("caller", Json::from(caller)),
                ("reason", Json::from(reason)),
                ("ok", Json::from(result.is_ok())),
            ]),
            HOOK_USES_KEPT,
        );
        match result {
            Ok(v) => Ok(v.to_json()),
            Err(CallError::Refused(e)) => Err(e.with("hook", hook)),
            Err(CallError::Module(f)) => {
                if f.disables() {
                    self.disable(module, f.json.clone(), tick);
                }
                Err(WorldError::new("hook_failed", format!("hook {hook} failed; nothing it did was kept")).with("failure", f.json))
            }
        }
    }

    /// For `engine_info`: one line per module.
    pub fn summary(&self) -> Json {
        Json::arr(self.modules().iter().map(|n| {
            let s = &self.slots[n];
            Json::obj([
                ("name", Json::from(n.as_str())),
                ("status", Json::from(if s.disabled.is_some() { "disabled" } else { "running" })),
                ("generation", Json::from(s.generation)),
            ])
        }))
    }

    /// `logic_status`: modules (or one), recent log lines, loads, events and hook uses.
    pub fn status(&self, module: Option<&str>, detailed: bool) -> Result<Json, WorldError> {
        if let Some(m) = module {
            if !self.slots.contains_key(m) {
                return Err(self.no_module(m));
            }
        }
        let keep = |m: &str| module.is_none_or(|w| w == m);
        let n = if detailed { 50 } else { 8 };
        let modules = self.modules().into_iter().filter(|m| keep(m)).map(|name| {
            let s = &self.slots[&name];
            let shared = self.shared.get(&name);
            let avg_us = if s.ticks == 0 { 0.0 } else { s.tick_total.as_secs_f64() * 1e6 / s.ticks as f64 };
            let mut f = vec![
                ("name", Json::from(name.as_str())),
                ("version", Json::from(s.info.version.as_str())),
                ("description", Json::from(s.info.description.as_str())),
                ("status", Json::from(if s.disabled.is_some() { "disabled" } else { "running" })),
                ("order", Json::from(i64::from(s.info.order))),
                ("generation", Json::from(s.generation)),
                ("loaded_at_tick", Json::from(s.loaded_tick)),
                ("build_hash", Json::from(format!("{:016x}", s.code.build_hash()))),
                ("sdk", Json::from(s.info.sdk.as_str())),
                ("source", Json::obj([
                    ("dir", s.origin.dir.as_ref().map_or(Json::Null, |d| Json::from(d.display().to_string()))),
                    ("library", s.origin.library.as_ref().map_or(Json::Null, |d| Json::from(d.display().to_string()))),
                ])),
                ("ticks", Json::from(s.ticks)),
                ("failures", Json::from(s.failures)),
                ("tick_us", Json::obj([("avg", Json::from(avg_us)), ("max", Json::from(s.tick_max.as_secs_f64() * 1e6))])),
                ("actions", Json::arr(s.input.names().map(Json::from))),
                ("agent_input", s.input.holds_json()),
                ("hooks", Json::arr(s.info.hooks.iter().map(HookInfo::to_json))),
                ("state_keys", Json::from(shared.map_or(0, |sh| sh.store.len()))),
                ("state_bytes", Json::from(shared.map_or(0, |sh| sh.store_bytes))),
                // Times this module left the floating-point environment changed (restored each time).
                ("fp_restored", Json::from(self.fp.count(&agent_of(&name)))),
            ];
            if let Some(r) = self.fp.last(&agent_of(&name)) {
                let mut last = crate::fpguard::drift_json(&r.drift);
                last.set("tick", Json::from(r.tick));
                f.push(("fp_last", last));
            }
            if detailed {
                if let Some(sh) = shared {
                    // Bounded: the first 64 keys, values as JSON.
                    f.push(("state", Json::obj(sh.store.iter().take(64).map(|(k, v)| (k.as_str(), v.to_json())))));
                }
            }
            if let Some(d) = &s.disabled {
                f.push(("disabled_by", d.clone()));
            }
            if let Some(e) = &s.first_error {
                f.push(("first_error", e.clone()));
            }
            if let Some(e) = &s.last_error {
                f.push(("last_error", e.clone()));
            }
            Json::obj(f)
        });
        let tail = |q: &VecDeque<Json>| Json::arr(q.iter().rev().filter(|j| j.get("module").and_then(Json::as_str).is_none_or(keep)).take(n).cloned().collect::<Vec<_>>().into_iter().rev());
        let log = self.log.iter().rev().filter(|l| keep(&l.module)).take(n).collect::<Vec<_>>();
        let log = Json::arr(log.into_iter().rev().map(|l| {
            Json::obj([("tick", Json::from(l.tick)), ("module", Json::from(l.module.as_str())), ("message", Json::from(l.message.as_str()))])
        }));
        let events = self.recent_events.iter().rev().filter(|e| keep(&e.from)).take(n).collect::<Vec<_>>();
        let events = Json::arr(events.into_iter().rev().map(EventRec::to_status_json));
        let hook_uses = Json::arr(
            self.hook_uses
                .iter()
                .rev()
                .filter(|u| u.get("hook").and_then(Json::as_str).is_some_and(|h| keep(h.split('/').next().unwrap_or_default())))
                .take(n)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev(),
        );
        Ok(Json::obj([
            ("modules", Json::arr(modules)),
            ("controller", Json::from(self.mover.name())),
            ("loads", tail(&self.loads)),
            ("log", log),
            ("events", events),
            ("events_pending", Json::from(self.pending_events.len())),
            ("sounds_dropped", Json::from(self.sounds_dropped)),
            ("hook_uses", hook_uses),
            ("hook_counts", Json::obj(self.hook_counts.iter().filter(|(h, _)| keep(h.split('/').next().unwrap_or_default())).map(|(h, c)| (h.as_str(), Json::from(*c))))),
        ]))
    }
}

/// Checks what `describe` answered.
fn parse_info(v: &Value, sdk_of_library: &str) -> Result<Info, WorldError> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
    let name = s("name");
    if !specula_sdk::valid_name(&name, 32) {
        return Err(WorldError::new("bad_module_name", format!("module name '{name}' must be [a-z][a-z0-9_]{{0,31}} (ModuleInfo::name)"))
            .with("name", name.as_str()));
    }
    let mut hooks: Vec<HookInfo> = Vec::new();
    for h in v.get("hooks").and_then(Value::as_list).unwrap_or_default() {
        let hname = h.get("name").and_then(Value::as_str).unwrap_or_default();
        if !specula_sdk::valid_name(hname, 64) {
            return Err(WorldError::new("bad_hook_name", format!("hook name '{hname}' must be [a-z][a-z0-9_]{{0,63}}")).with("module", name.as_str()));
        }
        if hooks.iter().any(|x| x.name == hname) {
            return Err(WorldError::new("bad_hook_name", format!("hook '{hname}' is declared twice")).with("module", name.as_str()));
        }
        let args_text = h.get("args").and_then(Value::as_str).unwrap_or_default();
        let args = if args_text.trim().is_empty() {
            Json::Null
        } else {
            Json::parse(args_text).map_err(|e| {
                WorldError::new("bad_hook_args", format!("hook '{hname}': 'args' is not JSON: {e:?}")).with("module", name.as_str())
            })?
        };
        hooks.push(HookInfo {
            module: name.clone(),
            name: hname.to_owned(),
            doc: h.get("doc").and_then(Value::as_str).unwrap_or_default().to_owned(),
            args,
        });
    }
    let order = v.get("order").and_then(Value::as_i64).unwrap_or(0).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    Ok(Info {
        version: s("version"),
        description: s("description"),
        order,
        sdk: if sdk_of_library.is_empty() { s("sdk") } else { sdk_of_library.to_owned() },
        hooks,
        name,
    })
}


#[cfg(test)]
mod tests;
