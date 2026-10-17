//! The language as data: every item kind, behaviour, action, payload and function with its
//! fields, types and units. The compiler checks field names against these tables (so the schema
//! an agent reads is the one that is enforced), and `lex_schema` serves them.
//!
//! Field types: `int`, `points`, `distance`, `speed`, `duration`, `ratio` (numbers in that unit:
//! a bare number in human units, a literal like `"7m"`, or an expression), `bool expr`,
//! `ref:<kind>` (an item id), `tags`, `actions`, `string`, and compound forms described in `doc`.

use specula_core::Json;

use crate::rules::BUILTIN_EVENTS;
use crate::units::Unit;

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    pub ty: &'static str,
    pub required: bool,
    pub doc: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub name: &'static str,
    pub doc: &'static str,
    pub fields: &'static [Field],
}

impl Spec {
    /// Field names, plus the always-allowed `notes` (and `kind` / `type` / the verb key).
    pub fn names(&self, extra: &[&'static str]) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = self.fields.iter().map(|f| f.name).collect();
        v.push("notes");
        v.extend_from_slice(extra);
        v
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("name", Json::from(self.name)),
            ("doc", Json::from(self.doc)),
            (
                "fields",
                Json::arr(self.fields.iter().map(|f| {
                    Json::obj([
                        ("name", Json::from(f.name)),
                        ("type", Json::from(f.ty)),
                        ("required", Json::Bool(f.required)),
                        ("doc", Json::from(f.doc)),
                    ])
                })),
            ),
        ])
    }
}

const fn f(name: &'static str, ty: &'static str, required: bool, doc: &'static str) -> Field {
    Field { name, ty, required, doc }
}

