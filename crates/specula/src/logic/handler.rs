//! Answers a module's requests during one call: world reads and commands, the state store, RNG
//! streams, the character controller, events and log lines. Everything a call changes outside
//! the world (state, RNG positions, events) is undone if the call fails, as the world is.

use super::character::CharacterMover;
use super::{EventRec, LogLine, Shared, SoundCue, LOG_LINES, MAX_PENDING_EVENTS, MAX_RNG_STREAMS};
use crate::write_policy::WorldWrite;
use crate::world::{EntityKey, WorldError};
use specula_core::rng::Rng;
use specula_core::Json;
use specula_sdk::Value;
use std::collections::VecDeque;

/// Most entity ids one `query` returns.
pub const MAX_QUERY: u64 = 4096;
/// Most events one module may emit per call.
pub const MAX_EVENTS_PER_CALL: usize = 256;
/// Largest event data, encoded.
pub const MAX_EVENT_BYTES: usize = 4096;
/// A module's state store: at most this many keys and encoded bytes.
pub const MAX_STATE_KEYS: usize = 1024;
pub const MAX_STATE_BYTES: usize = 1 << 20;
/// Longest log line kept (longer ones are cut).
const MAX_LOG_CHARS: usize = 400;

const OPS: &str = "find, get, query, spawn, set, delete, state_get, state_set, rng, rng_restart, move_character, touching, emit, log, trace, sound";
/// Most sounds one module call may ask for (the SDK's number, so modules can budget for it).
pub const MAX_SOUNDS_PER_CALL: usize = specula_sdk::MAX_SOUNDS_PER_CALL;
/// Tagged entities one `query {tag, near}` looks at, at most (the tag index's entries, never the
/// world): past it the query is refused (`limit`), never cut short silently.
pub const MAX_NEAR_SCAN: usize = 65_536;
/// Largest gain a module's sound may ask for (1 = as authored).
pub const MAX_SOUND_GAIN: f64 = 4.0;
/// Farthest a module's sound may be placed from the origin, metres.
const MAX_SOUND_COORD: f32 = 1.0e6;
/// Trace events one module call may record (more are dropped, and counted on the last).
pub const MAX_TRACE_PER_CALL: usize = 64;

pub(super) struct ModuleHandler<'a> {
    pub world: &'a mut dyn WorldWrite,
    pub module: &'a str,
    /// `module:<name>`: attribution of the module's world changes.
    pub agent: &'a str,
    pub tick: u64,
    pub shared: &'a mut Shared,
    pub mover: &'a mut dyn CharacterMover,
    /// Events emitted by this call (merged by the host only if the call succeeds).
    pub events: Vec<EventRec>,
    /// Events already waiting for the next tick (they and this call's share
    /// [`MAX_PENDING_EVENTS`]).
    pub pending_events: usize,
    pub log: &'a mut VecDeque<LogLine>,
    /// `(key, value before)` for every state write, to undo a failed call.
    pub state_undo: Vec<(String, Option<Value>)>,
    /// `(stream, state before)` for every stream touched, to undo a failed call.
    pub rng_undo: Vec<(String, Option<Rng>)>,
    /// Trace events this call recorded (at most [`MAX_TRACE_PER_CALL`]).
    pub traced: usize,
    /// Sounds this call asked for (queued by the host only if the call commits).
    pub sounds: Vec<SoundCue>,
}

/// The SDK's error map from a world error: `{code, message, ...details}`.
pub fn error_value(e: &WorldError) -> Value {
    let mut v = Value::map().with("code", e.code).with("message", e.message.as_str());
    for (k, d) in &e.details {
        v.insert(k, Value::from_json(d));
    }
    v
}

fn refuse(code: &'static str, message: impl Into<String>) -> Value {
    error_value(&WorldError::new(code, message))
}

fn id_arg(req: &Value) -> Result<u64, Value> {
    req.get("id").and_then(Value::as_u64).ok_or_else(|| refuse("invalid_field", "'id' must be an entity id"))
}

fn str_arg<'v>(req: &'v Value, key: &str) -> Result<&'v str, Value> {
    req.get(key).and_then(Value::as_str).ok_or_else(|| refuse("invalid_field", format!("'{key}' must be a string")))
}

impl specula_logic::Handler for ModuleHandler<'_> {
    fn request(&mut self, req: &Value) -> Result<Value, Value> {
        let op = req.get("op").and_then(Value::as_str).unwrap_or_default();
        if matches!(op, "spawn" | "set" | "delete" | "move_character") {
            // The reserved-data rules hold for modules as for agents (`/lex`, `/lex_live`,
            // `/project`, `/nav`): a write that breaks one is undone and refused.
            let mark = self.world.mark();
            let out = self.answer(op, req)?;
            crate::write_policy::module_request(self.world, mark).map_err(|e| error_value(&e))?;
            return Ok(out);
        }
        self.answer(op, req)
    }
}

