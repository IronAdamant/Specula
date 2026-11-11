//! Colloquium (Latin "conversation"): the example dialogue module. Dialogue graphs as data,
//! with conditions, choices, effects and localisable lines.
//!
//! - **Data** (`dialogue.json`): speakers, and graphs of nodes. A node has a speaker, a line,
//!   effects run when it is entered (`do`), and either up to four choices or a `next` node
//!   (`null` ends the talk). A choice has a line, a condition (`if`), effects and a `next`.
//!   Conditions read the world: `has_item [item, n]` (the inventory module's
//!   `props.inventory`), `quest [id, none|active|done]` (the quest module's `props.quests`),
//!   `flag name` (this module's flags), and `all`, `any`, `not`. Effects ask the other modules
//!   by event (`give`, `take` → inventarium; `quest_start`, `signal` → quaestio; `learn` →
//!   facultas), set a flag, or emit any event. Every reference is checked when the data loads.
//! - **Text:** lines are `{key, text}`: a string-table key and the English fallback. The
//!   simulation stores keys, never translated text, so a run is the same in every language; the
//!   lines show on world `text` entities that carry the key (`props.line`) beside the English
//!   text (docs/design/GAMEPLAY_MODULES.md "Text": the localisation lane's tables resolve them).
//! - **State lives in the world:** a talk is the actor's `props.colloquium` (`{format, graph,
//!   node, with, choices, flags}`), so it saves, reloads and replays with the world.
//! - **Ways in:** the `talk` action (start with the nearest entity tagged `talkable` whose
//!   `props.dialogue` names a graph, or go on), `choice_1..4`, hooks `start`, `choose`, `next`,
//!   `end`, `state`; a `colloquium_start` event.
//! - **Ways out:** `colloquium_node`, `colloquium_choice`, `colloquium_ended` events, the
//!   effects' events, the text entities under `/colloquium`, and a soft sound per line.
//!
//! Text input never comes from here: modules see actions, never keys (the window's keyboard
//! types only into what its operator points it at; docs/design/TEXT.md).

#![deny(unsafe_code)]

use specula_sdk::{Cx, HookDef, Id, Module, ModuleInfo, Patch, SdkError, Spawn, StopReason, Value, Vec3};
use std::collections::BTreeMap;

pub const MAX_GRAPHS: usize = 64;
pub const MAX_NODES: usize = 256;
pub const MAX_CHOICES: usize = 4;
pub const MAX_EFFECTS: usize = 8;
/// Condition nodes in one `if` (all, any, not and leaves), and how deep they nest.
pub const MAX_CONDITION_NODES: usize = 32;
pub const MAX_CONDITION_DEPTH: usize = 6;
/// Flags one actor may hold.
pub const MAX_FLAGS: usize = 256;
/// Actors in a talk at once (the per-tick distance check reads each).
pub const MAX_TALKS: usize = 16;
pub const FORMAT: i64 = 1;

const ROOT: &str = "/colloquium";
const DATA: &str = include_str!("../dialogue.json");

fn bad(at: &str, what: &str) -> SdkError {
    SdkError::new("bad_data", format!("dialogue.json: {at} {what}"))
}

fn short(s: &str) -> String {
    s.chars().take(64).collect()
}

/// A line: string-table key and English fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub key: String,
    pub text: String,
}

