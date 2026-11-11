//! Facultas (Latin "ability"): the example abilities module. Cooldowns, costs and effects as
//! data, cast from input actions, hooks or other modules' events.
//!
//! - **Data** (`abilities.json`): resources (a maximum and a regeneration rate), and abilities
//!   with a cooldown, costs, an optional item the caster must carry (read from
//!   `props.inventory`, the inventory module's documented layout), and a list of effects:
//!   `vfx` (a particle effect at the caster, removed when its time is up), `sound` (played by
//!   the engine, presentation only), `damage` (lowers `props.hp` of tagged entities in a radius;
//!   at 0 they are `defeated` and hidden), `status` (a timed flag on the caster) and `emit` (an
//!   event for other modules).
//! - **State lives in the world:** a caster's `props.abilities` holds the abilities it knows,
//!   when each is ready again (a tick number) and its resources as integers in thousandths with
//!   the tick they were measured at. Resources regenerate by arithmetic on the tick, not by a
//!   write every tick, so an idle caster costs nothing and the numbers are exact on every
//!   machine. Timed things are tick numbers too (`props.status.warded = <tick>`).
//! - **Ways in:** the `ability_1..3` actions (for the data's `player`), `facultas/cast`,
//!   `facultas/learn`, `facultas/forget` and `facultas/status` hooks, and `facultas_learn` /
//!   `facultas_request` events from other modules (quest rewards).
//! - **Ways out:** `facultas_cast`, `facultas_refused`, `facultas_hit`, `facultas_defeated`,
//!   `facultas_learned` events, effect entities under `/facultas`, and sounds.

#![deny(unsafe_code)]

use specula_sdk::{Cx, HookDef, Id, Module, ModuleInfo, Patch, SdkError, Spawn, StopReason, Value, Vec3};
use std::collections::BTreeMap;

pub const MAX_ABILITIES: usize = 64;
pub const MAX_RESOURCES: usize = 8;
pub const MAX_EFFECTS: usize = 8;
/// Longest cooldown and effect time, milliseconds.
pub const MAX_MS: u64 = 600_000;
/// Largest resource amount (whole units).
pub const MAX_RESOURCE: u64 = 1_000_000;
/// Targets one damage effect looks at (tagged entities; the nearest first is not needed: all in
/// the radius are hit, in id order).
pub const MAX_TARGETS: u32 = 256;
/// Effect entities alive at once (the oldest is removed first past this).
pub const MAX_FX: usize = 64;
/// Event requests handled per tick.
pub const MAX_REQUESTS_PER_TICK: usize = 64;
pub const FORMAT: i64 = 1;

const ROOT: &str = "/facultas";
const DATA: &str = include_str!("../abilities.json");

fn bad(at: &str, what: &str) -> SdkError {
    SdkError::new("bad_data", format!("abilities.json: {at} {what}"))
}

fn short(s: &str) -> String {
    s.chars().take(64).collect()
}

fn int(v: Option<&Value>, at: &str, lo: u64, hi: u64) -> Result<u64, SdkError> {
    v.and_then(Value::as_u64).filter(|n| (lo..=hi).contains(n)).ok_or_else(|| bad(at, &format!("must be a whole number {lo}..{hi}")))
}

