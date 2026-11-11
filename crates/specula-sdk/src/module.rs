//! What a module implements ([`Module`]) and what it may touch ([`Cx`]).

use crate::abi::{self, HostApi};
use crate::value::Value;
use specula_core::math::Vec3;
use std::collections::BTreeMap;
use std::fmt;

/// Game logic in a hot-reloadable library.
///
/// **Stateless:** a fresh `Self::default()` handles every call, so fields don't survive from one
/// call to the next, and nothing survives a reload except what the engine keeps: the world
/// (entities, their `props`) and the module's state store ([`Cx::state`]). That is what makes a
/// reload lose nothing. Don't use `static mut`, `thread_local!`, `OnceLock` caches, threads, the
/// wall clock or OS randomness: they break determinism (F2) or dangle after an unload.
pub trait Module: Default + 'static {
    /// Name, version, input actions and native hooks. Called once per load, before `start`.
    fn describe(&self) -> ModuleInfo;

    /// The module was loaded: the first time (`cx.reloaded()` false) or as a new build replacing
    /// a running one (true; the world and state store are as the old build left them). Build or
    /// find what the module needs here; it runs at a tick boundary.
    fn start(&self, _cx: &mut Cx) -> Result<(), SdkError> {
        Ok(())
    }

    /// One fixed simulation step. Modules run in `(order, name)` order every tick.
    fn tick(&self, _cx: &mut Cx) -> Result<(), SdkError> {
        Ok(())
    }

    /// A native hook this module declared in [`ModuleInfo::hooks`], called by the engine (Lex's
    /// escape hatch, or an agent's `logic_hook`) with JSON-like `args`; the answer goes back to
    /// the caller. Every use is logged with the caller's reason.
    fn hook(&self, _cx: &mut Cx, name: &str, _args: &Value) -> Result<Value, SdkError> {
        Err(SdkError::new("unknown_hook", format!("this module has no hook '{name}'")))
    }

    /// About to be replaced by a new build ([`StopReason::Reload`]) or unloaded. Changes made
    /// here are kept (e.g. delete what the module spawned on unload).
    fn stop(&self, _cx: &mut Cx, _reason: StopReason) -> Result<(), SdkError> {
        Ok(())
    }
}

/// What [`Module::describe`] answers.
#[derive(Clone, Debug, Default)]
pub struct ModuleInfo {
    /// `[a-z][a-z0-9_]{0,31}`: unique among loaded modules; reloads replace by name. World
    /// changes are attributed to `module:<name>`.
    pub name: &'static str,
    /// Free-form (e.g. `env!("CARGO_PKG_VERSION")`).
    pub version: &'static str,
    /// One line for agents (`logic_status`).
    pub description: &'static str,
    /// Lower runs first each tick (ties by name).
    pub order: i32,
    /// The module's input actions as a `specula.input.v1` action map (JSON text, e.g.
    /// `include_str!("../actions.json")`), or empty for none. The engine turns window,
    /// gamepad and agent input into these actions once per tick ([`Cx::action`]).
    pub actions: &'static str,
    /// Native hooks other systems may call by name (see [`Module::hook`]).
    pub hooks: Vec<HookDef>,
}

/// A named native behaviour a module implements in code: Lex's escape hatch for what its
/// building blocks can't express yet. Called as `<module>/<name>`.
#[derive(Clone, Debug, Default)]
pub struct HookDef {
    /// `[a-z][a-z0-9_]{0,63}`.
    pub name: &'static str,
    /// What it does and when to use it (shown to agents and Lex).
    pub doc: &'static str,
    /// The arguments as a JSON Schema object (JSON text), or empty for none.
    pub args: &'static str,
}

impl ModuleInfo {
    pub(crate) fn to_value(&self) -> Value {
        let hooks = self
            .hooks
            .iter()
            .map(|h| Value::map().with("name", h.name).with("doc", h.doc).with("args", h.args))
            .collect::<Vec<_>>();
        Value::map()
            .with("name", self.name)
            .with("version", self.version)
            .with("description", self.description)
            .with("order", self.order)
            .with("actions", self.actions)
            .with("hooks", hooks)
            .with("sdk", env!("CARGO_PKG_VERSION"))
    }
}