pub const KINDS: &[Spec] = &[
    Spec {
        name: "param",
        doc: "A named, typed, ranged number, used as $name in any field or expression. The balance lab tunes params; lex_simulate takes overrides.",
        fields: &[
            f("type", "unit name", false, "int (default), points, distance, speed, duration or ratio"),
            f("value", "number in the type's unit", true, "the current value"),
            f("min", "number", false, "lowest allowed value"),
            f("max", "number", false, "highest allowed value"),
            f("doc", "string", false, "what it controls, for the balance lab and other agents"),
        ],
    },
    Spec {
        name: "resource",
        doc: "A global counter (gold, lives). Changes clamp to [min, max]. Read as res.<id>.",
        fields: &[
            f("start", "number", true, "value at the start of a run"),
            f("min", "number", false, "lowest value (default: unbounded)"),
            f("max", "number", false, "highest value (default: unbounded)"),
            f("unit", "unit name", false, "int (default) or another unit"),
        ],
    },
    Spec {
        name: "path",
        doc: "A polyline in metres that follow_path and patrol walk, and that waves spawn on.",
        fields: &[
            f("points", "[[x, y, z], ...] metres", true, "at least 2 points; no zero-length segments"),
            f("loop", "bool", false, "patrols wrap from the last point to the first"),
        ],
    },
    Spec {
        name: "sites",
        doc: "Named positions: build spots and spawn points. Several sites items merge; names must be unique.",
        fields: &[f("points", "{name: [x, y, z]} metres", true, "site name to position; names may use any characters")],
    },
    Spec {
        name: "archetype",
        doc: "An entity template: tags, attributes, behaviours, an optional machine and an upgrade chain (next).",
        fields: &[
            f("extends", "ref:archetype", false, "inherit tags, attributes and behaviours (same name replaces)"),
            f("abstract", "bool", false, "a base for others: can't be spawned"),
            f("tags", "tags", false, "e.g. [\"enemy\", \"ground\"]"),
            f("attributes", "{name: number | literal | {base, unit}}", false, "a bare number is an int; \"7m\" infers the unit; or {\"base\": 7, \"unit\": \"distance\"}"),
            f("behaviours", "[behaviour]", false, "components: see the behaviour types"),
            f("machine", "ref:machine", false, "a state machine that switches behaviours on and off"),
            f("next", "ref:archetype", false, "the next tier (read as self.next / arch.next; used by become)"),
            f("on_spawn", "actions", false, "run when an entity of this archetype appears (self)"),
        ],
    },
    Spec {
        name: "effect",
        doc: "Gameplay-Ability-System-style effect: modifies attributes for a duration, periodically or instantly; grants tags; stacks by policy.",
        fields: &[
            f("duration", "\"instant\" | duration | \"infinite\"", false, "default instant (changes base values once)"),
            f("period", "duration", false, "apply the modifiers to base values every period (damage over time)"),
            f("magnitude", "unit name", false, "the unit of `magnitude` (set by the applier); default ratio"),
            f("modifiers", "[{attribute, op, value}]", false, "op: add, mul (by a ratio), set; value: an expression (may use magnitude)"),
            f("stacking", "string", false, "strongest (default), refresh, stack, independent, ignore"),
            f("max_stacks", "int", false, "for stack and independent: 1 to 8 (default 1); an entity carries at most 8 effect instances"),
            f("grant_tags", "tags", false, "tags the holder has while it is active"),
            f("block_tags", "tags", false, "a target with any of these tags is immune"),
            f("on_apply", "actions", false, "self: the holder, other: the source"),
            f("on_period", "actions", false, "every period"),
            f("on_expire", "actions", false, "when it runs out"),
        ],
    },
    Spec {
        name: "machine",
        doc: "A state machine. Behaviours named in some state run only in the states that list them; others always run.",
        fields: &[
            f("initial", "string", true, "the starting state"),
            f("states", "{name: {behaviours, on_enter, on_exit, transitions}}", true, "transitions: [{to, when?, after?, on?}]; the first that passes fires"),
        ],
    },
    Spec {
        name: "waves",
        doc: "The encounter schedule. Several waves items join in id order into one schedule (each item is at most 16 KB; only the first sets start and rng). Each wave starts when the last one's duration runs out (or on next_wave), runs on_start, then schedules its groups.",
        fields: &[
            f("start", "duration", false, "time before wave 1 (default 0)"),
            f("rng", "string", false, "the random stream for spawn points (default waves)"),
            f("waves", "[{duration, on_start, scale, groups}]", true, "groups: [{archetype, count, at: [path or site ids], start, every}]; scale: {attribute: ratio}; duration may be omitted on the last wave"),
        ],
    },
    Spec {
        name: "timer",
        doc: "Runs actions at start, then every `every`, up to `count` times, while `when` holds.",
        fields: &[
            f("start", "duration", false, "first firing (default 0)"),
            f("every", "duration", false, "repeat interval (default: once)"),
            f("count", "int", false, "most firings"),
            f("when", "bool expr", false, "skip a firing while false"),
            f("do", "actions", true, "what happens"),
        ],
    },
    Spec {
        name: "rule",
        doc: "A trigger: when an event happens (to an entity matching tags, and when holds), run actions. self: the subject, other: the instigator.",
        fields: &[
            f("on", "event name", true, "a built-in event or a custom one raised by emit"),
            f("tags", "tags", false, "the subject must match (\"!tag\" excludes)"),
            f("when", "bool expr", false, "extra condition"),
            f("do", "actions", true, "what happens"),
        ],
    },
    Spec {
        name: "command",
        doc: "Player or agent input as data: typed args, a cost, requirements with rejection codes, then actions. Atomic.",
        fields: &[
            f("args", "{name: \"site\" | \"archetype\" | \"entity\" | unit name | {type, tags}}", false, "entity args accept an entity id or a site name (its occupant)"),
            f("cost", "{resource: expr}", false, "checked first (rejection not_enough_<resource>), then paid"),
            f("require", "[{when, code, message?}]", false, "checked in order; the first false is the rejection"),
            f("do", "actions", true, "what happens"),
        ],
    },
    Spec {
        name: "outcome",
        doc: "Ends the run when `when` becomes true. Checked whenever state changes; the highest priority (then id) wins.",
        fields: &[
            f("result", "string", true, "won, lost or any name"),
            f("when", "bool expr", true, "the condition"),
            f("priority", "int", false, "default 0; higher is checked first"),
        ],
    },
    Spec {
        name: "native",
        doc: "The escape hatch: a native (module) hook, declared with why Lex couldn't express it. Every run counts its calls.",
        fields: &[
            f("hook", "string", true, "the hook's registered name, e.g. sepulcra.flow_route"),
            f("reason", "string", true, "why no Lex building block fits (this becomes the feature backlog)"),
            f("phase", "string", false, "spawn, move, act, effects (for behaviours) or any (actions only; default)"),
        ],
    },
];