impl ModuleHandler<'_> {
    fn answer(&mut self, op: &str, req: &Value) -> Result<Value, Value> {
        match op {
            "find" => match self.world.find_path(str_arg(req, "path")?) {
                Ok(id) => Ok(Value::from(id)),
                Err(e) if e.code == "not_found" => Ok(Value::Null),
                Err(e) => Err(error_value(&e)),
            },
            "get" => {
                let id = id_arg(req)?;
                Ok(match self.world.get(id) {
                    Some(e) => Value::from_json(&e.to_json(&self.world.path_of(id))),
                    None => Value::Null,
                })
            }
            "query" => {
                let limit = req.get("limit").and_then(Value::as_u64).unwrap_or(256).clamp(1, MAX_QUERY) as usize;
                let tag = req.get("tag").and_then(Value::as_str);
                let prefix = req.get("path_prefix").and_then(Value::as_str);
                // `near: {at: [x, y, z], radius}` (with a tag): entities whose world position is
                // within `radius` of `at`, read from the tag's index, so the limit counts only
                // what is in reach (review of m5-gmods, major 3).
                if let Some(near) = req.get("near") {
                    let at = near.get("at").and_then(Value::as_vec3).filter(|v| v.x.is_finite() && v.y.is_finite() && v.z.is_finite()).ok_or_else(|| refuse("invalid_field", "'near.at' must be [x, y, z]"))?;
                    let radius = near.get("radius").and_then(Value::as_f64).filter(|r| r.is_finite() && *r >= 0.0).ok_or_else(|| refuse("invalid_field", "'near.radius' must be a number >= 0"))? as f32;
                    let tag = tag.ok_or_else(|| refuse("invalid_field", "'near' needs a 'tag' (it reads the tag's index, never the whole world)"))?;
                    let world = self.world.as_world();
                    let n = world.tagged_count(tag);
                    if n > MAX_NEAR_SCAN {
                        return Err(refuse("limit", format!("{n} entities carry tag '{tag}'; a near query reads at most {MAX_NEAR_SCAN}: use a narrower tag")));
                    }
                    let mut ids = Vec::new();
                    for id in world.tagged(tag) {
                        if ids.len() == limit {
                            break;
                        }
                        let Some(e) = world.get(id) else { continue };
                        let d = e.world_position() - at;
                        if d.dot(d) > radius * radius {
                            continue;
                        }
                        if let Some(p) = prefix {
                            let path = world.path_of(id);
                            if !(path == p || path.strip_prefix(p).is_some_and(|rest| rest.starts_with('/') || p.ends_with('/'))) {
                                continue;
                            }
                        }
                        ids.push(Value::from(id));
                    }
                    return Ok(Value::List(ids));
                }
                let mut ids = Vec::new();
                for e in self.world.entities() {
                    if ids.len() == limit {
                        break;
                    }
                    if tag.is_some_and(|t| !e.tags.contains(t)) {
                        continue;
                    }
                    if let Some(p) = prefix {
                        let path = self.world.path_of(e.id);
                        let under = path == p || path.strip_prefix(p).is_some_and(|rest| rest.starts_with('/') || p.ends_with('/'));
                        if !under {
                            continue;
                        }
                    }
                    ids.push(Value::from(e.id));
                }
                Ok(Value::List(ids))
            }
            "spawn" => {
                let spec = req.get("spec").map(Value::to_json).unwrap_or(Json::Null);
                let spec = crate::tools::parse_spawn(&spec).map_err(|m| refuse("invalid_field", m))?;
                self.world.spawn(spec, self.agent).map(|e| Value::from(e.id)).map_err(|e| error_value(&e))
            }
            "set" => {
                let id = id_arg(req)?;
                let mut patch = req.get("patch").map(Value::to_json).unwrap_or_else(|| Json::obj(Vec::<(String, Json)>::new()));
                if !matches!(patch, Json::Obj(_)) {
                    return Err(refuse("invalid_field", "'patch' must be a map of world_set fields"));
                }
                if patch.get("id").is_some() || patch.get("path").is_some() || patch.get("name").is_some() {
                    return Err(refuse("invalid_field", "the patch names no entity (the id is given separately); to rename use 'rename'"));
                }
                patch.set("id", Json::from(id));
                let (key, change) = crate::tools::parse_set(&patch).map_err(|m| refuse("invalid_field", m))?;
                self.world.update(&key, change, self.agent).map(|e| Value::from(e.version)).map_err(|e| error_value(&e))
            }
            "delete" => {
                let id = id_arg(req)?;
                match self.world.delete(&EntityKey::Id(id), None) {
                    Ok(_) => Ok(Value::Bool(true)),
                    Err(e) if e.code == "not_found" => Ok(Value::Bool(false)),
                    Err(e) => Err(error_value(&e)),
                }
            }
            "state_get" => Ok(self.shared.store.get(str_arg(req, "key")?).cloned().unwrap_or_default()),
            "state_set" => {
                let key = str_arg(req, "key")?;
                if key.is_empty() || key.len() > 128 {
                    return Err(refuse("invalid_field", "'key' must be 1..128 bytes"));
                }
                let value = req.get("value").cloned().unwrap_or_default();
                let grows = !self.shared.store.contains_key(key) && !value.is_null();
                if grows && self.shared.store.len() >= MAX_STATE_KEYS {
                    return Err(refuse("state_full", format!("the state store holds at most {MAX_STATE_KEYS} keys; remove some (set them to null)")));
                }
                let before = self.shared.put(key, (!value.is_null()).then_some(value));
                if self.shared.store_bytes > MAX_STATE_BYTES {
                    // Put it back before refusing.
                    self.shared.put(key, before);
                    return Err(refuse("state_full", format!("the state store holds at most {MAX_STATE_BYTES} bytes; keep big data in entity props or files")));
                }
                self.state_undo.push((key.to_owned(), before));
                Ok(Value::Null)
            }
            "rng" => {
                let stream = str_arg(req, "stream")?;
                if stream.is_empty() || stream.len() > 64 {
                    return Err(refuse("invalid_field", "'stream' must be 1..64 bytes"));
                }
                let before = self.shared.rngs().get(stream).copied();
                if before.is_none() && self.shared.rngs().len() >= MAX_RNG_STREAMS {
                    return Err(error_value(
                        &WorldError::new(
                            "rng_full",
                            format!(
                                "this module already has {MAX_RNG_STREAMS} RNG streams, the most it may keep; draw from a few streams named by purpose (\"loot\", \"spawns\"), not one per entity or tick"
                            ),
                        )
                        .with("stream", stream)
                        .with("limit", MAX_RNG_STREAMS),
                    ));
                }
                if !self.rng_undo.iter().any(|(s, _)| s == stream) {
                    self.rng_undo.push((stream.to_owned(), before));
                }
                let seed = self.shared.seed;
                let module = self.module;
                let below = req.get("below").and_then(Value::as_u64);
                let make = || Rng::named(seed, &format!("module/{module}/{stream}"));
                Ok(self.shared.draw(stream, make, |rng| match below {
                    Some(n) => Value::from(rng.below(n.clamp(1, u64::from(u32::MAX)) as u32)),
                    None => Value::from(rng.next_u32()),
                }))
            }
            // A stream put back at its beginning: its next number is the first it ever gave.
            // For a run that starts again and must be the same game. Undone like a draw.
            "rng_restart" => {
                let stream = str_arg(req, "stream")?;
                if stream.is_empty() || stream.len() > 64 {
                    return Err(refuse("invalid_field", "'stream' must be 1..64 bytes"));
                }
                let before = self.shared.rngs().get(stream).copied();
                // (A stream never drawn from is at its beginning already: none is made for it.)
                if before.is_some() {
                    if !self.rng_undo.iter().any(|(s, _)| s == stream) {
                        self.rng_undo.push((stream.to_owned(), before));
                    }
                    let (seed, module) = (self.shared.seed, self.module);
                    self.shared.put_rng(stream.to_owned(), Some(Rng::named(seed, &format!("module/{module}/{stream}"))));
                }
                Ok(Value::Null)
            }
            "move_character" => {
                let id = id_arg(req)?;
                let motion = req.get("motion").and_then(Value::as_vec3).ok_or_else(|| refuse("invalid_field", "'motion' must be [x, y, z]"))?;
                let m = self.mover.move_character(self.world, id, motion, self.agent).map_err(|e| error_value(&e))?;
                Ok(Value::map().with("position", m.position).with("grounded", m.grounded).with("blocked", m.blocked))
            }
            // What touches an entity now, tagged or not (docs/design/MODULES.md, "What physics
            // tells a module"): its ids, at most MAX_TOUCHING, and how many more there are.
            "touching" => {
                let id = id_arg(req)?;
                if self.world.get(id).is_none() {
                    return Err(refuse("not_found", format!("no entity with id {id}")));
                }
                let all = self.mover.touching(id).ok_or_else(|| refuse("no_physics", "there is no physics here to ask what touches a thing"))?;
                let most = crate::physics_told::MAX_TOUCHING;
                Ok(Value::map().with("ids", all.iter().take(most).map(|k| Value::from(*k)).collect::<Vec<Value>>()).with("more", all.len().saturating_sub(most)))
            }
            "emit" => {
                let name = str_arg(req, "name")?;
                if !specula_sdk::valid_name(name, 64) {
                    return Err(refuse("invalid_field", "an event name is [a-z][a-z0-9_]{0,63}"));
                }
                if self.events.len() >= MAX_EVENTS_PER_CALL {
                    return Err(refuse("too_many_events", format!("at most {MAX_EVENTS_PER_CALL} events per call")));
                }
                if self.pending_events + self.events.len() >= MAX_PENDING_EVENTS {
                    return Err(refuse(
                        "too_many_events",
                        format!("{MAX_PENDING_EVENTS} events are already waiting for the next tick; emit fewer"),
                    ));
                }
                let data = req.get("data").cloned().unwrap_or_default();
                let bytes = data.encode().len();
                if bytes > MAX_EVENT_BYTES {
                    return Err(refuse("event_too_big", format!("event data is {bytes} bytes encoded; at most {MAX_EVENT_BYTES} (keep big data in the state store or entity props)")));
                }
                self.events.push(EventRec { name: name.to_owned(), data, from: self.module.to_owned(), tick: self.tick });
                Ok(Value::Null)
            }
            "log" => {
                let msg = str_arg(req, "message")?;
                let msg: String = msg.chars().take(MAX_LOG_CHARS).collect();
                if self.log.len() == LOG_LINES {
                    self.log.pop_front();
                }
                let sub = specula_core::trace::subsystem(&format!("module:{}", self.module));
                specula_core::trace_event!(sub, Info, "module.log", message = msg.as_str());
                self.log.push_back(LogLine { tick: self.tick, module: self.module.to_owned(), message: msg });
                Ok(Value::Null)
            }
            "trace" => {
                trace_request(self.module, req, &mut self.traced)?;
                Ok(Value::Null)
            }
            "sound" => {
                // Presentation only: checked for shape, then queued; whether it plays (audio
                // off, recipe missing) never changes the answer.
                let name = str_arg(req, "name")?;
                if !specula_sdk::valid_name(name, 64) {
                    return Err(refuse("invalid_field", "a sound name is [a-z][a-z0-9_]{0,63} (a recipe sounds/<name>.json)"));
                }
                let gain = match req.get("gain") {
                    None | Some(Value::Null) => 1.0,
                    Some(g) => g.as_f64().filter(|g| g.is_finite() && (0.0..=MAX_SOUND_GAIN).contains(g)).ok_or_else(|| refuse("invalid_field", format!("'gain' must be 0..{MAX_SOUND_GAIN}")))?,
                };
                let at = match req.get("at") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(
                        v.as_vec3()
                            .filter(|p| [p.x, p.y, p.z].iter().all(|c| c.is_finite() && c.abs() <= MAX_SOUND_COORD))
                            .map(|p| [p.x, p.y, p.z])
                            .ok_or_else(|| refuse("invalid_field", "'at' must be null or [x, y, z] within 1e6 m"))?,
                    ),
                };
                if self.sounds.len() >= MAX_SOUNDS_PER_CALL {
                    return Err(refuse("too_many_sounds", format!("at most {MAX_SOUNDS_PER_CALL} sounds per call")));
                }
                self.sounds.push(SoundCue { module: self.module.to_owned(), name: name.to_owned(), at, gain: gain as f32, tick: self.tick });
                Ok(Value::Null)
            }
            other => Err(refuse("unknown_op", format!("no request '{other}' (this engine answers: {OPS}); is the module's SDK newer than the engine?"))),
        }
    }
}