/// Why [`Module::stop`] runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// A new build of this module is taking over.
    Reload,
    /// The module is being unloaded (`logic_unload`).
    Unload,
}

impl StopReason {
    pub(crate) fn parse(s: &str) -> StopReason {
        if s == "reload" {
            StopReason::Reload
        } else {
            StopReason::Unload
        }
    }
}

/// An error with a machine-readable code (`not_found`, `invalid_field`...), a message that says
/// what to change, and optional details. Engine refusals arrive as this too.
#[derive(Clone, Debug, PartialEq)]
pub struct SdkError {
    pub code: String,
    pub message: String,
    /// Extra fields (`field`, `path`...), as the engine sent them.
    pub details: Value,
}

impl SdkError {
    pub fn new(code: &str, message: impl Into<String>) -> SdkError {
        SdkError { code: code.to_owned(), message: message.into(), details: Value::Null }
    }

    pub(crate) fn to_value(&self) -> Value {
        let mut v = match &self.details {
            Value::Map(_) => self.details.clone(),
            _ => Value::map(),
        };
        v.insert("code", self.code.as_str());
        v.insert("message", self.message.as_str());
        v
    }

    pub(crate) fn from_value(v: &Value) -> SdkError {
        let code = v.get("code").and_then(Value::as_str).unwrap_or("error");
        let message = v.get("message").and_then(Value::as_str).unwrap_or("the engine refused the request");
        SdkError { code: code.to_owned(), message: message.to_owned(), details: v.clone() }
    }
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SdkError {}

/// An entity id (stable for the entity's life; never reused).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub u64);

/// One action's state for this tick (see [`ModuleInfo::actions`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Action {
    /// Buttons: down at the tick. Axes: non-zero.
    pub held: bool,
    /// Became held since the last tick.
    pub pressed: bool,
    /// Stopped being held since the last tick.
    pub released: bool,
    /// Axis value (`[v, 0]` for one-number axes; `[1, 0]` / `[0, 0]` for buttons).
    pub value: [f32; 2],
}

/// Something a module emitted last tick ([`Cx::emit`]); every module sees every event once, in
/// emission order.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: String,
    pub data: Value,
    /// The emitting module's name (or `engine`).
    pub from: String,
    pub tick: u64,
}

/// An entity as [`Cx::entity`] read it (the same fields `world_query` returns to agents).
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub id: Id,
    /// The full entity description.
    pub data: Value,
}

impl Entity {
    pub fn path(&self) -> &str {
        self.data.get("path").and_then(Value::as_str).unwrap_or_default()
    }

    /// Local position (relative to the parent).
    pub fn position(&self) -> Vec3 {
        self.data.get("position").and_then(Value::as_vec3).unwrap_or(Vec3::ZERO)
    }

    pub fn rotation_deg(&self) -> Vec3 {
        self.data.get("rotation_deg").and_then(Value::as_vec3).unwrap_or(Vec3::ZERO)
    }

    pub fn version(&self) -> u64 {
        self.data.get("version").and_then(Value::as_u64).unwrap_or(0)
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.data.get("tags").and_then(Value::as_list).is_some_and(|t| t.iter().any(|v| v.as_str() == Some(tag)))
    }

    /// `props[key]`, or null.
    pub fn prop(&self, key: &str) -> &Value {
        static NULL: Value = Value::Null;
        self.data.get("props").and_then(|p| p.get(key)).unwrap_or(&NULL)
    }
}

/// What to spawn: the same fields as the `world_spawn` tool (`shape`, `name`, `parent`,
/// `position`, `rotation_deg`, `scale`, `color`, `tags`, `props`, `collider`, `profile`...),
/// validated by the engine exactly as for agents.
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn(pub Value);

impl Spawn {
    pub fn new(shape: &str) -> Spawn {
        Spawn(Value::map().with("shape", shape))
    }
    /// Any `world_spawn` field.
    pub fn field(mut self, key: &str, value: impl Into<Value>) -> Spawn {
        self.0.insert(key, value);
        self
    }
    pub fn name(self, name: &str) -> Spawn {
        self.field("name", name)
    }
    /// Parent path (`/ambulo`) or id.
    pub fn parent(self, parent: &str) -> Spawn {
        self.field("parent", parent)
    }
    pub fn position(self, p: Vec3) -> Spawn {
        self.field("position", p)
    }
    pub fn scale(self, s: Vec3) -> Spawn {
        self.field("scale", s)
    }
    /// sRGB `"#rrggbb"`.
    pub fn color(self, c: &str) -> Spawn {
        self.field("color", c)
    }
    pub fn tags(self, tags: &[&str]) -> Spawn {
        self.field("tags", tags.to_vec())
    }
    pub fn props(self, props: Value) -> Spawn {
        self.field("props", props)
    }
}