impl Line {
    fn parse(v: Option<&Value>, at: &str) -> Result<Line, SdkError> {
        let v = v.ok_or_else(|| bad(at, "is required: {key, text}"))?;
        let key = v
            .get("key")
            .and_then(Value::as_str)
            .filter(|k| !k.is_empty() && k.len() <= 96 && k.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'.'))
            .ok_or_else(|| bad(&format!("{at}.key"), "must be a string-table key ([a-z0-9_.]{1,96})"))?;
        let text = v.get("text").and_then(Value::as_str).filter(|t| !t.is_empty() && t.len() <= 1024).ok_or_else(|| bad(&format!("{at}.text"), "must be 1..1024 bytes of English"))?;
        Ok(Line { key: key.to_owned(), text: text.to_owned() })
    }

    pub fn to_value(&self) -> Value {
        Value::map().with("key", self.key.as_str()).with("text", self.text.as_str())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    HasItem(String, u64),
    Quest(String, String),
    Flag(String),
    All(Vec<Cond>),
    Any(Vec<Cond>),
    Not(Box<Cond>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Give(String, u64),
    Take(String, u64),
    QuestStart(String),
    Signal(String),
    Learn(String),
    Flag(String),
    Emit(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub line: Line,
    pub cond: Option<Cond>,
    pub effects: Vec<Effect>,
    pub next: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub speaker: String,
    pub line: Line,
    pub effects: Vec<Effect>,
    pub choices: Vec<Choice>,
    pub next: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    pub start: String,
    pub nodes: BTreeMap<String, Node>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    pub player: Option<String>,
    pub talk_range_m: f32,
    pub speakers: BTreeMap<String, Line>,
    pub graphs: BTreeMap<String, Graph>,
}

fn name_of(v: &Value, at: &str) -> Result<String, SdkError> {
    v.as_str().filter(|s| specula_sdk::valid_name(s, 64)).map(str::to_owned).ok_or_else(|| bad(at, "must be a name ([a-z][a-z0-9_]{0,63})"))
}

fn pair(v: &Value, at: &str) -> Result<(String, u64), SdkError> {
    match v.as_list() {
        Some([n, c]) => Ok((name_of(n, &format!("{at}[0]"))?, c.as_u64().filter(|c| (1..=1_000_000).contains(c)).ok_or_else(|| bad(&format!("{at}[1]"), "must be 1..1000000"))?)),
        _ => Err(bad(at, "must be [name, count]")),
    }
}

fn cond(v: &Value, at: &str, depth: usize, budget: &mut usize) -> Result<Cond, SdkError> {
    if depth > MAX_CONDITION_DEPTH {
        return Err(bad(at, &format!("nests deeper than {MAX_CONDITION_DEPTH}")));
    }
    if *budget == 0 {
        return Err(bad(at, &format!("has more than {MAX_CONDITION_NODES} parts")));
    }
    *budget -= 1;
    let m = v.as_map().filter(|m| m.len() == 1).ok_or_else(|| bad(at, "must be one of {has_item}, {quest}, {flag}, {all}, {any}, {not}"))?;
    let (k, x) = m.iter().next().expect("one entry");
    let at = format!("{at}.{k}");
    Ok(match k.as_str() {
        "has_item" => {
            let (i, n) = pair(x, &at)?;
            Cond::HasItem(i, n)
        }
        "quest" => match x.as_list() {
            Some([q, s]) => {
                let s = s.as_str().filter(|s| matches!(*s, "none" | "active" | "done")).ok_or_else(|| bad(&format!("{at}[1]"), "must be none, active or done"))?;
                Cond::Quest(name_of(q, &format!("{at}[0]"))?, s.to_owned())
            }
            _ => return Err(bad(&at, "must be [quest, none|active|done]")),
        },
        "flag" => Cond::Flag(name_of(x, &at)?),
        "all" | "any" => {
            let list = x.as_list().filter(|l| !l.is_empty()).ok_or_else(|| bad(&at, "must be a non-empty list"))?;
            let parts = list.iter().enumerate().map(|(i, c)| cond(c, &format!("{at}[{i}]"), depth + 1, budget)).collect::<Result<_, _>>()?;
            if k == "all" {
                Cond::All(parts)
            } else {
                Cond::Any(parts)
            }
        }
        "not" => Cond::Not(Box::new(cond(x, &at, depth + 1, budget)?)),
        _ => return Err(bad(&at, "is not a condition (has_item, quest, flag, all, any, not)")),
    })
}

fn effects(v: Option<&Value>, at: &str) -> Result<Vec<Effect>, SdkError> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let list = v.as_list().filter(|l| l.len() <= MAX_EFFECTS).ok_or_else(|| bad(at, &format!("must be a list of at most {MAX_EFFECTS} effects")))?;
    list.iter()
        .enumerate()
        .map(|(i, e)| {
            let at = format!("{at}[{i}]");
            let m = e.as_map().filter(|m| m.len() == 1).ok_or_else(|| bad(&at, "must be one of {give}, {take}, {quest_start}, {signal}, {learn}, {flag}, {emit}"))?;
            let (k, x) = m.iter().next().expect("one entry");
            let at = format!("{at}.{k}");
            Ok(match k.as_str() {
                "give" => pair(x, &at).map(|(i, n)| Effect::Give(i, n))?,
                "take" => pair(x, &at).map(|(i, n)| Effect::Take(i, n))?,
                "quest_start" => Effect::QuestStart(name_of(x, &at)?),
                "signal" => Effect::Signal(name_of(x, &at)?),
                "learn" => Effect::Learn(name_of(x, &at)?),
                "flag" => Effect::Flag(name_of(x, &at)?),
                "emit" => Effect::Emit(name_of(x, &at)?),
                _ => return Err(bad(&at, "is not an effect")),
            })
        })
        .collect()
}

fn next(v: Option<&Value>, at: &str) -> Result<Option<String>, SdkError> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(n) => name_of(n, at).map(Some),
    }
}