fn text(v: Option<&Value>, at: &str) -> Result<(String, String), SdkError> {
    let v = v.ok_or_else(|| bad(at, "is required: {key, text}"))?;
    let key = v.get("key").and_then(Value::as_str).filter(|k| !k.is_empty() && k.len() <= 96).ok_or_else(|| bad(&format!("{at}.key"), "must be a string-table key"))?;
    let t = v.get("text").and_then(Value::as_str).filter(|t| !t.is_empty() && t.len() <= 512).ok_or_else(|| bad(&format!("{at}.text"), "must be 1..512 bytes"))?;
    Ok((key.to_owned(), t.to_owned()))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub max: u64,
    pub regen_per_s: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Vfx { effect: String, tint: Option<String>, offset: Vec3, ms: u64 },
    Sound { name: String, gain: f32 },
    Damage { tag: String, radius_m: f32, amount: i64 },
    Status { status: String, ms: u64 },
    Emit { event: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ability {
    pub name: (String, String),
    pub action: Option<String>,
    pub cooldown_ms: u64,
    pub cost: BTreeMap<String, u64>,
    pub requires_item: Option<String>,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    pub player: Option<String>,
    pub starting: Vec<String>,
    pub resources: BTreeMap<String, Resource>,
    pub abilities: BTreeMap<String, Ability>,
}

fn effect(v: &Value, at: &str, resources: &BTreeMap<String, Resource>) -> Result<Effect, SdkError> {
    let _ = resources;
    let name = |k: &str| v.get(k).and_then(Value::as_str).filter(|s| specula_sdk::valid_name(s, 64)).map(str::to_owned).ok_or_else(|| bad(&format!("{at}.{k}"), "must be a name ([a-z][a-z0-9_]{0,63})"));
    Ok(match v.get("kind").and_then(Value::as_str) {
        Some("vfx") => {
            let tint = match v.get("tint") {
                None => None,
                Some(t) => Some(t.as_str().filter(|t| t.len() == 7 && t.starts_with('#') && t[1..].bytes().all(|c| c.is_ascii_hexdigit())).ok_or_else(|| bad(&format!("{at}.tint"), "must be \"#rrggbb\""))?.to_owned()),
            };
            let offset = match v.get("offset") {
                None => Vec3::ZERO,
                Some(o) => o.as_vec3().filter(|o| [o.x, o.y, o.z].iter().all(|c| c.abs() <= 100.0)).ok_or_else(|| bad(&format!("{at}.offset"), "must be [x, y, z] within 100 m"))?,
            };
            Effect::Vfx { effect: name("effect")?, tint, offset, ms: int(v.get("ms"), &format!("{at}.ms"), 1, MAX_MS)? }
        }
        Some("sound") => {
            let gain = v.get("gain").and_then(Value::as_f32).unwrap_or(1.0);
            if !(0.0..=4.0).contains(&gain) {
                return Err(bad(&format!("{at}.gain"), "must be 0..4"));
            }
            Effect::Sound { name: name("name")?, gain }
        }
        Some("damage") => {
            let radius_m = v.get("radius_m").and_then(Value::as_f32).filter(|r| (0.1..=100.0).contains(r)).ok_or_else(|| bad(&format!("{at}.radius_m"), "must be 0.1..100"))?;
            let amount = v.get("amount").and_then(Value::as_i64).filter(|a| (-1_000_000..=1_000_000).contains(a) && *a != 0).ok_or_else(|| bad(&format!("{at}.amount"), "must be a non-zero whole number within ±1e6 (negative heals)"))?;
            Effect::Damage { tag: name("tag")?, radius_m, amount }
        }
        Some("status") => Effect::Status { status: name("status")?, ms: int(v.get("ms"), &format!("{at}.ms"), 1, MAX_MS)? },
        Some("emit") => Effect::Emit { event: name("event")? },
        _ => return Err(bad(&format!("{at}.kind"), "must be vfx, sound, damage, status or emit")),
    })
}

impl Data {
    pub fn parse(text_: &str) -> Result<Data, SdkError> {
        let v = Value::parse_json(text_)?;
        if v.get("format").and_then(Value::as_str) != Some("specula.facultas.v1") {
            return Err(bad("format", "must be \"specula.facultas.v1\""));
        }
        let player = v.get("player").and_then(Value::as_str).map(str::to_owned);
        let mut resources = BTreeMap::new();
        let rs = v.get("resources").and_then(Value::as_map).ok_or_else(|| bad("resources", "must be an object"))?;
        if rs.len() > MAX_RESOURCES {
            return Err(bad("resources", &format!("at most {MAX_RESOURCES}")));
        }
        for (id, r) in rs {
            if !specula_sdk::valid_name(id, 32) {
                return Err(bad(&format!("resources.{id}"), "is not a name"));
            }
            text(r.get("name"), &format!("resources.{id}.name"))?;
            resources.insert(
                id.clone(),
                Resource { max: int(r.get("max"), &format!("resources.{id}.max"), 1, MAX_RESOURCE)?, regen_per_s: int(r.get("regen_per_s"), &format!("resources.{id}.regen_per_s"), 0, MAX_RESOURCE)? },
            );
        }
        let list = v.get("abilities").and_then(Value::as_map).ok_or_else(|| bad("abilities", "must be an object"))?;
        if list.is_empty() || list.len() > MAX_ABILITIES {
            return Err(bad("abilities", &format!("must define 1..{MAX_ABILITIES}")));
        }
        let mut abilities = BTreeMap::new();
        let mut actions = BTreeMap::new();
        for (id, a) in list {
            let at = format!("abilities.{id}");
            if !specula_sdk::valid_name(id, 64) {
                return Err(bad(&at, "is not a name"));
            }
            let action = match a.get("action") {
                None => None,
                Some(x) => {
                    let x = x.as_str().filter(|x| matches!(*x, "ability_1" | "ability_2" | "ability_3")).ok_or_else(|| bad(&format!("{at}.action"), "must be ability_1, ability_2 or ability_3"))?;
                    if let Some(other) = actions.insert(x.to_owned(), id.clone()) {
                        return Err(bad(&format!("{at}.action"), &format!("is also {other}'s")));
                    }
                    Some(x.to_owned())
                }
            };
            let mut cost = BTreeMap::new();
            if let Some(c) = a.get("cost") {
                for (r, n) in c.as_map().ok_or_else(|| bad(&format!("{at}.cost"), "must be an object"))? {
                    let res = resources.get(r).ok_or_else(|| bad(&format!("{at}.cost.{r}"), "names no resource"))?;
                    cost.insert(r.clone(), int(Some(n), &format!("{at}.cost.{r}"), 0, res.max)?);
                }
            }
            let fx = a.get("effects").and_then(Value::as_list).filter(|e| !e.is_empty() && e.len() <= MAX_EFFECTS).ok_or_else(|| bad(&format!("{at}.effects"), &format!("must be a list of 1..{MAX_EFFECTS}")))?;
            let effects = fx.iter().enumerate().map(|(i, e)| effect(e, &format!("{at}.effects[{i}]"), &resources)).collect::<Result<_, _>>()?;
            text(a.get("summary"), &format!("{at}.summary"))?;
            abilities.insert(
                id.clone(),
                Ability {
                    name: text(a.get("name"), &format!("{at}.name"))?,
                    action,
                    cooldown_ms: int(a.get("cooldown_ms"), &format!("{at}.cooldown_ms"), 0, MAX_MS)?,
                    cost,
                    requires_item: a.get("requires_item").and_then(Value::as_str).map(str::to_owned),
                    effects,
                },
            );
        }
        let starting = match v.get("starting") {
            None => Vec::new(),
            Some(s) => s
                .as_list()
                .ok_or_else(|| bad("starting", "must be a list of ability ids"))?
                .iter()
                .map(|x| x.as_str().filter(|x| abilities.contains_key(*x)).map(str::to_owned).ok_or_else(|| bad("starting", "names an ability that isn't defined")))
                .collect::<Result<_, _>>()?,
        };
        Ok(Data { player, starting, resources, abilities })
    }

    fn ability(&self, id: &str) -> Result<&Ability, SdkError> {
        self.abilities.get(id).ok_or_else(|| {
            let known: Vec<&str> = self.abilities.keys().map(String::as_str).take(24).collect();
            SdkError::new("unknown_ability", format!("no ability '{}' (abilities: {})", short(id), known.join(", ")))
        })
    }
}

/// Ticks for `ms` milliseconds at `hz`, rounded up (integers only).
pub fn ticks_for(ms: u64, hz: u32) -> u64 {
    (ms * u64::from(hz)).div_ceil(1000)
}

/// One resource's level: thousandths of a unit, measured at a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level {
    pub milli: u64,
    pub at: u64,
}

impl Level {
    /// The level at `now`: regenerated by exact integer arithmetic, capped at the maximum.
    pub fn now(self, r: &Resource, now: u64, hz: u32) -> u64 {
        let elapsed = now.saturating_sub(self.at);
        let gained = (r.regen_per_s.saturating_mul(1000)).saturating_mul(elapsed) / u64::from(hz.max(1));
        self.milli.saturating_add(gained).min(r.max * 1000)
    }
}

/// What a caster's `props.abilities` holds.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Caster {
    pub known: Vec<String>,
    /// Tick each ability is ready again.
    pub ready_at: BTreeMap<String, u64>,
    pub levels: BTreeMap<String, Level>,
}

