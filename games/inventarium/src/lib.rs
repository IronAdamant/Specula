//! Inventarium (Latin "inventory"): the example inventory module. Stacks, slots and weight, as
//! data, on any entity.
//!
//! - **Data** (`items.json`, embedded at build time; edit it and `logic_build` reloads it):
//!   each item's stack size, weight in grams (integers: no float drift in a sum), tags and a
//!   localisable name (`{key, text}`, see docs/design/GAMEPLAY_MODULES.md "Text").
//! - **State lives in the world:** an entity's inventory is its `props.inventory`
//!   (`{format, slots: [{item, count} | null], max_weight_g, weight_g}`), visible to agents in
//!   `world_query`, saved with the world and save games, journaled, undoable. The module keeps
//!   nothing else (a fresh value handles every call).
//! - **Ways in:** hooks (`logic_hook {hook: "inventarium/give", ...}`), events other modules
//!   emit (`inventarium_give`, `inventarium_take`: applied at the next tick), and the `interact`
//!   action, which picks up the nearest entity tagged `pickup` (`props.pickup = {item, count}`)
//!   in reach of the player.
//! - **Ways out:** events every module sees next tick: `inventarium_changed {owner, item, delta,
//!   count}`, `inventarium_picked`, `inventarium_refused {owner, item, count, code}`, and a
//!   pickup sound.
//!
//! Cost before work: the data is capped (items, stacks, weights) and every request is checked
//! against the caps before anything changes; at most [`MAX_REQUESTS_PER_TICK`] event requests
//! are applied a tick (the rest are refused, counted in one event).

// Modules deny unsafe code; `export_module!` allows it for its one exported symbol.
#![deny(unsafe_code)]

use specula_sdk::{Cx, HookDef, Id, Module, ModuleInfo, Patch, SdkError, Spawn, StopReason, Value, Vec3};
use std::collections::BTreeMap;

/// Item kinds the data may define.
pub const MAX_ITEMS: usize = 256;
/// Slots an inventory may have.
pub const MAX_SLOTS: u32 = 64;
/// Largest stack.
pub const MAX_STACK: u32 = 9999;
/// Heaviest item, and heaviest load, in grams.
pub const MAX_ITEM_WEIGHT_G: u64 = 1_000_000;
pub const MAX_LOAD_G: u64 = 100_000_000;
/// Event requests (`inventarium_give` / `_take`) applied per tick; the rest are refused.
pub const MAX_REQUESTS_PER_TICK: usize = 128;
/// Pickups looked at per `interact` (nearest wins).
pub const MAX_PICKUPS_SCANNED: u32 = 256;
/// The inventory layout written to `props.inventory`.
pub const FORMAT: i64 = 1;

const ROOT: &str = "/inventarium";
const DATA: &str = include_str!("../items.json");

/// A line of text a player may read: a string-table key and its English text (the fallback).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub key: String,
    pub text: String,
}

impl Text {
    fn parse(v: Option<&Value>, at: &str) -> Result<Text, SdkError> {
        let v = v.ok_or_else(|| bad(at, "is required: {key, text}"))?;
        let key = v.get("key").and_then(Value::as_str).filter(|k| valid_key(k)).ok_or_else(|| bad(&format!("{at}.key"), "must be a string-table key ([a-z0-9_.]{1,96})"))?;
        let text = v.get("text").and_then(Value::as_str).filter(|t| !t.is_empty() && t.len() <= 512).ok_or_else(|| bad(&format!("{at}.text"), "must be 1..512 bytes of English text"))?;
        Ok(Text { key: key.to_owned(), text: text.to_owned() })
    }

    pub fn to_value(&self) -> Value {
        Value::map().with("key", self.key.as_str()).with("text", self.text.as_str())
    }
}

/// `[a-z0-9_.]{1,96}`, starting with a letter.
pub fn valid_key(k: &str) -> bool {
    let b = k.as_bytes();
    !b.is_empty() && b.len() <= 96 && b[0].is_ascii_lowercase() && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'.')
}

fn bad(at: &str, what: &str) -> SdkError {
    SdkError::new("bad_data", format!("items.json: {at} {what}"))
}