impl Data {
    pub fn parse(text: &str) -> Result<Data, SdkError> {
        let v = Value::parse_json(text)?;
        if v.get("format").and_then(Value::as_str) != Some("specula.colloquium.v1") {
            return Err(bad("format", "must be \"specula.colloquium.v1\""));
        }
        let player = v.get("player").and_then(Value::as_str).map(str::to_owned);
        let talk_range_m = v.get("talk_range_m").and_then(Value::as_f32).unwrap_or(2.5);
        if !(0.1..=50.0).contains(&talk_range_m) {
            return Err(bad("talk_range_m", "must be 0.1..50"));
        }
        let mut speakers = BTreeMap::new();
        for (id, s) in v.get("speakers").and_then(Value::as_map).ok_or_else(|| bad("speakers", "must be an object"))? {
            if !specula_sdk::valid_name(id, 64) || speakers.len() >= MAX_GRAPHS {
                return Err(bad(&format!("speakers.{id}"), "is not a name (or there are too many speakers)"));
            }
            speakers.insert(id.clone(), Line::parse(s.get("name"), &format!("speakers.{id}.name"))?);
        }
        let gs = v.get("graphs").and_then(Value::as_map).filter(|g| !g.is_empty() && g.len() <= MAX_GRAPHS).ok_or_else(|| bad("graphs", &format!("must be an object of 1..{MAX_GRAPHS} graphs")))?;
        let mut graphs = BTreeMap::new();
        for (gid, g) in gs {
            let at = format!("graphs.{gid}");
            if !specula_sdk::valid_name(gid, 64) {
                return Err(bad(&at, "is not a name"));
            }
            let ns = g.get("nodes").and_then(Value::as_map).filter(|n| !n.is_empty() && n.len() <= MAX_NODES).ok_or_else(|| bad(&format!("{at}.nodes"), &format!("must be an object of 1..{MAX_NODES} nodes")))?;
            let mut nodes = BTreeMap::new();
            for (nid, n) in ns {
                let at = format!("{at}.nodes.{nid}");
                if !specula_sdk::valid_name(nid, 64) {
                    return Err(bad(&at, "is not a name"));
                }
                let speaker = name_of(n.get("speaker").unwrap_or(&Value::Null), &format!("{at}.speaker"))?;
                if !speakers.contains_key(&speaker) {
                    return Err(bad(&format!("{at}.speaker"), "names no speaker"));
                }
                let cs = match n.get("choices") {
                    None => &[][..],
                    Some(c) => c.as_list().filter(|c| !c.is_empty() && c.len() <= MAX_CHOICES).ok_or_else(|| bad(&format!("{at}.choices"), &format!("must be a list of 1..{MAX_CHOICES}")))?,
                };
                let choices = cs
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let at = format!("{at}.choices[{i}]");
                        let mut budget = MAX_CONDITION_NODES;
                        Ok(Choice {
                            line: Line::parse(c.get("line"), &format!("{at}.line"))?,
                            cond: c.get("if").map(|x| cond(x, &format!("{at}.if"), 0, &mut budget)).transpose()?,
                            effects: effects(c.get("do"), &format!("{at}.do"))?,
                            next: next(c.get("next"), &format!("{at}.next"))?,
                        })
                    })
                    .collect::<Result<Vec<_>, SdkError>>()?;
                if !choices.is_empty() && n.get("next").is_some_and(|x| !x.is_null()) {
                    return Err(bad(&at, "has both choices and next: a node goes on by one or the other"));
                }
                nodes.insert(
                    nid.clone(),
                    Node { speaker, line: Line::parse(n.get("line"), &format!("{at}.line"))?, effects: effects(n.get("do"), &format!("{at}.do"))?, choices, next: next(n.get("next"), &format!("{at}.next"))? },
                );
            }
            let start = name_of(g.get("start").unwrap_or(&Value::Null), &format!("{at}.start"))?;
            // Every reference points at a node of the same graph.
            let missing = |n: &Option<String>| n.as_ref().is_some_and(|n| !nodes.contains_key(n));
            if !nodes.contains_key(&start) {
                return Err(bad(&format!("{at}.start"), "names no node"));
            }
            for (nid, n) in &nodes {
                if missing(&n.next) {
                    return Err(bad(&format!("{at}.nodes.{nid}.next"), "names no node"));
                }
                if let Some(i) = n.choices.iter().position(|c| missing(&c.next)) {
                    return Err(bad(&format!("{at}.nodes.{nid}.choices[{i}].next"), "names no node"));
                }
            }
            graphs.insert(gid.clone(), Graph { start, nodes });
        }
        Ok(Data { player, talk_range_m, speakers, graphs })
    }
}

/// What the world says about an actor, for conditions.
pub struct Facts<'a> {
    pub actor: &'a specula_sdk::Entity,
}