/// What to change: the same fields as the `world_set` tool (`position`, `rotation_deg`, `scale`,
/// `color`, `props` (merged; null removes a key), `add_tags`, `remove_tags`, `visible`...).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Patch(pub Value);

impl Patch {
    pub fn new() -> Patch {
        Patch(Value::map())
    }
    /// Any `world_set` field.
    pub fn field(mut self, key: &str, value: impl Into<Value>) -> Patch {
        self.0.insert(key, value);
        self
    }
    pub fn position(self, p: Vec3) -> Patch {
        self.field("position", p)
    }
    pub fn rotation_deg(self, r: Vec3) -> Patch {
        self.field("rotation_deg", r)
    }
    /// Merged into the entity's props key by key (a null value removes that key).
    pub fn props(self, props: Value) -> Patch {
        self.field("props", props)
    }
}

/// What [`Cx::move_character`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moved {
    /// Where the character is now (its entity's position was set to it).
    pub position: Vec3,
    /// Standing on something.
    pub grounded: bool,
    /// Something stopped part of the horizontal motion.
    pub blocked: bool,
}

/// Everything a module may touch during one call. Each method is a request to the engine,
/// applied immediately and deterministically (the world, the state store, the RNG streams).
pub struct Cx<'a> {
    host: &'a HostApi,
    tick: u64,
    tick_hz: u32,
    reloaded: bool,
    generation: u64,
    actions: BTreeMap<String, Action>,
    events: Vec<Event>,
}

impl<'a> Cx<'a> {
    pub(crate) fn new(host: &'a HostApi, input: &Value) -> Cx<'a> {
        let num = |k: &str| input.get(k).and_then(Value::as_u64).unwrap_or(0);
        let mut actions = BTreeMap::new();
        if let Some(m) = input.get("actions").and_then(Value::as_map) {
            for (name, a) in m {
                // [held, pressed, released, x, y]
                if let Some([h, p, r, x, y]) = a.as_list() {
                    let b = |v: &Value| v.as_bool().unwrap_or(false);
                    let f = |v: &Value| v.as_f32().unwrap_or(0.0);
                    actions.insert(name.clone(), Action { held: b(h), pressed: b(p), released: b(r), value: [f(x), f(y)] });
                }
            }
        }
        let events = input
            .get("events")
            .and_then(Value::as_list)
            .unwrap_or_default()
            .iter()
            .map(|e| Event {
                name: e.get("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
                data: e.get("data").cloned().unwrap_or_default(),
                from: e.get("from").and_then(Value::as_str).unwrap_or_default().to_owned(),
                tick: e.get("tick").and_then(Value::as_u64).unwrap_or(0),
            })
            .collect();
        Cx {
            host,
            tick: num("tick"),
            tick_hz: num("tick_hz").clamp(1, 1000) as u32,
            reloaded: input.get("reloaded").and_then(Value::as_bool).unwrap_or(false),
            generation: num("generation"),
            actions,
            events,
        }
    }

    /// Ticks completed before this one: the only clock a module sees.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn tick_hz(&self) -> u32 {
        self.tick_hz
    }

    /// Seconds per tick.
    pub fn dt(&self) -> f32 {
        1.0 / self.tick_hz as f32
    }

    /// In [`Module::start`]: this build replaced a running one.
    pub fn reloaded(&self) -> bool {
        self.reloaded
    }

    /// How many times this module has been loaded in this engine (1 = first).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// An action's state this tick (neutral if unknown or idle).
    pub fn action(&self, name: &str) -> Action {
        self.actions.get(name).copied().unwrap_or_default()
    }

    /// Every action's state, by name.
    pub fn actions(&self) -> &BTreeMap<String, Action> {
        &self.actions
    }

    /// Events emitted during the previous tick.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    fn ask(&mut self, req: Value) -> Result<Value, SdkError> {
        abi::request(self.host, &req)
    }

