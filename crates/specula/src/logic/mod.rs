//! The module host: game logic as hot-reloaded Rust libraries (see `docs/design/MODULES.md`).
//!
//! The engine owns all state. A module is code the [`LogicHost`] calls through the SDK's C ABI
//! (`specula_logic::ModuleCode`) at tick boundaries:
//! - **load / reload** ([`LogicHost::load`]): the new build is described and checked first, so a
//!   broken one never replaces a working one; then the old build stops, the new one takes its
//!   name and starts with `reloaded = true`. The world, the module's state store and its RNG
//!   positions carry over, because none of them live in the module.
//! - **tick** ([`LogicHost::tick`]): every running module in `(order, name)` order, each with its
//!   input actions for the tick and the events of the previous one.
//! - **failure:** a call that fails changes nothing (world changes, state writes and RNG draws
//!   are rolled back); a panic also disables the module, with a structured error saying where.
//! - **hooks** ([`LogicHost::call_hook`]): named native behaviours modules declare, the escape
//!   hatch for Lex; every use is logged with its caller and reason.
//!
//! Module world changes are attributed to `module:<name>` and journaled like a hosted game's
//! (exempt from claims: modules are the game's rules). Each load is recorded with its tick and
//! build hash (DETERMINISM.md rule 8).

pub mod character;
mod handler;
pub mod input;
pub mod tools;

use crate::world::{Recording, World, WorldError};
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
/// Most requests one module call may make (a runaway loop fails the call instead of the frame).
pub const MAX_REQUESTS_PER_CALL: u64 = 200_000;
/// Log lines, loads, hook uses and events kept for `logic_status`.
pub(crate) const LOG_LINES: usize = 256;
const LOADS_KEPT: usize = 64;
const HOOK_USES_KEPT: usize = 256;
const EVENTS_KEPT: usize = 64;
/// Most events delivered per tick (beyond it the oldest are dropped, counted).
const MAX_EVENTS_PER_TICK: usize = 1024;

/// What a module call was, for journaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Start,
    Tick,
    Hook,
    Stop,
}

/// Journals the world changes a module call left (`agent` is `module:<name>`). The engine
/// journals them like a hosted game's; tests just take them.
pub type Journal<'a> = dyn FnMut(&mut World, &str, Phase) + 'a;

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
}

#[derive(Clone, Debug)]
pub(crate) struct LogLine {
    pub tick: u64,
    pub module: String,
    pub message: String,
}

/// What the engine keeps for a module across reloads (and unloads, unless forgotten).
#[derive(Default)]
pub(crate) struct Shared {
    pub store: BTreeMap<String, Value>,
    /// Encoded size of `store` (keys + values).
    pub store_bytes: usize,
    pub rngs: BTreeMap<String, Rng>,
    pub seed: u64,
    /// Loads so far: the next load's generation is this + 1.
    pub loads: u64,
}