pub const BEHAVIOURS: &[Spec] = &[
    Spec {
        name: "health",
        doc: "The entity dies when the attribute drops to 0 or below; on_death runs at once (other: the killer).",
        fields: &[f("attribute", "attribute name", true, "usually hp (points)"), f("on_death", "actions", false, "drops, scores")],
    },
    Spec {
        name: "follow_path",
        doc: "Walks a path at speed x scale (mm/s x permille, with a remainder carried tick to tick); on_arrive runs at its end. Sets path_remaining.",
        fields: &[
            f("path", "ref:path | \"@spawn\"", true, "@spawn: the path a wave spawned it on"),
            f("speed", "speed", true, "e.g. self.speed"),
            f("scale", "ratio", false, "e.g. self.speed_scale (slows); default 100%"),
            f("on_arrive", "actions", false, "e.g. lose a life and despawn"),
        ],
    },
    Spec {
        name: "patrol",
        doc: "Walks a path forever: loop or pingpong.",
        fields: &[
            f("path", "ref:path", true, ""),
            f("speed", "speed", true, ""),
            f("scale", "ratio", false, ""),
            f("mode", "string", false, "loop (default) or pingpong"),
        ],
    },
    Spec {
        name: "chase",
        doc: "Moves toward the nearest matching entity within range; on_reach runs when within reach (target: that entity).",
        fields: &[
            f("targets", "tags", true, ""),
            f("range", "distance", true, "acquire radius"),
            f("speed", "speed", true, ""),
            f("scale", "ratio", false, ""),
            f("reach", "distance", false, "default 0.5m"),
            f("on_reach", "actions", false, ""),
        ],
    },
    Spec {
        name: "attack",
        doc: "Fires at eligible targets (alive, matching tags, within range) when its cooldown is ready.",
        fields: &[
            f("targets", "tags", true, "e.g. [\"enemy\", \"!flying\"]"),
            f("range", "distance", true, "3D, from the entity"),
            f("cooldown", "duration", true, "between shots"),
            f("policy", "string", false, "first (default: least path_remaining), last, closest, strongest, weakest; ties: lowest id"),
            f("delivery", "string", false, "projectile (default if projectile is set), instant, pulse (every eligible target)"),
            f("projectile", "{archetype, with?}", false, "the projectile archetype and attributes to set on it"),
            f("muzzle", "[x, y, z] metres", false, "projectile start offset"),
            f("blocked_by", "tags", false, "no firing while the attacker has any of these"),
            f("payload", "[payload]", true, "what a hit does: {damage} or {effect, magnitude?, duration?}"),
        ],
    },
    Spec {
        name: "projectile",
        doc: "Flies toward its aim (following its target when homing) and delivers the attack's payload on arrival: to its target, or within splash of the aim.",
        fields: &[
            f("speed", "speed", true, "e.g. self.speed"),
            f("homing", "bool", false, "default true: follows the target, fizzles if it dies; false: lands where the target stood"),
            f("splash", "distance", false, "default 0: single target"),
        ],
    },
    Spec {
        name: "spawner",
        doc: "Spawns `count` of an archetype at itself every `every` (summons continue its path).",
        fields: &[
            f("archetype", "ref:archetype", true, ""),
            f("every", "duration", true, ""),
            f("count", "int", false, "default 1"),
            f("start", "duration", false, "delay after spawning; default every"),
        ],
    },
    Spec {
        name: "lifetime",
        doc: "Despawns the entity after a duration (on_expire runs first).",
        fields: &[f("duration", "duration", true, ""), f("on_expire", "actions", false, "")],
    },
    Spec {
        name: "native",
        doc: "Runs a declared native hook for this entity in the hook's phase.",
        fields: &[f("hook", "ref:native", true, ""), f("args", "{name: expr}", false, "numbers passed to the hook")],
    },
];