impl Facts<'_> {
    fn items(&self, item: &str) -> u64 {
        self.actor.prop("inventory").get("slots").and_then(Value::as_list).unwrap_or_default().iter().filter(|s| s.get("item").and_then(Value::as_str) == Some(item)).filter_map(|s| s.get("count").and_then(Value::as_u64)).sum()
    }

    fn quest(&self, q: &str) -> &str {
        self.actor.prop("quests").get(q).and_then(|s| s.get("state")).and_then(Value::as_str).unwrap_or("none")
    }

    fn flag(&self, f: &str) -> bool {
        self.actor.prop("colloquium").get("flags").and_then(|fl| fl.get(f)).and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn holds(&self, c: &Cond) -> bool {
        match c {
            Cond::HasItem(i, n) => self.items(i) >= *n,
            Cond::Quest(q, s) => self.quest(q) == s,
            Cond::Flag(f) => self.flag(f),
            Cond::All(cs) => cs.iter().all(|c| self.holds(c)),
            Cond::Any(cs) => cs.iter().any(|c| self.holds(c)),
            Cond::Not(c) => !self.holds(c),
        }
    }
}

/// An actor's talk, from `props.colloquium`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Talk {
    pub graph: Option<String>,
    pub node: Option<String>,
    pub with: Option<u64>,
    /// Indices of the node's choices on offer (their conditions held when the node was entered).
    pub choices: Vec<usize>,
    pub flags: BTreeMap<String, bool>,
}

impl Talk {
    pub fn from_value(v: &Value) -> Result<Talk, SdkError> {
        if v.is_null() {
            return Ok(Talk::default());
        }
        let bad = |w: &str| SdkError::new("bad_talk", format!("props.colloquium: {w}"));
        if v.get("format").and_then(Value::as_i64) != Some(FORMAT) {
            return Err(bad(&format!("format must be {FORMAT}")));
        }
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        let choices = v.get("choices").and_then(Value::as_list).unwrap_or_default();
        if choices.len() > MAX_CHOICES {
            return Err(bad("too many choices"));
        }
        let flags = v.get("flags").and_then(Value::as_map).cloned().unwrap_or_default();
        if flags.len() > MAX_FLAGS {
            return Err(bad("too many flags"));
        }
        Ok(Talk {
            graph: s("graph"),
            node: s("node"),
            with: v.get("with").and_then(Value::as_u64),
            choices: choices.iter().filter_map(Value::as_u64).map(|c| c as usize).filter(|c| *c < MAX_CHOICES).collect(),
            flags: flags.into_iter().filter_map(|(k, b)| b.as_bool().map(|b| (k, b))).collect(),
        })
    }

    pub fn to_value(&self) -> Value {
        let mut flags = Value::map();
        for (k, b) in &self.flags {
            flags.insert(k, *b);
        }
        let choices: Vec<Value> = self.choices.iter().map(|c| Value::from(*c)).collect();
        Value::map().with("format", FORMAT).with("graph", self.graph.as_deref()).with("node", self.node.as_deref()).with("with", self.with).with("choices", choices).with("flags", flags)
    }

    pub fn talking(&self) -> bool {
        self.graph.is_some() && self.node.is_some()
    }
}

#[derive(Default)]
pub struct Colloquium;

#[cfg(not(feature = "linked"))]
specula_sdk::export_module!(Colloquium);

const ACTIONS: &str = r#"{"format": "specula.input.v1", "actions": [
  {"name": "talk", "kind": "button", "group": "dialogue", "description": "Talk to the nearest talkable entity in reach, or go on to the next line. Agents: logic_input {\"talk\": true}.", "bindings": ["Key.E", "Key.Enter", "Pad.West"]},
  {"name": "choice_1", "kind": "button", "group": "dialogue", "description": "Pick the first choice on offer.", "bindings": ["Key.1", "Pad.DpadUp"]},
  {"name": "choice_2", "kind": "button", "group": "dialogue", "description": "Pick the second choice.", "bindings": ["Key.2", "Pad.DpadRight"]},
  {"name": "choice_3", "kind": "button", "group": "dialogue", "description": "Pick the third choice.", "bindings": ["Key.3", "Pad.DpadDown"]},
  {"name": "choice_4", "kind": "button", "group": "dialogue", "description": "Pick the fourth choice.", "bindings": ["Key.4", "Pad.DpadLeft"]}
]}"#;

const ACTOR: &str = r#"{"type": "object", "required": ["actor"], "properties": {"actor": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}}}"#;

