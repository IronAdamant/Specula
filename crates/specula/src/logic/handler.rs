//! Answers a module's requests during one call: world reads and commands, the state store, RNG
//! streams, the character controller, events and log lines. Everything a call changes outside
//! the world (state, RNG positions, events) is undone if the call fails, as the world is.

use super::character::CharacterMover;
use super::{EventRec, LogLine, Shared, LOG_LINES, MAX_PENDING_EVENTS, MAX_RNG_STREAMS};
use crate::world::{EntityKey, World, WorldError};
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

const OPS: &str = "find, get, query, spawn, set, delete, state_get, state_set, rng, move_character, emit, log";

pub(super) struct ModuleHandler<'a> {
    pub world: &'a mut World,
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
            "move_character" => {
                let id = id_arg(req)?;
                let motion = req.get("motion").and_then(Value::as_vec3).ok_or_else(|| refuse("invalid_field", "'motion' must be [x, y, z]"))?;
                let m = self.mover.move_character(self.world, id, motion, self.agent).map_err(|e| error_value(&e))?;
                Ok(Value::map().with("position", m.position).with("grounded", m.grounded).with("blocked", m.blocked))
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
                self.log.push_back(LogLine { tick: self.tick, module: self.module.to_owned(), message: msg });
                Ok(Value::Null)
            }
            other => Err(refuse("unknown_op", format!("no request '{other}' (this engine answers: {OPS}); is the module's SDK newer than the engine?"))),
        }
    }
}