/// One kind of item.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemDef {
    pub name: Text,
    pub stack: u32,
    pub weight_g: u64,
    pub tags: Vec<String>,
}

/// The module's data (`items.json`).
#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    /// The entity the window's `interact` acts for (path), if any.
    pub player: Option<String>,
    pub pickup_range_m: f32,
    pub slots: u32,
    pub max_weight_g: u64,
    pub items: BTreeMap<String, ItemDef>,
}

fn int(v: Option<&Value>, at: &str, lo: u64, hi: u64) -> Result<u64, SdkError> {
    v.and_then(Value::as_u64).filter(|n| (lo..=hi).contains(n)).ok_or_else(|| bad(at, &format!("must be a whole number {lo}..{hi}")))
}

impl Data {
    /// Parses and checks the data; every problem names its place.
    pub fn parse(text: &str) -> Result<Data, SdkError> {
        let v = Value::parse_json(text)?;
        if v.get("format").and_then(Value::as_str) != Some("specula.inventarium.v1") {
            return Err(bad("format", "must be \"specula.inventarium.v1\""));
        }
        let player = match v.get("player") {
            None | Some(Value::Null) => None,
            Some(p) => Some(p.as_str().filter(|p| p.starts_with('/') && p.len() <= 256).ok_or_else(|| bad("player", "must be an entity path"))?.to_owned()),
        };
        let range = v.get("pickup_range_m").and_then(Value::as_f32).unwrap_or(1.5);
        if !(0.1..=20.0).contains(&range) {
            return Err(bad("pickup_range_m", "must be 0.1..20"));
        }
        let d = v.get("defaults");
        let slots = int(d.and_then(|d| d.get("slots")), "defaults.slots", 1, u64::from(MAX_SLOTS))? as u32;
        let max_weight_g = int(d.and_then(|d| d.get("max_weight_g")), "defaults.max_weight_g", 1, MAX_LOAD_G)?;
        let list = v.get("items").and_then(Value::as_map).ok_or_else(|| bad("items", "must be an object of item definitions"))?;
        if list.is_empty() || list.len() > MAX_ITEMS {
            return Err(bad("items", &format!("must define 1..{MAX_ITEMS} items")));
        }
        let mut items = BTreeMap::new();
        for (id, def) in list {
            if !specula_sdk::valid_name(id, 64) {
                return Err(bad(&format!("items.{id}"), "is not a valid item id ([a-z][a-z0-9_]{0,63})"));
            }
            let at = format!("items.{id}");
            let tags = match def.get("tags") {
                None => Vec::new(),
                Some(t) => {
                    let t = t.as_list().filter(|t| t.len() <= 16).ok_or_else(|| bad(&format!("{at}.tags"), "must be a list of at most 16 tags"))?;
                    t.iter().map(|x| x.as_str().filter(|s| specula_sdk::valid_name(s, 32)).map(str::to_owned).ok_or_else(|| bad(&format!("{at}.tags"), "holds a bad tag"))).collect::<Result<_, _>>()?
                }
            };
            items.insert(
                id.clone(),
                ItemDef {
                    name: Text::parse(def.get("name"), &format!("{at}.name"))?,
                    stack: int(def.get("stack"), &format!("{at}.stack"), 1, u64::from(MAX_STACK))? as u32,
                    weight_g: int(def.get("weight_g"), &format!("{at}.weight_g"), 0, MAX_ITEM_WEIGHT_G)?,
                    tags,
                },
            );
        }
        Ok(Data { player, pickup_range_m: range, slots, max_weight_g, items })
    }

    fn def(&self, item: &str) -> Result<&ItemDef, SdkError> {
        self.items.get(item).ok_or_else(|| {
            let known: Vec<&str> = self.items.keys().map(String::as_str).take(24).collect();
            SdkError::new("unknown_item", format!("no item '{}' in items.json (items: {})", short(item), known.join(", ")))
        })
    }
}

/// An echo of agent input, cut short.
fn short(s: &str) -> String {
    s.chars().take(64).collect()
}

/// One slot's contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: String,
    pub count: u32,
}

/// An inventory: what `props.inventory` holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inventory {
    pub slots: Vec<Option<Stack>>,
    pub max_weight_g: u64,
}