impl Module for Colloquium {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo {
            name: "colloquium",
            version: env!("CARGO_PKG_VERSION"),
            description: "Dialogue graphs as data: nodes, conditional choices and effects that ask the inventory, quest and ability modules; lines as string-table keys.",
            order: 30,
            actions: ACTIONS,
            hooks: vec![
                HookDef {
                    name: "start",
                    doc: "Starts graph for actor (entity path or id), optionally with an entity to talk to (the lines show above it). Answers the talk's state.",
                    args: r#"{"type": "object", "required": ["actor", "graph"], "properties": {"actor": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}, "graph": {"type": "string", "maxLength": 64}, "with": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}}}"#,
                },
                HookDef {
                    name: "choose",
                    doc: "Picks choice (1..4, as numbered on screen) of the actor's current node. Answers the talk's state.",
                    args: r#"{"type": "object", "required": ["actor", "choice"], "properties": {"actor": {"oneOf": [{"type": "string", "maxLength": 256}, {"type": "integer", "minimum": 0}]}, "choice": {"type": "integer", "minimum": 1, "maximum": 4}}}"#,
                },
                HookDef { name: "next", doc: "Goes on from a node without choices (or ends the talk at its end). Answers the talk's state.", args: ACTOR },
                HookDef { name: "end", doc: "Ends the actor's talk.", args: ACTOR },
                HookDef { name: "state", doc: "Reads the actor's talk: graph, node, speaker and line (key and English), choices on offer, flags (changes nothing).", args: ACTOR },
            ],
        }
    }

    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        Data::parse(DATA)?;
        if cx.find(ROOT)?.is_none() {
            cx.spawn(Spawn::new("group").name("colloquium"))?;
        }
        Ok(())
    }

    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        let starts: Vec<_> = cx.events().iter().filter(|e| e.name == "colloquium_start").take(MAX_TALKS).cloned().collect();
        let talk = cx.action("talk").pressed;
        let choice = (1..=MAX_CHOICES).find(|n| cx.action(&format!("choice_{n}")).pressed);
        let talking = cx.state("talking")?;
        let talking: Vec<u64> = talking.as_list().unwrap_or_default().iter().filter_map(Value::as_u64).collect();
        if starts.is_empty() && !talk && choice.is_none() && talking.is_empty() {
            return Ok(());
        }
        let data = Data::parse(DATA)?;
        for e in &starts {
            let graph = e.data.get("graph").and_then(Value::as_str).unwrap_or_default().to_owned();
            let r = actor_id(cx, e.data.get("actor")).and_then(|a| {
                let with = e.data.get("with").map(|w| actor_id(cx, Some(w))).transpose()?;
                begin(cx, &data, a, &graph, with).map(drop)
            });
            if let Err(err) = r {
                cx.log(&format!("colloquium_start from {} refused: {err}", e.from))?;
            }
        }
        // Talks end when the actor walks away from whom it talks to.
        for a in talking {
            let Some(actor) = cx.entity(Id(a))? else {
                forget_talker(cx, a)?;
                continue;
            };
            let t = Talk::from_value(actor.prop("colloquium"))?;
            if let Some(w) = t.with.filter(|_| t.talking()) {
                let far = match cx.entity(Id(w))? {
                    Some(npc) => (world_pos(&npc) - world_pos(&actor)).length() > data.talk_range_m * 2.0,
                    None => true,
                };
                if far {
                    finish(cx, Id(a), t, "walked_away")?;
                }
            }
        }
        if !talk && choice.is_none() {
            return Ok(());
        }
        let Some(player) = data.player.as_deref() else { return Ok(()) };
        let Some(me) = cx.find(player)? else { return Ok(()) };
        let me_e = cx.entity(me)?.ok_or_else(|| SdkError::new("not_found", "the player vanished"))?;
        let t = Talk::from_value(me_e.prop("colloquium"))?;
        if t.talking() {
            let r = match choice {
                Some(n) => choose(cx, &data, me, n),
                None => go_on(cx, &data, me),
            };
            if let Err(e) = r {
                // A choice not on offer, or "talk" at a node waiting for a choice: nothing happens.
                cx.emit("colloquium_refused", Value::map().with("actor", me.0).with("code", e.code.as_str()))?;
            }
        } else if talk {
            // The nearest talkable entity in reach.
            let at = world_pos(&me_e);
            let mut best: Option<(f32, Id, String)> = None;
            for id in cx.query_near("talkable", at, data.talk_range_m, 256)? {
                let Some(e) = cx.entity(id)? else { continue };
                let Some(graph) = e.prop("dialogue").as_str().map(str::to_owned) else { continue };
                let d = (world_pos(&e) - at).length();
                if d <= data.talk_range_m && best.as_ref().is_none_or(|(bd, _, _)| d < *bd) {
                    best = Some((d, id, graph));
                }
            }
            if let Some((_, npc, graph)) = best {
                begin(cx, &data, me, &graph, Some(npc))?;
            }
        }
        Ok(())
    }

    fn hook(&self, cx: &mut Cx, name: &str, args: &Value) -> Result<Value, SdkError> {
        let data = Data::parse(DATA)?;
        let actor = actor_id(cx, args.get("actor"))?;
        match name {
            "start" => {
                let graph = args.get("graph").and_then(Value::as_str).filter(|g| g.len() <= 64).ok_or_else(|| SdkError::new("invalid_field", "'graph' must be a graph id"))?;
                let with = args.get("with").map(|w| actor_id(cx, Some(w))).transpose()?;
                begin(cx, &data, actor, graph, with)?;
            }
            "choose" => {
                let n = args.get("choice").and_then(Value::as_u64).filter(|n| (1..=MAX_CHOICES as u64).contains(n)).ok_or_else(|| SdkError::new("invalid_field", "'choice' must be 1..4"))?;
                choose(cx, &data, actor, n as usize)?;
            }
            "next" => go_on(cx, &data, actor)?,
            "end" => {
                let e = cx.entity(actor)?.ok_or_else(|| SdkError::new("not_found", "no actor"))?;
                let t = Talk::from_value(e.prop("colloquium"))?;
                if t.talking() {
                    finish(cx, actor, t, "ended")?;
                }
            }
            "state" => {}
            other => return Err(SdkError::new("unknown_hook", format!("colloquium has no hook '{}' (it has: start, choose, next, end, state)", short(other)))),
        }
        state_of(cx, &data, actor)
    }

    fn stop(&self, cx: &mut Cx, reason: StopReason) -> Result<(), SdkError> {
        if reason == StopReason::Unload {
            if let Some(root) = cx.find(ROOT)? {
                cx.delete(root)?;
            }
            cx.set_state("talking", Value::Null)?;
        }
        Ok(())
    }
}