pub const ACTIONS: &[Spec] = &[
    Spec { name: "add", doc: "Add to a resource (clamped).", fields: &[f("add", "ref:resource", true, ""), f("amount", "expr in the resource's unit", true, "")] },
    Spec { name: "set", doc: "Set a resource (clamped).", fields: &[f("set", "ref:resource", true, ""), f("value", "expr", true, "")] },
    Spec {
        name: "change",
        doc: "Change an entity attribute's base value.",
        fields: &[f("change", "\"<who>.<attribute>\"", true, "e.g. self.invested"), f("by", "expr", false, "add this"), f("to", "expr", false, "or set to this")],
    },
    Spec {
        name: "spawn",
        doc: "Spawn entities.",
        fields: &[
            f("spawn", "ref:archetype | archetype expr", true, "e.g. warding_1 or arg.stone"),
            f("at", "path id | site | entity expr", true, "self, target, arg.site, a site name or a path id"),
            f("count", "int", false, "default 1"),
            f("with", "{attribute: expr}", false, "base values to set (evaluated before spawning)"),
        ],
    },
    Spec { name: "despawn", doc: "Remove an entity without it dying (no on_death).", fields: &[f("despawn", "entity expr", true, "")] },
    Spec { name: "kill", doc: "The entity dies (on_death runs).", fields: &[f("kill", "entity expr", true, "")] },
    Spec { name: "damage", doc: "Lower an entity's health attribute.", fields: &[f("damage", "points expr", true, ""), f("to", "entity expr", false, "default target")] },
    Spec {
        name: "apply",
        doc: "Apply an effect.",
        fields: &[f("apply", "ref:effect", true, ""), f("to", "entity expr", false, "default target, else self"), f("magnitude", "expr", false, ""), f("duration", "duration", false, "overrides the effect's")],
    },
    Spec {
        name: "emit",
        doc: "Raise a custom event for rules.",
        fields: &[f("emit", "event name", true, ""), f("subject", "entity expr", false, ""), f("value", "int expr", false, "")],
    },
    Spec {
        name: "become",
        doc: "Turn an entity into another archetype in place (same id, position and behaviour state where names match).",
        fields: &[f("become", "archetype expr", true, "e.g. self.next"), f("who", "entity expr", false, "default self"), f("keep", "[attribute]", false, "base values to carry over")],
    },
    Spec { name: "next_wave", doc: "Start the next wave now.", fields: &[f("next_wave", "true", true, "")] },
    Spec { name: "goto", doc: "Switch the entity's machine state.", fields: &[f("goto", "state name", true, ""), f("who", "entity expr", false, "default self")] },
    Spec { name: "outcome", doc: "End the run with a result.", fields: &[f("outcome", "string", true, "")] },
    Spec {
        name: "if",
        doc: "Branch.",
        fields: &[f("if", "bool expr", true, ""), f("then", "actions", false, ""), f("else", "actions", false, "")],
    },
    Spec { name: "native", doc: "Call a declared native hook.", fields: &[f("native", "ref:native", true, ""), f("args", "{name: expr}", false, "")] },
];

pub const PAYLOADS: &[Spec] = &[
    Spec { name: "damage", doc: "Lower the target's health attribute (evaluated when fired).", fields: &[f("damage", "points expr", true, "")] },
    Spec {
        name: "effect",
        doc: "Apply an effect to the target.",
        fields: &[f("effect", "ref:effect", true, ""), f("magnitude", "expr", false, ""), f("duration", "duration", false, "")],
    },
];

pub const FUNCTIONS: &[(&str, &str, &str)] = &[
    ("min", "min(a, b)", "same unit"),
    ("max", "max(a, b)", "same unit"),
    ("clamp", "clamp(x, lo, hi)", "same unit"),
    ("abs", "abs(x)", ""),
    ("if", "if(cond, a, b)", "a and b of the same type"),
    ("exists", "exists(entity | archetype)", "a live entity, or an archetype that is set (e.g. exists(self.next))"),
    ("has", "has(entity, tag)", "tags written bare: has(target, flying)"),
    ("count", "count(tag)", "live entities with the tag"),
    ("dist", "dist(a, b)", "distance between two entities"),
    ("occupied", "occupied(site, tag)", "a live entity with the tag stands on the site"),
    ("rand", "rand(stream, lo, hi)", "uniform in lo..=hi from a named, seeded stream"),
    ("chance", "chance(stream, ratio)", "true with that probability"),
];

pub const NAMES: &[(&str, &str)] = &[
    ("self", "the entity the rule runs for (behaviours, effects, rules)"),
    ("other", "the instigator (killer, effect source, attacker)"),
    ("target", "the entity hit (payloads) or chased"),
    ("arg.<name>", "command arguments"),
    ("res.<resource>", "a resource's value"),
    ("$<param>", "a param's value"),
    ("tick", "the current tick (a duration)"),
    ("magnitude", "inside effects: the applier's magnitude"),
    ("event.value", "inside rules: the event's value"),
    ("waves.current|total|remaining|ticks_left|all_cleared", "the wave schedule"),
    ("<entity>.<attribute>", "current value (with effects); also alive, archetype, next, path_remaining"),
    ("<archetype>.<attribute>", "base value; also next"),
];