impl Inventory {
    pub fn new(slots: u32, max_weight_g: u64) -> Inventory {
        Inventory { slots: vec![None; slots.clamp(1, MAX_SLOTS) as usize], max_weight_g: max_weight_g.clamp(1, MAX_LOAD_G) }
    }

    /// Reads `props.inventory` (as written by [`Inventory::to_value`]; an agent may have edited
    /// it, so everything is checked).
    pub fn from_value(v: &Value) -> Result<Inventory, SdkError> {
        let bad = |what: &str| SdkError::new("bad_inventory", format!("props.inventory: {what}"));
        if v.get("format").and_then(Value::as_i64) != Some(FORMAT) {
            return Err(bad(&format!("format must be {FORMAT}")));
        }
        let slots = v.get("slots").and_then(Value::as_list).filter(|s| !s.is_empty() && s.len() <= MAX_SLOTS as usize).ok_or_else(|| bad(&format!("slots must be a list of 1..{MAX_SLOTS}")))?;
        let max_weight_g = v.get("max_weight_g").and_then(Value::as_u64).filter(|w| (1..=MAX_LOAD_G).contains(w)).ok_or_else(|| bad("max_weight_g must be 1..100000000"))?;
        let mut out = Vec::with_capacity(slots.len());
        for (i, s) in slots.iter().enumerate() {
            out.push(match s {
                Value::Null => None,
                s => {
                    let item = s.get("item").and_then(Value::as_str).filter(|i| specula_sdk::valid_name(i, 64)).ok_or_else(|| bad(&format!("slots[{i}].item must be an item id")))?;
                    let count = s.get("count").and_then(Value::as_u64).filter(|c| (1..=u64::from(MAX_STACK)).contains(c)).ok_or_else(|| bad(&format!("slots[{i}].count must be 1..{MAX_STACK}")))?;
                    Some(Stack { item: item.to_owned(), count: count as u32 })
                }
            });
        }
        Ok(Inventory { slots: out, max_weight_g })
    }

    pub fn to_value(&self, data: &Data) -> Value {
        let slots: Vec<Value> = self.slots.iter().map(|s| s.as_ref().map_or(Value::Null, |s| Value::map().with("item", s.item.as_str()).with("count", s.count))).collect();
        Value::map().with("format", FORMAT).with("slots", slots).with("max_weight_g", self.max_weight_g).with("weight_g", self.weight_g(data))
    }

    /// Total weight; items the data no longer defines weigh nothing.
    pub fn weight_g(&self, data: &Data) -> u64 {
        self.slots.iter().flatten().map(|s| data.items.get(&s.item).map_or(0, |d| d.weight_g) * u64::from(s.count)).sum()
    }

    pub fn count(&self, item: &str) -> u64 {
        self.slots.iter().flatten().filter(|s| s.item == item).map(|s| u64::from(s.count)).sum()
    }

    /// How many more of `item` fit, by free stack room and by weight.
    pub fn room_for(&self, data: &Data, item: &str) -> Result<u64, SdkError> {
        let def = data.def(item)?;
        let stack = def.stack.min(MAX_STACK);
        let by_slots: u64 = self
            .slots
            .iter()
            .map(|s| match s {
                None => u64::from(stack),
                Some(s) if s.item == item => u64::from(stack.saturating_sub(s.count)),
                Some(_) => 0,
            })
            .sum();
        let by_weight = match def.weight_g {
            0 => u64::MAX,
            w => self.max_weight_g.saturating_sub(self.weight_g(data)) / w,
        };
        Ok(by_slots.min(by_weight))
    }