fn actor_id(cx: &mut Cx, v: Option<&Value>) -> Result<Id, SdkError> {
    match v {
        Some(Value::Str(p)) if p.len() <= 256 => cx.find(p)?.ok_or_else(|| SdkError::new("not_found", format!("no entity at '{}'", short(p)))),
        Some(Value::Int(i)) if *i >= 0 => {
            let id = Id(*i as u64);
            cx.entity(id)?.map(|_| id).ok_or_else(|| SdkError::new("not_found", format!("no entity {i}")))
        }
        _ => Err(SdkError::new("invalid_field", "'actor' must be an entity path or id")),
    }
}

fn world_pos(e: &specula_sdk::Entity) -> Vec3 {
    e.data.get("world_position").and_then(Value::as_vec3).unwrap_or_else(|| e.position())
}

fn graph<'d>(data: &'d Data, id: &str) -> Result<&'d Graph, SdkError> {
    data.graphs.get(id).ok_or_else(|| {
        let known: Vec<&str> = data.graphs.keys().map(String::as_str).take(24).collect();
        SdkError::new("unknown_graph", format!("no dialogue graph '{}' (graphs: {})", short(id), known.join(", ")))
    })
}

fn remember_talker(cx: &mut Cx, a: u64) -> Result<(), SdkError> {
    let mut list: Vec<Value> = cx.state("talking")?.as_list().unwrap_or_default().to_vec();
    if !list.iter().any(|v| v.as_u64() == Some(a)) {
        if list.len() >= MAX_TALKS {
            return Err(SdkError::new("limit", format!("at most {MAX_TALKS} talks at once")));
        }
        list.push(Value::from(a));
    }
    cx.set_state("talking", list)
}

fn forget_talker(cx: &mut Cx, a: u64) -> Result<(), SdkError> {
    let list: Vec<Value> = cx.state("talking")?.as_list().unwrap_or_default().iter().filter(|v| v.as_u64() != Some(a)).cloned().collect();
    cx.set_state("talking", if list.is_empty() { Value::Null } else { Value::from(list) })
}

fn begin(cx: &mut Cx, data: &Data, actor: Id, gid: &str, with: Option<Id>) -> Result<(), SdkError> {
    let g = graph(data, gid)?;
    let e = cx.entity(actor)?.ok_or_else(|| SdkError::new("not_found", "no actor"))?;
    let mut t = Talk::from_value(e.prop("colloquium"))?;
    t.graph = Some(gid.to_owned());
    t.with = with.map(|w| w.0);
    remember_talker(cx, actor.0)?;
    enter(cx, data, actor, t, &g.start.clone())
}