impl Caster {
    /// A new caster: the starting abilities, every resource full.
    pub fn new(data: &Data, now: u64) -> Caster {
        Caster {
            known: data.starting.clone(),
            ready_at: BTreeMap::new(),
            levels: data.resources.iter().map(|(k, r)| (k.clone(), Level { milli: r.max * 1000, at: now })).collect(),
        }
    }

    pub fn from_value(v: &Value, data: &Data, now: u64) -> Result<Caster, SdkError> {
        let bad = |what: &str| SdkError::new("bad_caster", format!("props.abilities: {what}"));
        if v.get("format").and_then(Value::as_i64) != Some(FORMAT) {
            return Err(bad(&format!("format must be {FORMAT}")));
        }
        let known = v.get("known").and_then(Value::as_list).filter(|k| k.len() <= MAX_ABILITIES).ok_or_else(|| bad("known must be a list"))?;
        let known = known.iter().map(|k| k.as_str().filter(|k| specula_sdk::valid_name(k, 64)).map(str::to_owned).ok_or_else(|| bad("known holds a bad id"))).collect::<Result<Vec<_>, _>>()?;
        let mut ready_at = BTreeMap::new();
        for (k, t) in v.get("ready_at").and_then(Value::as_map).into_iter().flatten().take(MAX_ABILITIES) {
            ready_at.insert(k.clone(), t.as_u64().ok_or_else(|| bad("ready_at values must be ticks"))?);
        }
        let mut levels = BTreeMap::new();
        // Resources the data defines: missing ones start full (data grew), extra ones are dropped.
        let saved = v.get("resources");
        for (k, r) in &data.resources {
            let lv = match saved.and_then(|s| s.get(k)) {
                Some(l) => Level {
                    milli: l.get("milli").and_then(Value::as_u64).ok_or_else(|| bad("resources.*.milli must be a whole number"))?.min(r.max * 1000),
                    at: l.get("at").and_then(Value::as_u64).ok_or_else(|| bad("resources.*.at must be a tick"))?,
                },
                None => Level { milli: r.max * 1000, at: now },
            };
            levels.insert(k.clone(), lv);
        }
        Ok(Caster { known, ready_at, levels })
    }