impl Shared {
    /// Sets (`Some`) or removes (`None`) a key, keeping `store_bytes`; returns the old value.
    pub fn put(&mut self, key: &str, value: Option<Value>) -> Option<Value> {
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
    /// Why the module stopped running (a panic), if it did.
    disabled: Option<Json>,
    ticks: u64,
    failures: u64,
    last_error: Option<Json>,
    tick_total: Duration,
    tick_max: Duration,
    input: ModuleInput,
}

/// See the module docs.
pub struct LogicHost {
    slots: BTreeMap<String, Slot>,
    shared: BTreeMap<String, Shared>,
    /// Emitted this tick, delivered next tick.
    pending_events: Vec<EventRec>,
    events_dropped: u64,
    recent_events: VecDeque<EventRec>,
    log: VecDeque<LogLine>,
    loads: VecDeque<Json>,
    hook_uses: VecDeque<Json>,
    hook_counts: BTreeMap<String, u64>,
    mover: Box<dyn CharacterMover>,
    /// Gamepads for module actions (opened at the first window frame that needs them; `None`
    /// inside when XInput is unavailable).
    pads: Option<Option<specula_platform::gamepad::Gamepads>>,
}

impl Default for LogicHost {
    fn default() -> LogicHost {
        LogicHost {
            slots: BTreeMap::new(),
            shared: BTreeMap::new(),
            pending_events: Vec::new(),
            events_dropped: 0,
            recent_events: VecDeque::new(),
            log: VecDeque::new(),
            loads: VecDeque::new(),
            hook_uses: VecDeque::new(),
            hook_counts: BTreeMap::new(),
            mover: Box::new(Kinematic),
            pads: None,
        }
    }
}

fn agent_of(module: &str) -> String {
    format!("module:{module}")
}

fn push_bounded<T>(q: &mut VecDeque<T>, item: T, max: usize) {
    if q.len() == max {
        q.pop_front();
    }
    q.push_back(item);
}

/// A call's failure as data: the module's error map, plus what kind of failure.
fn failure_json(out: &specula_logic::CallOutcome, module: &str, what: &str, tick: u64) -> Json {
    let kind = match out.status {
        status::PANIC => "panic",
        status::ERROR => "error",
        status::BAD_INPUT => "bad_input",
        status::UNKNOWN_ENTRY => "unknown_entry",
        _ => "unknown_status",
    };
    let mut j = out.reply.to_json();
    if !matches!(j, Json::Obj(_)) {
        j = Json::obj([("detail", j)]);
    }
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

impl LogicHost {
    pub fn new() -> LogicHost {
        LogicHost::default()
    }

    /// Replaces the character controller behind `move_character` (the physics lane's P2
    /// controller, when it lands).
    pub fn set_mover(&mut self, mover: Box<dyn CharacterMover>) {
        self.mover = mover;
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
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

    /// One call into module `name` with the world recorded in full, so a failure rolls back
    /// exactly; on success the changes are handed to `journal`.
    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        name: &str,
        call: u32,
        input: &Value,
        world: &mut World,
        tick: u64,
        phase: Phase,
        journal: &mut Journal<'_>,
    ) -> Result<Value, (specula_logic::CallOutcome, Json)> {
        let slot = self.slots.get(name).expect("callers check the module exists");
        let shared = self.shared.entry(name.to_owned()).or_default();
        let agent = agent_of(name);
        let before = world.recording();
        world.set_recording(Recording::Full);
        let mark = world.mark();
        let mut h = ModuleHandler {
            world,
            module: name,
            agent: &agent,
            tick,
            shared,
            mover: &mut *self.mover,
            events: Vec::new(),
            log: &mut self.log,
            state_undo: Vec::new(),
            rng_undo: Vec::new(),
        };
        let out = slot.code.call(call, input, &mut h, MAX_REQUESTS_PER_CALL);
        if out.ok() {
            let events = std::mem::take(&mut h.events);
            world.set_recording(before);
            journal(world, &agent, phase);
            self.pending_events.extend(events);
            Ok(out.reply)
        } else {
            h.undo();
            world.rollback_to(mark);
            world.set_recording(before);
            let what = match phase {
                Phase::Start => "start",
                Phase::Tick => "tick",
                Phase::Hook => "hook",
                Phase::Stop => "stop",
            };
            let j = failure_json(&out, name, what, tick);
            Err((out, j))
        }
    }

    /// Loads `code` at this tick boundary: as a new module, or replacing the running build of
    /// the same name (see the module docs). The world changes `start` makes go to `journal`.
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        &mut self,
        code: ModuleCode,
        origin: Origin,
        seed: Option<u64>,
        world: &mut World,
        tick: u64,
        tick_hz: u32,
        journal: &mut Journal<'_>,
    ) -> Result<Json, WorldError> {
        let t0 = Instant::now();
        let described = code.call(entry::DESCRIBE, &Value::Null, &mut NoWorld, 0);
        if !described.ok() {
            let f = failure_json(&described, "?", "describe", tick);
            return Err(WorldError::new("describe_failed", "the module failed to describe itself; nothing was replaced").with("failure", f));
        }
        let info = parse_info(&described.reply, code.sdk_version())?;
        let name = info.name.clone();
        let mut input = ModuleInput::new(
            described.reply.get("actions").and_then(Value::as_str).unwrap_or_default(),
        )
        .map_err(|e| {
            WorldError::new("bad_actions", format!("the module's actions are not a valid specula.input.v1 map: {e}"))
                .with("module", name.as_str())
        })?;
        let reloaded = self.slots.contains_key(&name);
        if !reloaded && self.slots.len() >= MAX_MODULES {
            return Err(WorldError::new("too_many_modules", format!("at most {MAX_MODULES} modules at once; unload one first")));
        }
        // The new build is good to go: the old one stops (its changes kept even if it fails).
        let mut stop_error = None;
        if reloaded {
            let stop = Value::map().with("tick", tick).with("tick_hz", tick_hz).with("reason", "reload");
            if let Err((_, j)) = self.call(&name, entry::STOP, &stop, world, tick, Phase::Stop, journal) {
                stop_error = Some(j);
            }
            let old = self.slots.remove(&name).expect("checked");
            input.adopt_holds(&old.input);
            // `old` drops here: its library unloads before the new one runs.
        }
        let shared = self.shared.entry(name.clone()).or_default();
        if let Some(s) = seed {
            shared.seed = s;
            shared.rngs.clear();
        }
        shared.loads += 1;
        let generation = shared.loads;
        let build_hash = format!("{:016x}", code.build_hash());
        let sdk = info.sdk.clone();
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
                tick_total: Duration::ZERO,
                tick_max: Duration::ZERO,
                input,
            },
        );
        let start = Value::map()
            .with("tick", tick)
            .with("tick_hz", tick_hz)
            .with("seed", self.shared[&name].seed)
            .with("reloaded", reloaded)
            .with("generation", generation);
        let t1 = Instant::now();
        let started = self.call(&name, entry::START, &start, world, tick, Phase::Start, journal);
        let start_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let slot = self.slots.get_mut(&name).expect("just inserted");
        let start_error = match started {
            Ok(_) => None,
            Err((_, j)) => {
                // A module that can't start can't tick either.
                slot.disabled = Some(j.clone());
                Some(j)
            }
        };
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
            ("status", Json::from(if start_error.is_some() { "disabled" } else { "running" })),
            ("generation", Json::from(generation)),
            ("loaded_at_tick", Json::from(tick)),
            ("build_hash", Json::from(build_hash)),
            ("sdk", Json::from(sdk)),
            ("abi", Json::from(specula_logic::abi::ABI_VERSION)),
            ("actions", Json::arr(slot.input.names().map(Json::from))),
            ("hooks", Json::arr(slot.info.hooks.iter().map(|h| Json::from(h.qualified())))),
            ("timings_ms", Json::obj([("start", Json::from(start_ms)), ("load", Json::from(total_ms))])),
        ];
        if let Some(e) = start_error {
            report.push(("start_error", e));
        }
        if let Some(e) = stop_error {
            report.push(("old_stop_error", e));
        }
        Ok(Json::obj(report))
    }

    /// Stops and unloads module `name` (`stop` with reason `unload` runs first). `forget` also
    /// drops its state store and RNG streams; otherwise a later load continues from them.
    pub fn unload(&mut self, name: &str, forget: bool, world: &mut World, tick: u64, tick_hz: u32, journal: &mut Journal<'_>) -> Result<Json, WorldError> {
        if !self.slots.contains_key(name) {
            return Err(self.no_module(name));
        }
        let stop = Value::map().with("tick", tick).with("tick_hz", tick_hz).with("reason", "unload");
        let stop_error = if self.slots[name].disabled.is_none() {
            self.call(name, entry::STOP, &stop, world, tick, Phase::Stop, journal).err().map(|(_, j)| j)
        } else {
            None
        };
        let slot = self.slots.remove(name).expect("checked");
        if forget {
            self.shared.remove(name);
        }
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

    /// One window frame's input, for every module's actions (gamepads polled here).
    pub fn feed_window(&mut self, events: &[Event], dt_s: f32) {
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
            slot.input.feed(events, states, dt_s);
        }
    }

    /// `logic_input`: an agent holds actions of module `name` for `ticks` ticks.
    pub fn hold(&mut self, name: &str, actions: &Json, ticks: u64) -> Result<Json, WorldError> {
        let Some(slot) = self.slots.get_mut(name) else { return Err(self.no_module(name)) };
        let held = slot.input.hold(actions, ticks).map_err(|m| WorldError::new("invalid_field", m).with("field", "actions"))?;
        Ok(Json::obj([("module", Json::from(name)), ("held", held)]))
    }

    /// One simulation tick of every running module (call after the tick's other systems; the
    /// tick number is the one being run).
    pub fn tick(&mut self, world: &mut World, tick: u64, tick_hz: u32, journal: &mut Journal<'_>) {
        if self.slots.is_empty() {
            self.pending_events.clear();
            return;
        }
        let mut delivering = std::mem::take(&mut self.pending_events);
        if delivering.len() > MAX_EVENTS_PER_TICK {
            let extra = delivering.len() - MAX_EVENTS_PER_TICK;
            delivering.drain(..extra);
            self.events_dropped += extra as u64;
        }
        let events = Value::List(delivering.iter().map(EventRec::to_value).collect());
        for name in self.modules() {
            let slot = self.slots.get_mut(&name).expect("listed");
            if slot.disabled.is_some() {
                continue;
            }
            let actions = slot.input.take_tick();
            let input = Value::map().with("tick", tick).with("tick_hz", tick_hz).with("actions", actions).with("events", events.clone());
            let t = Instant::now();
            let result = self.call(&name, entry::TICK, &input, world, tick, Phase::Tick, journal);
            let took = t.elapsed();
            let slot = self.slots.get_mut(&name).expect("listed");
            slot.ticks += 1;
            slot.tick_total += took;
            slot.tick_max = slot.tick_max.max(took);
            if let Err((out, j)) = result {
                slot.failures += 1;
                if out.status == status::PANIC {
                    slot.disabled = Some(j.clone());
                    push_bounded(
                        &mut self.loads,
                        Json::obj([("tick", Json::from(tick)), ("module", Json::from(name.as_str())), ("event", Json::from("disabled")), ("why", j.clone())]),
                        LOADS_KEPT,
                    );
                    eprintln!("specula: module '{name}' panicked at tick {tick} and was disabled: {}", j);
                }
                slot.last_error = Some(j);
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

    /// Calls native hook `hook` (`module/name`) with `args`, for `caller` (an agent, or `lex`)
    /// who says why in `reason` (required: every use is logged, so Lex can grow from evidence).
    /// The hook's world changes go to `journal`.
    #[allow(clippy::too_many_arguments)]
    pub fn call_hook(
        &mut self,
        hook: &str,
        args: &Json,
        reason: &str,
        caller: &str,
        world: &mut World,
        tick: u64,
        tick_hz: u32,
        journal: &mut Journal<'_>,
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
        let result = self.call(module, entry::HOOK, &input, world, tick, Phase::Hook, journal);
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
            Err((out, j)) => {
                if out.status == status::PANIC {
                    if let Some(s) = self.slots.get_mut(module) {
                        s.disabled = Some(j.clone());
                    }
                }
                Err(WorldError::new("hook_failed", format!("hook {hook} failed; nothing it did was kept")).with("failure", j))
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
            ];
            if detailed {
                if let Some(sh) = shared {
                    // Bounded: the first 64 keys, values as JSON.
                    f.push(("state", Json::obj(sh.store.iter().take(64).map(|(k, v)| (k.as_str(), v.to_json())))));
                }
            }
            if let Some(d) = &s.disabled {
                f.push(("disabled_by", d.clone()));
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
        let events = Json::arr(events.into_iter().rev().map(|e| e.to_value().to_json()));
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
            ("events_dropped", Json::from(self.events_dropped)),
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