/// Enters node `nid`: runs its effects, works out the choices on offer, shows it.
fn enter(cx: &mut Cx, data: &Data, actor: Id, mut t: Talk, nid: &str) -> Result<(), SdkError> {
    let gid = t.graph.clone().unwrap_or_default();
    let g = graph(data, &gid)?;
    let node = g.nodes.get(nid).ok_or_else(|| SdkError::new("bad_talk", format!("graph '{gid}' has no node '{}'", short(nid))))?;
    run(cx, actor, &mut t, &node.effects)?;
    // Conditions see the world as it is on entering (effects asked of other modules land next tick).
    let e = cx.entity(actor)?.ok_or_else(|| SdkError::new("not_found", "no actor"))?;
    let e = specula_sdk::Entity { id: e.id, data: e.data.clone().with("props", props_with_flags(&e, &t)) };
    let facts = Facts { actor: &e };
    t.choices = node.choices.iter().enumerate().filter(|(_, c)| c.cond.as_ref().is_none_or(|c| facts.holds(c))).map(|(i, _)| i).collect();
    t.node = Some(nid.to_owned());
    let with = t.with;
    cx.set(actor, Patch::new().props(Value::map().with("colloquium", t.to_value())))?;
    show(cx, data, actor, with, node, &t.choices)?;
    cx.sound("talk", None, 0.8)?;
    cx.emit("colloquium_node", Value::map().with("actor", actor.0).with("graph", gid.as_str()).with("node", nid).with("with", with).with("line", node.line.key.as_str()))
}

/// The actor's props with this talk's flags (so a flag set on entering counts at once).
fn props_with_flags(e: &specula_sdk::Entity, t: &Talk) -> Value {
    let mut p = e.data.get("props").cloned().unwrap_or_else(Value::map);
    let mut flags = Value::map();
    for (k, b) in &t.flags {
        flags.insert(k, *b);
    }
    p.insert("colloquium", Value::map().with("flags", flags));
    p
}

fn run(cx: &mut Cx, actor: Id, t: &mut Talk, effects: &[Effect]) -> Result<(), SdkError> {
    let a = actor.0;
    for fx in effects {
        match fx {
            Effect::Give(i, n) => cx.emit("inventarium_give", Value::map().with("owner", a).with("item", i.as_str()).with("count", *n))?,
            Effect::Take(i, n) => cx.emit("inventarium_take", Value::map().with("owner", a).with("item", i.as_str()).with("count", *n))?,
            Effect::QuestStart(q) => cx.emit("quaestio_start", Value::map().with("actor", a).with("quest", q.as_str()))?,
            Effect::Signal(s) => cx.emit("quaestio_signal", Value::map().with("actor", a).with("signal", s.as_str()))?,
            Effect::Learn(ab) => cx.emit("facultas_learn", Value::map().with("caster", a).with("ability", ab.as_str()))?,
            Effect::Flag(f) => {
                if t.flags.len() >= MAX_FLAGS && !t.flags.contains_key(f) {
                    return Err(SdkError::new("limit", format!("an actor holds at most {MAX_FLAGS} flags")));
                }
                t.flags.insert(f.clone(), true);
            }
            Effect::Emit(ev) => cx.emit(ev, Value::map().with("actor", a))?,
        }
    }
    Ok(())
}

fn current(cx: &mut Cx, data: &Data, actor: Id) -> Result<(Talk, Node), SdkError> {
    let e = cx.entity(actor)?.ok_or_else(|| SdkError::new("not_found", "no actor"))?;
    let t = Talk::from_value(e.prop("colloquium"))?;
    if !t.talking() {
        return Err(SdkError::new("not_talking", "the actor isn't in a talk (start one)"));
    }
    let g = graph(data, t.graph.as_deref().unwrap_or_default())?;
    // The data may have changed under a saved talk: a vanished node ends it.
    let node = g.nodes.get(t.node.as_deref().unwrap_or_default()).cloned().ok_or_else(|| SdkError::new("bad_talk", "the talk's node is gone from the data"))?;
    Ok((t, node))
}

fn choose(cx: &mut Cx, data: &Data, actor: Id, n: usize) -> Result<(), SdkError> {
    let (mut t, node) = current(cx, data, actor)?;
    let Some(&index) = t.choices.get(n.wrapping_sub(1)) else {
        return Err(SdkError::new("no_such_choice", format!("{} choice(s) on offer; asked for {n}", t.choices.len())));
    };
    let c = node.choices.get(index).cloned().ok_or_else(|| SdkError::new("bad_talk", "the choice is gone from the data"))?;
    cx.emit("colloquium_choice", Value::map().with("actor", actor.0).with("graph", t.graph.as_deref()).with("node", t.node.as_deref()).with("choice", index).with("line", c.line.key.as_str()))?;
    run(cx, actor, &mut t, &c.effects)?;
    match &c.next {
        Some(next) => enter(cx, data, actor, t, next),
        None => finish(cx, actor, t, "ended"),
    }
}

fn go_on(cx: &mut Cx, data: &Data, actor: Id) -> Result<(), SdkError> {
    let (t, node) = current(cx, data, actor)?;
    if !node.choices.is_empty() {
        return Err(SdkError::new("choose_first", "this node waits for a choice (choose 1..4)"));
    }
    match &node.next {
        Some(next) => enter(cx, data, actor, t, next),
        None => finish(cx, actor, t, "ended"),
    }
}