    /// Raw request (`{"op": ..., ...}`) for ops this SDK version has no method for yet.
    pub fn request(&mut self, req: Value) -> Result<Value, SdkError> {
        self.ask(req)
    }

    /// The entity at `path` (`/ambulo/player`), if any.
    pub fn find(&mut self, path: &str) -> Result<Option<Id>, SdkError> {
        Ok(self.ask(Value::map().with("op", "find").with("path", path))?.as_u64().map(Id))
    }

    /// Reads an entity (`None` if it doesn't exist).
    pub fn entity(&mut self, id: Id) -> Result<Option<Entity>, SdkError> {
        let v = self.ask(Value::map().with("op", "get").with("id", id.0))?;
        Ok((!v.is_null()).then_some(Entity { id, data: v }))
    }

    /// Ids of entities with `tag` (and under `path_prefix`, if given), in id order, at most
    /// `limit` (≤ 4096).
    pub fn query(&mut self, tag: Option<&str>, path_prefix: Option<&str>, limit: u32) -> Result<Vec<Id>, SdkError> {
        let v = self.ask(Value::map().with("op", "query").with("tag", tag).with("path_prefix", path_prefix).with("limit", limit))?;
        Ok(v.as_list().unwrap_or_default().iter().filter_map(Value::as_u64).map(Id).collect())
    }

    /// Entities tagged `tag` whose world position is within `radius` metres of `at`, by id (at
    /// most `limit`): read from the engine's tag index, never a walk of the world, and the limit
    /// counts only what is in reach. Use it for "what is near me" (pickups, targets, talkers).
    /// Refused (`limit`) when more than 65,536 entities carry the tag.
    pub fn query_near(&mut self, tag: &str, at: Vec3, radius: f32, limit: u32) -> Result<Vec<Id>, SdkError> {
        let near = Value::map().with("at", at).with("radius", radius);
        let v = self.ask(Value::map().with("op", "query").with("tag", tag).with("near", near).with("limit", limit))?;
        Ok(v.as_list().unwrap_or_default().iter().filter_map(Value::as_u64).map(Id).collect())
    }

    pub fn spawn(&mut self, spawn: Spawn) -> Result<Id, SdkError> {
        let v = self.ask(Value::map().with("op", "spawn").with("spec", spawn.0))?;
        v.as_u64().map(Id).ok_or_else(|| SdkError::new("bad_answer", "spawn answered no id"))
    }

    /// Changes an entity; returns its new version.
    pub fn set(&mut self, id: Id, patch: Patch) -> Result<u64, SdkError> {
        Ok(self.ask(Value::map().with("op", "set").with("id", id.0).with("patch", patch.0))?.as_u64().unwrap_or(0))
    }

    /// Deletes an entity and everything under it; false if it didn't exist.
    pub fn delete(&mut self, id: Id) -> Result<bool, SdkError> {
        Ok(self.ask(Value::map().with("op", "delete").with("id", id.0))?.as_bool().unwrap_or(false))
    }

    /// The module's engine-owned state under `key` (null if unset). Survives reloads.
    pub fn state(&mut self, key: &str) -> Result<Value, SdkError> {
        self.ask(Value::map().with("op", "state_get").with("key", key))
    }

    /// Sets (null: removes) the module's state under `key`.
    pub fn set_state(&mut self, key: &str, value: impl Into<Value>) -> Result<(), SdkError> {
        self.ask(Value::map().with("op", "state_set").with("key", key).with("value", value.into())).map(drop)
    }

    /// The next 32 random bits of the engine stream `stream` (seeded per module and stream, so
    /// a new stream never shifts another; the position survives reloads).
    ///
    /// A module keeps at most 1,024 streams: a new name past that fails with code `rng_full`.
    /// Name streams by purpose (`"loot"`, `"spawns"`), never one per entity or tick (that
    /// grows without bound); for per-entity randomness draw from a purpose stream in a fixed order
    /// (entities sorted by id).
    pub fn random_u32(&mut self, stream: &str) -> Result<u32, SdkError> {
        let v = self.ask(Value::map().with("op", "rng").with("stream", stream))?;
        Ok(v.as_u64().unwrap_or(0) as u32)
    }