/// The schema as JSON, for one topic or an index of topics.
pub fn to_json(topic: Option<&str>) -> Result<Json, String> {
    let list = |specs: &[Spec]| Json::arr(specs.iter().map(Spec::to_json));
    let units = Json::arr(Unit::ALL.iter().map(|u| {
        Json::obj([("name", Json::from(u.name())), ("bare_number_means", Json::from(u.human())), ("suffix", Json::from(u.suffix()))])
    }));
    Ok(match topic {
        None | Some("index") => Json::obj([
            ("format", Json::from("{\"lex\": 1, \"name\": ..., \"tick_hz\": 30, \"items\": {\"<id>\": {\"kind\": ..., ...}}}; ids [a-z][a-z0-9_]*")),
            ("kinds", Json::arr(KINDS.iter().map(|k| Json::obj([("name", Json::from(k.name)), ("doc", Json::from(k.doc))])))),
            ("topics", Json::arr(["kinds", "behaviours", "actions", "payloads", "expressions", "events", "units", "tick"].map(Json::from))),
            ("hint", Json::from("lex_schema {topic} gives each topic's fields; lex_validate checks a set and says what to change")),
        ]),
        Some("kinds") => list(KINDS),
        Some("behaviours") => list(BEHAVIOURS),
        Some("actions") => list(ACTIONS),
        Some("payloads") => list(PAYLOADS),
        Some("units") => units,
        Some("events") => Json::arr(BUILTIN_EVENTS.iter().map(|(n, u, d)| {
            Json::obj([("name", Json::from(*n)), ("value_unit", Json::from(u.name())), ("doc", Json::from(*d))])
        })),
        Some("expressions") => Json::obj([
            ("operators", Json::from("|| && == != < <= > >= + - * / % ! (unary -); comparisons don't chain")),
            ("unit_rules", Json::from("+ - % and comparisons need one unit (a bare 0 matches any); * or / by a plain number keeps the unit; * ratio scales (x * 35% = x*350/1000, truncating); u / u is a plain number; plain numbers are integers")),
            ("literals", Json::from("5, 7m, 2.2m/s, 0.8s, 24t, 35%, 1.35x, 10pt, true, false")),
            ("names", Json::arr(NAMES.iter().map(|(n, d)| Json::obj([("name", Json::from(*n)), ("doc", Json::from(*d))])))),
            ("functions", Json::arr(FUNCTIONS.iter().map(|(n, s, d)| Json::obj([("name", Json::from(*n)), ("signature", Json::from(*s)), ("doc", Json::from(*d))])))),
            ("semantics", Json::from("integers only; / truncates toward zero; overflow saturates; x / 0 is 0 and is reported in the run's diagnostics")),
        ]),
        Some("tick") => Json::from(
            "1 commands (in order, each atomic) 2 waves due 3 spawns due 4 timers (by id) 5 spawners 6 movement 7 attacks 8 projectiles 9 effects, lifetimes, machines 10 cleanup (remove the dead, clear waves). Entities by id within each phase; entities created during a phase join from the next. Events reach rules after each phase. Outcomes are checked after every change; the first true one ends the run on the spot.",
        ),
        Some(other) => {
            return Err(format!(
                "no schema topic '{other}' (topics: index, kinds, behaviours, actions, payloads, expressions, events, units, tick)"
            ))
        }
    })
}

pub fn kind(name: &str) -> Option<&'static Spec> {
    KINDS.iter().find(|k| k.name == name)
}

pub fn behaviour(name: &str) -> Option<&'static Spec> {
    BEHAVIOURS.iter().find(|k| k.name == name)
}

pub fn action(name: &str) -> Option<&'static Spec> {
    ACTIONS.iter().find(|k| k.name == name)
}

pub fn payload(name: &str) -> Option<&'static Spec> {
    PAYLOADS.iter().find(|k| k.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_renders() {
        for t in ["index", "kinds", "behaviours", "actions", "payloads", "expressions", "events", "units", "tick"] {
            assert!(to_json(Some(t)).is_ok(), "{t}");
        }
        assert!(to_json(Some("nope")).unwrap_err().contains("topics:"));
        let names: Vec<&str> = KINDS.iter().map(|k| k.name).collect();
        assert!(names.contains(&"archetype") && names.contains(&"native"));
    }
}