    /// Adds up to `count` of `item`: existing stacks first, then empty slots in order. All or
    /// nothing unless `partial`. Answers how many went in.
    pub fn add(&mut self, data: &Data, item: &str, count: u64, partial: bool) -> Result<u64, SdkError> {
        if count == 0 || count > u64::from(MAX_STACK) * u64::from(MAX_SLOTS) {
            return Err(SdkError::new("invalid_field", format!("'count' must be 1..{}", u64::from(MAX_STACK) * u64::from(MAX_SLOTS))));
        }
        let room = self.room_for(data, item)?;
        if room < count && !partial {
            return Err(no_room(item, count, room));
        }
        let stack = data.def(item)?.stack.min(MAX_STACK);
        let mut left = count.min(room);
        let added = left;
        for s in self.slots.iter_mut().flatten().filter(|s| s.item == item) {
            let put = left.min(u64::from(stack.saturating_sub(s.count)));
            s.count += put as u32;
            left -= put;
        }
        for slot in self.slots.iter_mut().filter(|s| s.is_none()) {
            if left == 0 {
                break;
            }
            let put = left.min(u64::from(stack));
            *slot = Some(Stack { item: item.to_owned(), count: put as u32 });
            left -= put;
        }
        Ok(added)
    }

    /// Removes `count` of `item` (all or nothing), from the last slots first.
    pub fn remove(&mut self, item: &str, count: u64) -> Result<(), SdkError> {
        let have = self.count(item);
        if count == 0 || have < count {
            return Err(SdkError::new("not_enough", format!("has {have} '{}', asked to take {count}", short(item))));
        }
        let mut left = count;
        for slot in self.slots.iter_mut().rev() {
            if left == 0 {
                break;
            }
            if let Some(s) = slot.as_mut().filter(|s| s.item == item) {
                let take = left.min(u64::from(s.count));
                s.count -= take as u32;
                left -= take;
                if s.count == 0 {
                    *slot = None;
                }
            }
        }
        Ok(())
    }

    /// Moves slot `from` onto slot `to`: into an empty slot, merged into the same item (up to its
    /// stack; the rest stays), or swapped with another item.
    pub fn move_slot(&mut self, data: &Data, from: usize, to: usize) -> Result<(), SdkError> {
        let n = self.slots.len();
        if from >= n || to >= n {
            return Err(SdkError::new("invalid_field", format!("slots are 0..{}", n - 1)));
        }
        if from == to || self.slots[from].is_none() {
            return Ok(());
        }
        let same = matches!((&self.slots[from], &self.slots[to]), (Some(a), Some(b)) if a.item == b.item);
        if same {
            let stack = data.items.get(&self.slots[to].as_ref().map(|s| s.item.clone()).unwrap_or_default()).map_or(MAX_STACK, |d| d.stack);
            let room = stack.saturating_sub(self.slots[to].as_ref().map_or(0, |s| s.count));
            let moving = self.slots[from].as_ref().map_or(0, |s| s.count).min(room);
            if let Some(t) = self.slots[to].as_mut() {
                t.count += moving;
            }
            if let Some(f) = self.slots[from].as_mut() {
                f.count -= moving;
                if f.count == 0 {
                    self.slots[from] = None;
                }
            }
        } else {
            self.slots.swap(from, to);
        }
        Ok(())
    }

    /// Resizes to `slots` (refused if a stack would be lost) and sets the weight limit.
    pub fn resize(&mut self, slots: u32, max_weight_g: u64) -> Result<(), SdkError> {
        let slots = slots as usize;
        if self.slots.iter().skip(slots).any(Option::is_some) {
            return Err(SdkError::new("not_empty", format!("slots {slots}.. still hold items: move them first")));
        }
        self.slots.resize(slots, None);
        self.max_weight_g = max_weight_g;
        Ok(())
    }
}

fn no_room(item: &str, count: u64, room: u64) -> SdkError {
    let mut e = SdkError::new("no_room", format!("room for {room} more '{}' (slots and weight), asked for {count}", short(item)));
    e.details = Value::map().with("room", room);
    e
}

#[derive(Default)]
pub struct Inventarium;

#[cfg(not(feature = "linked"))]
specula_sdk::export_module!(Inventarium);

const ACTIONS: &str = r#"{"format": "specula.input.v1", "actions": [
  {"name": "interact", "kind": "button", "group": "world",
   "description": "Pick up the nearest thing in reach (entities tagged pickup). Agents: logic_input {\"interact\": true}.",
   "bindings": ["Key.E", "Pad.West"]}
]}"#;