    /// Uniform in `0..n` (n ≥ 1).
    pub fn random_below(&mut self, stream: &str, n: u32) -> Result<u32, SdkError> {
        let v = self.ask(Value::map().with("op", "rng").with("stream", stream).with("below", n.max(1)))?;
        Ok(v.as_u64().unwrap_or(0) as u32)
    }

    /// Uniform in `[0, 1)`.
    pub fn random_f32(&mut self, stream: &str) -> Result<f32, SdkError> {
        Ok((self.random_u32(stream)? >> 8) as f32 / (1u32 << 24) as f32)
    }

    /// Moves a character entity by `motion` (metres this tick, gravity included) through the
    /// engine's character controller, which stops it at the ground and slides it along walls,
    /// and sets the entity's position. The engine answers it with the physics character
    /// controller (P2): steps, slopes, moving platforms and pushing dynamic bodies.
    pub fn move_character(&mut self, id: Id, motion: Vec3) -> Result<Moved, SdkError> {
        let v = self.ask(Value::map().with("op", "move_character").with("id", id.0).with("motion", motion))?;
        Ok(Moved {
            position: v.get("position").and_then(Value::as_vec3).unwrap_or(Vec3::ZERO),
            grounded: v.get("grounded").and_then(Value::as_bool).unwrap_or(false),
            blocked: v.get("blocked").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    /// Emits an event every module sees next tick (and agents in `logic_status`).
    pub fn emit(&mut self, name: &str, data: impl Into<Value>) -> Result<(), SdkError> {
        self.ask(Value::map().with("op", "emit").with("name", name).with("data", data.into())).map(drop)
    }

    /// A line in the module's log (`logic_status`); bounded, so keep it occasional. It is also a
    /// `module.log` trace event.
    pub fn log(&mut self, message: &str) -> Result<(), SdkError> {
        self.ask(Value::map().with("op", "log").with("message", message)).map(drop)
    }

    /// A structured trace event (docs/design/TRACE.md) under the subsystem `module:<name>`:
    /// `cx.trace(TraceLevel::Info, "door.opened", Value::map().with("door", id.0))`. `code` is a
    /// machine code (`^[a-z0-9_.]{1,64}$`); `fields` a map of numbers, strings and booleans (at
    /// most 16). Agents read it with `trace_query {subsystem: "module:<name>"}`, tied to the
    /// tick (and to the agent call, for hooks an agent called).
    ///
    /// The engine records it only at the levels its settings ask for, and always answers the
    /// same (nothing in a trace feeds back into the simulation). At most 64 events per call.
    pub fn trace(&mut self, level: TraceLevel, code: &str, fields: impl Into<Value>) -> Result<(), SdkError> {
        self.ask(Value::map().with("op", "trace").with("level", level.as_str()).with("code", code).with("fields", fields.into())).map(drop)
    }

    /// Plays a sound by name (`[a-z][a-z0-9_]{0,63}`: a lowercase letter first, so `"1up"` is
    /// refused): a sound recipe the engine finds in the
    /// module's own `sounds/<name>.json`, else the project's assets (`sounds/<name>.json`).
    /// `at` places it in the world (spatial), else it plays flat; `gain` is 0..4 (1 = as
    /// authored). It goes to the `sfx` bus, recorded as played by `module:<name>`
    /// (`audio_played`).
    ///
    /// **Presentation only:** a sound never feeds back into the simulation. The engine plays it
    /// after the call commits (a failed call plays nothing), skips it when audio is off or the
    /// recipe can't be found or rendered (traced, never an error), and answers the same either
    /// way, so runs with and without audio stay identical. At most 16 sounds per call.
    pub fn sound(&mut self, name: &str, at: Option<Vec3>, gain: f32) -> Result<(), SdkError> {
        self.ask(Value::map().with("op", "sound").with("name", name).with("at", at).with("gain", gain)).map(drop)
    }
}

/// How much a module's trace event matters ([`Cx::trace`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl TraceLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            TraceLevel::Error => "error",
            TraceLevel::Warn => "warn",
            TraceLevel::Info => "info",
            TraceLevel::Debug => "debug",
            TraceLevel::Trace => "trace",
        }
    }
}