fn finish(cx: &mut Cx, actor: Id, mut t: Talk, how: &str) -> Result<(), SdkError> {
    let (graph, node) = (t.graph.take(), t.node.take());
    t.with = None;
    t.choices.clear();
    cx.set(actor, Patch::new().props(Value::map().with("colloquium", t.to_value())))?;
    forget_talker(cx, actor.0)?;
    hide(cx, actor)?;
    cx.emit("colloquium_ended", Value::map().with("actor", actor.0).with("graph", graph).with("node", node).with("how", how))
}

/// The talk's text entities: `/colloquium/talk_<actor>` holds the line and the choices.
fn show(cx: &mut Cx, data: &Data, actor: Id, with: Option<u64>, node: &Node, offered: &[usize]) -> Result<(), SdkError> {
    hide(cx, actor)?;
    let anchor = match with {
        Some(w) => cx.entity(Id(w))?,
        None => cx.entity(actor)?,
    };
    let at = anchor.as_ref().map_or(Vec3::ZERO, world_pos) + Vec3::new(0.0, 2.5, 0.0);
    let root = match cx.find(ROOT)? {
        Some(r) => r,
        None => cx.spawn(Spawn::new("group").name("colloquium"))?,
    };
    let panel = cx.spawn(Spawn::new("group").name(&format!("talk_{}", actor.0)).field("parent", root.0).position(at))?;
    let speaker = data.speakers.get(&node.speaker);
    let line_text = match speaker {
        Some(s) => format!("{}: {}", s.text, node.line.text),
        None => node.line.text.clone(),
    };
    text_entity(cx, panel, "line", &line_text, &node.line, speaker, Vec3::ZERO, 0.17, "#e8e4d8")?;
    for (n, &i) in offered.iter().enumerate() {
        let c = &node.choices[i];
        text_entity(cx, panel, &format!("choice_{}", n + 1), &format!("{}. {}", n + 1, c.line.text), &c.line, None, Vec3::new(0.0, -0.32 * (n as f32 + 1.0) - 0.1, 0.0), 0.14, "#5ee0e0")?;
    }
    if offered.is_empty() {
        text_entity(cx, panel, "hint", "[E]", &Line { key: "dialogue.common.continue_hint".into(), text: "[E]".into() }, None, Vec3::new(0.0, -0.42, 0.0), 0.12, "#9aa3ad")?;
    }
    Ok(())
}

/// One world text. Its `props.line` carries the string-table key beside the English text shown,
/// so a localised build (or an agent) can resolve it; the text itself is the English fallback.
#[allow(clippy::too_many_arguments)]
fn text_entity(cx: &mut Cx, panel: Id, name: &str, shown: &str, line: &Line, speaker: Option<&Line>, at: Vec3, size: f32, color: &str) -> Result<Id, SdkError> {
    let mut props = Value::map().with("line", line.to_value());
    if let Some(s) = speaker {
        props.insert("speaker", s.to_value());
    }
    cx.spawn(
        Spawn::new("text")
            .name(name)
            .field("parent", panel.0)
            .position(at)
            .color(color)
            .tags(&["colloquium_text"])
            .props(props)
            .field("text", Value::map().with("text", shown).with("size", size).with("max_width", 4.0).with("align", "center").with("anchor", [0.5f32, 0.0])),
    )
}

fn hide(cx: &mut Cx, actor: Id) -> Result<(), SdkError> {
    if let Some(p) = cx.find(&format!("{ROOT}/talk_{}", actor.0))? {
        cx.delete(p)?;
    }
    Ok(())
}

fn state_of(cx: &mut Cx, data: &Data, actor: Id) -> Result<Value, SdkError> {
    let e = cx.entity(actor)?.ok_or_else(|| SdkError::new("not_found", "no actor"))?;
    let t = Talk::from_value(e.prop("colloquium"))?;
    let mut flags = Value::map();
    for (k, b) in &t.flags {
        flags.insert(k, *b);
    }
    let mut out = Value::map().with("talking", t.talking()).with("flags", flags);
    if let (Some(g), Some(n)) = (&t.graph, &t.node) {
        if let Some(node) = data.graphs.get(g).and_then(|g| g.nodes.get(n)) {
            let choices: Vec<Value> = t.choices.iter().filter_map(|i| node.choices.get(*i)).map(|c| c.line.to_value()).collect();
            out = out
                .with("graph", g.as_str())
                .with("node", n.as_str())
                .with("with", t.with)
                .with("speaker", data.speakers.get(&node.speaker).map(Line::to_value))
                .with("line", node.line.to_value())
                .with("choices", choices)
                .with("waits_for", if node.choices.is_empty() { "next" } else { "choice" });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