const OWNER_ARGS: &str = r#"{"type": "object", "required": ["owner"], "properties": {
  "owner": {"description": "The entity: its path or id", "oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}}}"#;

impl Module for Inventarium {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo {
            name: "inventarium",
            version: env!("CARGO_PKG_VERSION"),
            description: "Inventories as data: stacks, slots and weight in an entity's props.inventory; give, take, move, pick up.",
            order: 10,
            actions: ACTIONS,
            hooks: vec![
                HookDef {
                    name: "open",
                    doc: "Gives an entity an inventory (or resizes its own): slots 1..64, max_weight_g 1..1e8 (defaults from items.json). Answers the inventory.",
                    args: r#"{"type": "object", "required": ["owner"], "properties": {"owner": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}, "slots": {"type": "integer", "minimum": 1, "maximum": 64}, "max_weight_g": {"type": "integer", "minimum": 1, "maximum": 100000000}}}"#,
                },
                HookDef {
                    name: "give",
                    doc: "Adds count (1..639936) of item to owner's inventory (opened with the defaults if it has none). All or nothing (no_room) unless partial. Answers {added, count, weight_g}.",
                    args: r#"{"type": "object", "required": ["owner", "item", "count"], "properties": {"owner": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}, "item": {"type": "string", "maxLength": 64}, "count": {"type": "integer", "minimum": 1, "maximum": 639936}, "partial": {"type": "boolean"}}}"#,
                },
                HookDef {
                    name: "take",
                    doc: "Removes count of item from owner's inventory, all or nothing (not_enough). Answers {count} left.",
                    args: r#"{"type": "object", "required": ["owner", "item", "count"], "properties": {"owner": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}, "item": {"type": "string", "maxLength": 64}, "count": {"type": "integer", "minimum": 1, "maximum": 639936}}}"#,
                },
                HookDef {
                    name: "move",
                    doc: "Moves slot from onto slot to: into an empty slot, merged with the same item, or swapped.",
                    args: r#"{"type": "object", "required": ["owner", "from", "to"], "properties": {"owner": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}, "from": {"type": "integer", "minimum": 0, "maximum": 63}, "to": {"type": "integer", "minimum": 0, "maximum": 63}}}"#,
                },
                HookDef { name: "inspect", doc: "Reads owner's inventory with item names, weight and room (changes nothing).", args: OWNER_ARGS },
            ],
        }
    }

    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        // Bad data refuses the build: a running one keeps going (MODULES.md, one transaction).
        Data::parse(DATA)?;
        if cx.find(ROOT)?.is_none() {
            cx.spawn(Spawn::new("group").name("inventarium"))?;
        }
        Ok(())
    }

    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        let requests = cx.events().iter().filter(|e| e.name == "inventarium_give" || e.name == "inventarium_take").count();
        let interact = cx.action("interact").pressed;
        if requests == 0 && !interact {
            // Nothing to do: no data parsed, no requests made.
            return Ok(());
        }
        let data = Data::parse(DATA)?;
        let events: Vec<_> = cx.events().iter().filter(|e| e.name == "inventarium_give" || e.name == "inventarium_take").cloned().collect();
        for e in events.iter().take(MAX_REQUESTS_PER_TICK) {
            let give = e.name == "inventarium_give";
            let item = e.data.get("item").and_then(Value::as_str).unwrap_or_default().to_owned();
            let count = e.data.get("count").and_then(Value::as_u64).unwrap_or(0);
            let partial = e.data.get("partial").and_then(Value::as_bool).unwrap_or(false);
            let result = owner(cx, e.data.get("owner")).and_then(|id| if give { give_to(cx, &data, id, &item, count, partial).map(drop) } else { take_from(cx, &data, id, &item, count).map(drop) });
            if let Err(err) = result {
                cx.emit(
                    "inventarium_refused",
                    Value::map().with("owner", e.data.get("owner").cloned().unwrap_or_default()).with("item", short(&item)).with("count", count).with("code", err.code.as_str()).with("from", e.from.as_str()),
                )?;
            }
        }
        if events.len() > MAX_REQUESTS_PER_TICK {
            cx.emit("inventarium_refused", Value::map().with("code", "busy").with("dropped", events.len() - MAX_REQUESTS_PER_TICK))?;
        }
        if interact {
            pick_up(cx, &data)?;
        }
        Ok(())
    }

    fn hook(&self, cx: &mut Cx, name: &str, args: &Value) -> Result<Value, SdkError> {
        let data = Data::parse(DATA)?;
        let id = owner(cx, args.get("owner"))?;
        match name {
            "open" => {
                let slots = match args.get("slots") {
                    None => data.slots,
                    Some(s) => s.as_u64().filter(|s| (1..=u64::from(MAX_SLOTS)).contains(s)).ok_or_else(|| SdkError::new("invalid_field", "'slots' must be 1..64"))? as u32,
                };
                let max = match args.get("max_weight_g") {
                    None => data.max_weight_g,
                    Some(w) => w.as_u64().filter(|w| (1..=MAX_LOAD_G).contains(w)).ok_or_else(|| SdkError::new("invalid_field", "'max_weight_g' must be 1..100000000"))?,
                };
                let mut inv = read(cx, id)?.unwrap_or_else(|| Inventory::new(slots, max));
                inv.resize(slots, max)?;
                write(cx, &data, id, &inv)?;
                Ok(inv.to_value(&data))
            }
            "give" => {
                let item = str_arg(args, "item")?;
                let count = args.get("count").and_then(Value::as_u64).ok_or_else(|| SdkError::new("invalid_field", "'count' must be a whole number"))?;
                let partial = args.get("partial").and_then(Value::as_bool).unwrap_or(false);
                let (added, inv) = give_to(cx, &data, id, item, count, partial)?;
                Ok(Value::map().with("added", added).with("count", inv.count(item)).with("weight_g", inv.weight_g(&data)))
            }
            "take" => {
                let item = str_arg(args, "item")?;
                let count = args.get("count").and_then(Value::as_u64).ok_or_else(|| SdkError::new("invalid_field", "'count' must be a whole number"))?;
                let inv = take_from(cx, &data, id, item, count)?;
                Ok(Value::map().with("count", inv.count(item)))
            }
            "move" => {
                let slot = |k: &str| args.get(k).and_then(Value::as_u64).map(|n| n as usize).ok_or_else(|| SdkError::new("invalid_field", format!("'{k}' must be a slot number")));
                let (from, to) = (slot("from")?, slot("to")?);
                let mut inv = read(cx, id)?.ok_or_else(|| no_inventory(id))?;
                inv.move_slot(&data, from, to)?;
                write(cx, &data, id, &inv)?;
                Ok(inv.to_value(&data))
            }
            "inspect" => {
                let inv = read(cx, id)?.ok_or_else(|| no_inventory(id))?;
                let mut v = inv.to_value(&data);
                let names: Vec<Value> = inv
                    .slots
                    .iter()
                    .map(|s| s.as_ref().and_then(|s| data.items.get(&s.item)).map_or(Value::Null, |d| d.name.to_value()))
                    .collect();
                v.insert("names", names);
                v.insert("free_slots", inv.slots.iter().filter(|s| s.is_none()).count());
                Ok(v)
            }
            other => Err(SdkError::new("unknown_hook", format!("inventarium has no hook '{}' (it has: open, give, take, move, inspect)", short(other)))),
        }
    }

    fn stop(&self, cx: &mut Cx, reason: StopReason) -> Result<(), SdkError> {
        // Inventories belong to their entities and stay with them; only the module's own group goes.
        if reason == StopReason::Unload {
            if let Some(root) = cx.find(ROOT)? {
                cx.delete(root)?;
            }
        }
        Ok(())
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, SdkError> {
    args.get(key).and_then(Value::as_str).filter(|s| s.len() <= 64).ok_or_else(|| SdkError::new("invalid_field", format!("'{key}' must be a string of at most 64 bytes")))
}

fn no_inventory(id: Id) -> SdkError {
    SdkError::new("no_inventory", format!("entity {} has no props.inventory (open one with inventarium/open)", id.0))
}

/// The entity an `owner` argument names: a path or an id that exists.
pub fn owner(cx: &mut Cx, v: Option<&Value>) -> Result<Id, SdkError> {
    match v {
        Some(Value::Str(p)) if p.len() <= 256 => cx.find(p)?.ok_or_else(|| SdkError::new("not_found", format!("no entity at '{}'", short(p)))),
        Some(Value::Int(i)) if *i >= 0 => {
            let id = Id(*i as u64);
            cx.entity(id)?.map(|_| id).ok_or_else(|| SdkError::new("not_found", format!("no entity {i}")))
        }
        _ => Err(SdkError::new("invalid_field", "'owner' must be an entity path or id")),
    }
}

fn read(cx: &mut Cx, id: Id) -> Result<Option<Inventory>, SdkError> {
    let e = cx.entity(id)?.ok_or_else(|| SdkError::new("not_found", format!("no entity {}", id.0)))?;
    match e.prop("inventory") {
        Value::Null => Ok(None),
        v => Inventory::from_value(v).map(Some),
    }
}

fn write(cx: &mut Cx, data: &Data, id: Id, inv: &Inventory) -> Result<(), SdkError> {
    cx.set(id, Patch::new().props(Value::map().with("inventory", inv.to_value(data)))).map(drop)
}

fn give_to(cx: &mut Cx, data: &Data, id: Id, item: &str, count: u64, partial: bool) -> Result<(u64, Inventory), SdkError> {
    let mut inv = read(cx, id)?.unwrap_or_else(|| Inventory::new(data.slots, data.max_weight_g));
    let added = inv.add(data, item, count, partial)?;
    if added > 0 {
        write(cx, data, id, &inv)?;
        cx.emit("inventarium_changed", Value::map().with("owner", id.0).with("item", item).with("delta", added as i64).with("count", inv.count(item)))?;
    }
    Ok((added, inv))
}

fn take_from(cx: &mut Cx, data: &Data, id: Id, item: &str, count: u64) -> Result<Inventory, SdkError> {
    let mut inv = read(cx, id)?.ok_or_else(|| no_inventory(id))?;
    inv.remove(item, count)?;
    write(cx, data, id, &inv)?;
    cx.emit("inventarium_changed", Value::map().with("owner", id.0).with("item", item).with("delta", -(count as i64)).with("count", inv.count(item)))?;
    Ok(inv)
}

fn world_pos(e: &specula_sdk::Entity) -> Vec3 {
    e.data.get("world_position").and_then(Value::as_vec3).unwrap_or_else(|| e.position())
}

/// The player picks up the nearest `pickup` in reach (ties by id).
fn pick_up(cx: &mut Cx, data: &Data) -> Result<(), SdkError> {
    let Some(path) = &data.player else { return Ok(()) };
    let Some(player) = cx.find(path)? else { return Ok(()) };
    let Some(me) = cx.entity(player)? else { return Ok(()) };
    let at = world_pos(&me);
    let mut best: Option<(f32, Id, Value)> = None;
    // From the tag index, within reach: the cap counts only pickups in range (never a walk of
    // the world, and never the first ones by id elsewhere).
    for id in cx.query_near("pickup", at, data.pickup_range_m, MAX_PICKUPS_SCANNED)? {
        let Some(e) = cx.entity(id)? else { continue };
        let d = (world_pos(&e) - at).length();
        if d <= data.pickup_range_m && best.as_ref().is_none_or(|(bd, _, _)| d < *bd) {
            best = Some((d, id, e.prop("pickup").clone()));
        }
    }
    let Some((_, id, pickup)) = best else { return Ok(()) };
    let item = pickup.get("item").and_then(Value::as_str).unwrap_or_default().to_owned();
    let count = pickup.get("count").and_then(Value::as_u64).unwrap_or(1);
    match give_to(cx, data, player, &item, count, true) {
        Ok((0, _)) => cx.emit("inventarium_refused", Value::map().with("owner", player.0).with("item", short(&item)).with("count", count).with("code", "no_room")),
        Ok((added, _)) => {
            if added == count {
                cx.delete(id)?;
            } else {
                cx.set(id, Patch::new().props(Value::map().with("pickup", pickup.clone().with("count", count - added))))?;
            }
            cx.sound("pickup", Some(at), 1.0)?;
            cx.emit("inventarium_picked", Value::map().with("owner", player.0).with("item", item.as_str()).with("count", added).with("pickup", id.0))
        }
        Err(e) => cx.emit("inventarium_refused", Value::map().with("owner", player.0).with("item", short(&item)).with("count", count).with("code", e.code.as_str())),
    }
}

#[cfg(test)]
mod tests;