/// A module's `trace` request ([`specula_sdk::Cx::trace`]): an event under `module:<name>`. The
/// answer never depends on the trace settings (nothing in a trace feeds back into the
/// simulation): only a malformed level or code is refused.
fn trace_request(module: &str, req: &Value, traced: &mut usize) -> Result<(), Value> {
    use specula_core::trace::{self, Level};
    let level = str_arg(req, "level")?;
    let Some(level) = Level::parse(level) else {
        return Err(refuse("invalid_field", format!("trace level '{level}' must be error, warn, info, debug or trace")));
    };
    let code = str_arg(req, "code")?;
    if !valid_trace_name(code) {
        return Err(refuse("invalid_field", format!("trace code '{}' must match ^[a-z0-9_.]{{1,64}}$", code.chars().take(80).collect::<String>())));
    }
    if let Some(p) = ENGINE_CODE_PREFIXES.iter().find(|p| code.starts_with(*p)) {
        return Err(refuse("reserved", format!("trace code '{code}': codes starting '{p}' are the engine's (a module's event must never pass for a watchdog's); name it after your module, e.g. '{module}.{}'", code.trim_start_matches(*p))));
    }
    let fields = req.get("fields").and_then(Value::as_map);
    if let Some(bad) = fields.and_then(|m| m.keys().find(|k| !valid_trace_name(k))) {
        return Err(refuse("invalid_field", format!("trace field '{}' must match ^[a-z0-9_.]{{1,64}}$", bad.chars().take(80).collect::<String>())));
    }
    *traced += 1;
    if *traced > MAX_TRACE_PER_CALL {
        return Ok(());
    }
    let sub = trace::subsystem(&format!("module:{module}"));
    if !sub.enabled(level) {
        return Ok(());
    }
    let code = module_name(module, code).unwrap_or("module.names_full");
    let mut out: Vec<(&'static str, trace::Value)> = Vec::new();
    if let Some(map) = fields {
        for (k, v) in map.iter().take(trace::MAX_FIELDS - 1) {
            let key = module_name(module, k).unwrap_or("field");
            out.push((key, trace::Value::from_json(&v.to_json())));
        }
    }
    let mut fields = out;
    if *traced == MAX_TRACE_PER_CALL {
        fields.push(("more_dropped", trace::Value::Bool(true)));
    }
    sub.emit(level, code, fields);
    Ok(())
}

/// The code prefixes the engine's own events use (watchdogs, write decisions, the module host's):
/// a module may not emit them, so `trace_incidents` and agents filtering on them never see a
/// module's event pass for the engine's (review, e45f66c).
pub const ENGINE_CODE_PREFIXES: [&str; 15] =
    ["engine.", "frame.", "write.", "swarm.", "sight.", "world.", "gpu.", "jobs.", "memory.", "module.", "mcp.", "trace.", "model.", "save.", "studio"];

/// Trace codes and field names a module uses: `^[a-z0-9_.]{1,64}$`.
fn valid_trace_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

/// Distinct trace names (codes and field names) one module may use, and modules that may name
/// any: the process-wide intern table is shared, so one module can't fill it for everyone. Past
/// its cap a module's new codes become `module.names_full` (its own subsystem says whose).
pub const MODULE_NAMES: usize = 64;
const NAMING_MODULES: usize = 64;

static MODULE_NAMES_USED: std::sync::Mutex<std::collections::BTreeMap<String, std::collections::BTreeMap<String, &'static str>>> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// `name` interned for `module`, within its cap (`None` past it).
fn module_name(module: &str, name: &str) -> Option<&'static str> {
    let mut all = MODULE_NAMES_USED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !all.contains_key(module) && all.len() >= NAMING_MODULES {
        return None;
    }
    let mine = all.entry(module.to_owned()).or_default();
    if let Some(n) = mine.get(name) {
        return Some(n);
    }
    if mine.len() >= MODULE_NAMES {
        return None;
    }
    let n = specula_core::trace::intern(name)?;
    mine.insert(name.to_owned(), n);
    Some(n)
}