    pub fn to_value(&self) -> Value {
        let mut ready = Value::map();
        for (k, t) in &self.ready_at {
            ready.insert(k, *t);
        }
        let mut res = Value::map();
        for (k, l) in &self.levels {
            res.insert(k, Value::map().with("milli", l.milli).with("at", l.at));
        }
        let known: Vec<Value> = self.known.iter().map(|k| Value::from(k.as_str())).collect();
        Value::map().with("format", FORMAT).with("known", known).with("ready_at", ready).with("resources", res)
    }

    /// Why `ability` can't be cast now (`None`: it can), with the facts.
    pub fn check(&self, data: &Data, id: &str, now: u64, hz: u32, items: &dyn Fn(&str) -> u64) -> Result<(), SdkError> {
        let a = data.ability(id)?;
        if !self.known.iter().any(|k| k == id) {
            return Err(SdkError::new("not_known", format!("the caster doesn't know '{id}' (learn it with facultas/learn)")));
        }
        if let Some(&ready) = self.ready_at.get(id).filter(|r| **r > now) {
            let mut e = SdkError::new("cooldown", format!("'{id}' is ready in {} ticks", ready - now));
            e.details = Value::map().with("ready_at", ready).with("ticks_left", ready - now);
            return Err(e);
        }
        if let Some(item) = &a.requires_item {
            if items(item) == 0 {
                return Err(SdkError::new("missing_item", format!("'{id}' needs a '{item}' in the caster's inventory")));
            }
        }
        for (r, cost) in &a.cost {
            let have = self.levels.get(r).map_or(0, |l| l.now(&data.resources[r], now, hz));
            if have < cost * 1000 {
                let mut e = SdkError::new("no_resource", format!("'{id}' costs {cost} {r}; the caster has {}.{:03}", have / 1000, have % 1000));
                e.details = Value::map().with("resource", r.as_str()).with("have_milli", have).with("cost", *cost);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Pays for and starts the cooldown of `id` (after [`Caster::check`]).
    pub fn pay(&mut self, data: &Data, id: &str, now: u64, hz: u32) {
        let a = &data.abilities[id];
        for (r, cost) in &a.cost {
            let res = &data.resources[r];
            let have = self.levels.get(r).map_or(res.max * 1000, |l| l.now(res, now, hz));
            self.levels.insert(r.clone(), Level { milli: have.saturating_sub(cost * 1000), at: now });
        }
        self.ready_at.insert(id.to_owned(), now + ticks_for(a.cooldown_ms, hz));
    }
}

#[derive(Default)]
pub struct Facultas;

#[cfg(not(feature = "linked"))]
specula_sdk::export_module!(Facultas);

const ACTIONS: &str = r#"{"format": "specula.input.v1", "actions": [
  {"name": "ability_1", "kind": "button", "group": "abilities", "description": "First ability (abilities.json picks which). Agents: logic_input {\"ability_1\": true}.", "bindings": ["Key.Q", "Pad.RightBumper"]},
  {"name": "ability_2", "kind": "button", "group": "abilities", "description": "Second ability.", "bindings": ["Key.R", "Pad.LeftBumper"]},
  {"name": "ability_3", "kind": "button", "group": "abilities", "description": "Third ability.", "bindings": ["Key.F", "Pad.North"]}
]}"#;

const CASTER_ABILITY: &str = r#"{"type": "object", "required": ["caster", "ability"], "properties": {"caster": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}, "ability": {"type": "string", "maxLength": 64}}}"#;

impl Module for Facultas {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo {
            name: "facultas",
            version: env!("CARGO_PKG_VERSION"),
            description: "Abilities as data: cooldowns, resource costs, required items, and effects (vfx, sound, damage, status, events).",
            order: 20,
            actions: ACTIONS,
            hooks: vec![
                HookDef { name: "cast", doc: "Casts ability for caster (an entity path or id) now, or refuses with why (not_known, cooldown, missing_item, no_resource). Answers {ability, ready_at, hits}.", args: CASTER_ABILITY },
                HookDef { name: "learn", doc: "Teaches caster an ability (the caster gets props.abilities if it had none).", args: CASTER_ABILITY },
                HookDef { name: "forget", doc: "Takes an ability from caster.", args: CASTER_ABILITY },
                HookDef {
                    name: "status",
                    doc: "Reads caster's abilities: known, ticks until each is ready, resources now, timed statuses (changes nothing).",
                    args: r#"{"type": "object", "required": ["caster"], "properties": {"caster": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}}}"#,
                },
            ],
        }
    }

    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        Data::parse(DATA)?;
        if cx.find(ROOT)?.is_none() {
            cx.spawn(Spawn::new("group").name("facultas"))?;
        }
        Ok(())
    }

    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        expire_fx(cx)?;
        let wanted = |n: &str| n == "facultas_learn" || n == "facultas_request";
        let requests: Vec<_> = cx.events().iter().filter(|e| wanted(&e.name)).take(MAX_REQUESTS_PER_TICK).cloned().collect();
        let pressed: Vec<String> = ["ability_1", "ability_2", "ability_3"].iter().filter(|a| cx.action(a).pressed).map(|a| (*a).to_owned()).collect();
        if requests.is_empty() && pressed.is_empty() {
            return Ok(());
        }
        let data = Data::parse(DATA)?;
        // Each cast is checked (costs, cooldowns, the call's sound budget) before it is paid for,
        // so a refusal changes nothing. What runs after paying can only fail unexpectedly, and
        // then the whole call is rolled back by the host (`?`), payments included.
        let mut budget = Budget::default();
        for e in &requests {
            let ability = e.data.get("ability").and_then(Value::as_str).unwrap_or_default().to_owned();
            let result = caster_id(cx, e.data.get("caster")).and_then(|c| if e.name == "facultas_learn" { learn(cx, &data, c, &ability, true).map(|_| None) } else { plan(cx, &data, c, &ability, &mut budget).map(Some) });
            match result {
                Ok(Some(p)) => drop(apply(cx, &data, p)?),
                Ok(None) => {}
                Err(err) => cx.emit("facultas_refused", Value::map().with("caster", e.data.get("caster").cloned().unwrap_or_default()).with("ability", short(&ability)).with("code", err.code.as_str()).with("from", e.from.as_str()))?,
            }
        }
        let player = match data.player.as_deref() {
            Some(path) if !pressed.is_empty() => cx.find(path)?,
            _ => None,
        };
        if let Some(me) = player {
            for action in pressed {
                let Some((id, _)) = data.abilities.iter().find(|(_, a)| a.action.as_deref() == Some(action.as_str())) else { continue };
                let id = id.clone();
                match plan(cx, &data, me, &id, &mut budget) {
                    Ok(p) => drop(apply(cx, &data, p)?),
                    Err(err) => {
                        let mut ev = Value::map().with("caster", me.0).with("ability", id.as_str()).with("code", err.code.as_str());
                        if let Some(t) = err.details.get("ticks_left") {
                            ev.insert("ticks_left", t.clone());
                        }
                        cx.emit("facultas_refused", ev)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn hook(&self, cx: &mut Cx, name: &str, args: &Value) -> Result<Value, SdkError> {
        let data = Data::parse(DATA)?;
        let caster = caster_id(cx, args.get("caster"))?;
        let ability = || args.get("ability").and_then(Value::as_str).filter(|a| a.len() <= 64).ok_or_else(|| SdkError::new("invalid_field", "'ability' must be an ability id"));
        match name {
            "cast" => {
                let p = plan(cx, &data, caster, ability()?, &mut Budget::default())?;
                apply(cx, &data, p)
            }
            "learn" => learn(cx, &data, caster, ability()?, true),
            "forget" => learn(cx, &data, caster, ability()?, false),
            "status" => {
                let now = cx.tick();
                let hz = cx.tick_hz();
                let me = read(cx, &data, caster, now)?;
                let e = cx.entity(caster)?.ok_or_else(|| SdkError::new("not_found", "the caster vanished"))?;
                let mut ready = Value::map();
                for k in &me.known {
                    ready.insert(k, me.ready_at.get(k).map_or(0, |r| r.saturating_sub(now)));
                }
                let mut res = Value::map();
                for (k, l) in &me.levels {
                    res.insert(k, Value::map().with("now", l.now(&data.resources[k], now, hz) as f64 / 1000.0).with("max", data.resources[k].max));
                }
                let mut statuses = Value::map();
                for (k, until) in e.prop("status").as_map().into_iter().flatten() {
                    let left = until.as_u64().unwrap_or(0).saturating_sub(now);
                    if left > 0 {
                        statuses.insert(k, left);
                    }
                }
                let known: Vec<Value> = me.known.iter().map(|k| Value::from(k.as_str())).collect();
                Ok(Value::map().with("known", known).with("ticks_until_ready", ready).with("resources", res).with("status_ticks_left", statuses))
            }
            other => Err(SdkError::new("unknown_hook", format!("facultas has no hook '{}' (it has: cast, learn, forget, status)", short(other)))),
        }
    }

    fn stop(&self, cx: &mut Cx, reason: StopReason) -> Result<(), SdkError> {
        if reason == StopReason::Unload {
            if let Some(root) = cx.find(ROOT)? {
                cx.delete(root)?;
            }
            cx.set_state("fx", Value::Null)?;
        }
        Ok(())
    }
}

/// The entity a `caster` argument names.
fn caster_id(cx: &mut Cx, v: Option<&Value>) -> Result<Id, SdkError> {
    match v {
        Some(Value::Str(p)) if p.len() <= 256 => cx.find(p)?.ok_or_else(|| SdkError::new("not_found", format!("no entity at '{}'", short(p)))),
        Some(Value::Int(i)) if *i >= 0 => {
            let id = Id(*i as u64);
            cx.entity(id)?.map(|_| id).ok_or_else(|| SdkError::new("not_found", format!("no entity {i}")))
        }
        _ => Err(SdkError::new("invalid_field", "'caster' must be an entity path or id")),
    }
}

fn read(cx: &mut Cx, data: &Data, id: Id, now: u64) -> Result<Caster, SdkError> {
    let e = cx.entity(id)?.ok_or_else(|| SdkError::new("not_found", format!("no entity {}", id.0)))?;
    match e.prop("abilities") {
        Value::Null => Ok(Caster::new(data, now)),
        v => Caster::from_value(v, data, now),
    }
}

fn write(cx: &mut Cx, id: Id, c: &Caster) -> Result<(), SdkError> {
    cx.set(id, Patch::new().props(Value::map().with("abilities", c.to_value()))).map(drop)
}

/// How many of `item` the entity carries, from `props.inventory` (the inventory module's layout:
/// `slots: [{item, count} | null]`).
fn carried(e: &specula_sdk::Entity, item: &str) -> u64 {
    e.prop("inventory").get("slots").and_then(Value::as_list).unwrap_or_default().iter().filter(|s| s.get("item").and_then(Value::as_str) == Some(item)).filter_map(|s| s.get("count").and_then(Value::as_u64)).sum()
}

fn world_pos(e: &specula_sdk::Entity) -> Vec3 {
    e.data.get("world_position").and_then(Value::as_vec3).unwrap_or_else(|| e.position())
}

fn learn(cx: &mut Cx, data: &Data, caster: Id, ability: &str, learn: bool) -> Result<Value, SdkError> {
    data.ability(ability)?;
    let now = cx.tick();
    let mut c = read(cx, data, caster, now)?;
    let had = c.known.iter().any(|k| k == ability);
    if learn && !had {
        if c.known.len() >= MAX_ABILITIES {
            return Err(SdkError::new("limit", format!("a caster knows at most {MAX_ABILITIES} abilities")));
        }
        c.known.push(ability.to_owned());
        cx.emit("facultas_learned", Value::map().with("caster", caster.0).with("ability", ability))?;
    } else if !learn {
        c.known.retain(|k| k != ability);
        c.ready_at.remove(ability);
    }
    write(cx, caster, &c)?;
    let known: Vec<Value> = c.known.iter().map(|k| Value::from(k.as_str())).collect();
    Ok(Value::map().with("known", known))
}

/// What one call has promised so far: its sounds count against the SDK's per-call budget.
#[derive(Default)]
pub struct Budget {
    sounds: usize,
}

/// A cast that passed every check, paid for in `caster` (not yet written).
pub struct Plan {
    caster: Id,
    id: String,
    c: Caster,
    at: Vec3,
}

/// Checks a cast of `id` by `caster` (refusals change nothing): known, ready, affordable, and
/// within the call's sound budget, counting the casts before it in the same call.
fn plan(cx: &mut Cx, data: &Data, caster: Id, id: &str, budget: &mut Budget) -> Result<Plan, SdkError> {
    let now = cx.tick();
    let hz = cx.tick_hz();
    let mut c = read(cx, data, caster, now)?;
    let me = cx.entity(caster)?.ok_or_else(|| SdkError::new("not_found", "the caster vanished"))?;
    c.check(data, id, now, hz, &|item| carried(&me, item))?;
    let sounds = data.abilities[id].effects.iter().filter(|fx| matches!(fx, Effect::Sound { .. })).count();
    if budget.sounds + sounds > specula_sdk::MAX_SOUNDS_PER_CALL {
        return Err(SdkError::new("too_many_sounds", format!("this call already asked for {} sounds (at most {} a call): cast again next tick", budget.sounds, specula_sdk::MAX_SOUNDS_PER_CALL)));
    }
    budget.sounds += sounds;
    c.pay(data, id, now, hz);
    Ok(Plan { caster, id: id.to_owned(), c, at: world_pos(&me) })
}

/// Pays for a planned cast and runs its effects in order. Any error here is unexpected (the plan
/// checked what can be refused): it goes up, and the host rolls the call back.
fn apply(cx: &mut Cx, data: &Data, p: Plan) -> Result<Value, SdkError> {
    let Plan { caster, id, c, at } = p;
    let id = id.as_str();
    let (now, hz) = (cx.tick(), cx.tick_hz());
    write(cx, caster, &c)?;
    let mut hits = 0u64;
    for fx in &data.abilities[id].effects {
        match fx {
            Effect::Vfx { effect, tint, offset, ms } => spawn_fx(cx, effect, tint.as_deref(), at + *offset, now + ticks_for(*ms, hz))?,
            Effect::Sound { name, gain } => cx.sound(name, Some(at), *gain)?,
            Effect::Damage { tag, radius_m, amount } => hits += damage(cx, caster, id, tag, at, *radius_m, *amount)?,
            Effect::Status { status, ms } => {
                // `props` merge one level deep: keep the caster's other statuses (expired ones go).
                let mut all = Value::map();
                if let Some(e) = cx.entity(caster)? {
                    for (k, until) in e.prop("status").as_map().into_iter().flatten().take(MAX_ABILITIES) {
                        if until.as_u64().is_some_and(|u| u > now) {
                            all.insert(k, until.clone());
                        }
                    }
                }
                all.insert(status, now + ticks_for(*ms, hz));
                cx.set(caster, Patch::new().props(Value::map().with("status", all)))?;
            }
            Effect::Emit { event } => cx.emit(event, Value::map().with("caster", caster.0).with("ability", id))?,
        }
    }
    let ready = c.ready_at[id];
    cx.emit("facultas_cast", Value::map().with("caster", caster.0).with("ability", id).with("hits", hits))?;
    Ok(Value::map().with("ability", id).with("ready_at", ready).with("hits", hits))
}

/// Hits every entity tagged `tag` within `radius` of `at` with `props.hp` above 0 (id order).
fn damage(cx: &mut Cx, caster: Id, ability: &str, tag: &str, at: Vec3, radius: f32, amount: i64) -> Result<u64, SdkError> {
    let mut hits = 0;
    // From the tag index, within reach: the cap counts only targets in range.
    for target in cx.query_near(tag, at, radius, MAX_TARGETS)? {
        let Some(e) = cx.entity(target)? else { continue };
        let Some(hp) = e.prop("hp").as_i64().filter(|hp| *hp > 0) else { continue };
        if (world_pos(&e) - at).length() > radius {
            continue;
        }
        let left = (hp - amount).clamp(0, 1_000_000_000);
        let mut patch = Patch::new().props(Value::map().with("hp", left));
        if left == 0 {
            // Untagged (tagged `defeated` instead): a defeated target no longer counts toward
            // anyone's target cap.
            patch = patch.props(Value::map().with("hp", 0).with("defeated", true)).field("visible", false).field("remove_tags", vec![Value::from(tag)]).field("add_tags", vec![Value::from("defeated")]);
        }
        cx.set(target, patch)?;
        hits += 1;
        cx.emit("facultas_hit", Value::map().with("caster", caster.0).with("target", target.0).with("ability", ability).with("hp", left))?;
        if left == 0 {
            cx.emit("facultas_defeated", Value::map().with("caster", caster.0).with("target", target.0).with("ability", ability).with("tag", tag))?;
        }
    }
    Ok(hits)
}

/// A timed effect entity under `/facultas`, removed when `until` passes ([`expire_fx`]).
fn spawn_fx(cx: &mut Cx, effect: &str, tint: Option<&str>, at: Vec3, until: u64) -> Result<(), SdkError> {
    let root = match cx.find(ROOT)? {
        Some(r) => r,
        None => cx.spawn(Spawn::new("group").name("facultas"))?,
    };
    let mut fx: Vec<Value> = cx.state("fx")?.as_list().unwrap_or_default().to_vec();
    // Too many alive: the oldest goes first.
    while fx.len() >= MAX_FX {
        if let Some(id) = fx.first().and_then(|f| f.as_list()).and_then(|f| f.first()).and_then(Value::as_u64) {
            cx.delete(Id(id))?;
        }
        fx.remove(0);
    }
    let mut vfx = Value::map().with("effect", effect);
    if let Some(t) = tint {
        vfx.insert("params", Value::map().with("tint", t));
    }
    // Unnamed: two casts of the same effect in one tick must not collide on a name.
    let id = cx.spawn(Spawn::new("group").field("parent", root.0).position(at).field("vfx", vfx).tags(&["facultas_fx"]))?;
    fx.push(Value::from(vec![Value::from(id.0), Value::from(until)]));
    cx.set_state("fx", fx)
}

/// Removes effect entities whose time is up (one state read a tick; nothing else when none are due).
fn expire_fx(cx: &mut Cx) -> Result<(), SdkError> {
    let fx = cx.state("fx")?;
    let Some(list) = fx.as_list().filter(|l| !l.is_empty()) else { return Ok(()) };
    let now = cx.tick();
    let due = |f: &Value| f.as_list().and_then(|f| f.get(1)).and_then(Value::as_u64).is_some_and(|u| u <= now);
    if !list.iter().any(due) {
        return Ok(());
    }
    let mut keep = Vec::new();
    for f in list {
        if due(f) {
            if let Some(id) = f.as_list().and_then(|f| f.first()).and_then(Value::as_u64) {
                cx.delete(Id(id))?;
            }
        } else {
            keep.push(f.clone());
        }
    }
    cx.set_state("fx", if keep.is_empty() { Value::Null } else { Value::from(keep) })
}

#[cfg(test)]
mod tests;