#[cfg(test)]
mod trace_names_tests {
    use super::*;

    /// Review probe (b893d0a, minor 10): one module's names could fill the shared intern table.
    #[test]
    fn a_module_is_capped_on_its_own() {
        for i in 0..(MODULE_NAMES + 10) {
            let got = module_name("greedy_test_module", &format!("code_{i}"));
            assert_eq!(got.is_some(), i < MODULE_NAMES, "{i}");
        }
        assert!(module_name("modest_test_module", "door.opened").is_some(), "another module still names its own");
        assert!(valid_trace_name("door.opened") && !valid_trace_name("Door") && !valid_trace_name(&"a".repeat(65)));
    }

    /// Review probe (e45f66c, minor 6): a module could emit `engine.stall` or `write.refused`.
    #[test]
    fn modules_may_not_emit_the_engines_codes() {
        let req = |code: &str| Value::map().with("level", "warn").with("code", code).with("fields", Value::map());
        let mut n = 0;
        for code in ["engine.stall", "write.refused", "module.disabled", "gpu.validation", "trace.settings_changed"] {
            let e = trace_request("probe_module", &req(code), &mut n).unwrap_err();
            assert_eq!(e.get("code").and_then(Value::as_str), Some("reserved"), "{code}");
        }
        assert!(trace_request("probe_module", &req("probe_module.door_opened"), &mut n).is_ok());
    }
}
