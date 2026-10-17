# Design: Lex, gameplay as data

Status: designed and built 2026-10-17 (M3, the Lex lane): the `specula-lex` crate (parse, validate, compile, a
deterministic runtime) and the engine's `lex_*` tools. Foundations **F1** (swarm-first) and **F2** (determinism).
Roadmap item: "Gameplay as data: Lex" in [ROADMAP.md](../ROADMAP.md); research: [AGENT_GAMEMAKING.md](../research/AGENT_GAMEMAKING.md).

**Goal:** agents build a game's rules as typed text data, checked on every edit by a validator whose errors say
exactly what to change, and run them headless, deterministically, thousands of times. Code is the escape hatch, and
each use of it is declared with a reason, so Lex grows by evidence.

*Lex* is Latin for "law".

## Why data, and why this shape

- **Agents write valid data far more reliably than valid code** when the data has a grammar and the tool answers
  with compiler-quality errors (Ludii, VGDL, PuzzleScript results in the research). So every value in Lex has a
  declared type and unit, every reference is checked, and every error names the item, the field path, what was
  expected, what was found, and a suggestion.
- **Unreal's Gameplay Ability System shows the model scales**: attributes, effects as data (duration, period,
  stacking, modifiers), and tags. Lex takes that core and adds the parts a whole game needs: behaviours, spawns,
  waves, economies, commands (player input) and outcomes.
- **One flat namespace of items**, each with a `kind`. References are by id and typed ("expected an effect, got
  'runner', an archetype"). Items are small, so the engine stores each as its own entity: two agents editing two
  archetypes never conflict, and claims, versions, the journal and undo work per item.
- **Integers at run time.** Content speaks metres, seconds, m/s, percent and points; the compiler converts once to
  millimetres, ticks, mm/s, permille and milli-points. No float ever enters the simulation (F2 level D3, F6), the
  same choice Sepulcra's hand-written sim made.

## The document

A rule set is one JSON document (`*.lex.json`; JSON like every other Specula data file):

```json
{
  "lex": 1,
  "name": "sepulcra_slice",
  "tick_hz": 30,
  "items": {
    "dust":     { "kind": "resource", "start": 150, "min": 0 },
    "lane_mid": { "kind": "path", "points": [[-1, 0, 19], [-1, 0, 3]] },
    "slow":     { "kind": "effect", "duration": "1.5s", "stacking": "strongest",
                  "modifiers": [{ "attribute": "speed_scale", "op": "mul", "value": "100% - magnitude" }] },
    "shambler": { "kind": "archetype", "tags": ["enemy", "ground"],
                  "attributes": { "hp": { "base": 55, "unit": "points" }, "speed": { "base": 0.8, "unit": "speed" },
                                  "speed_scale": { "base": 1, "unit": "ratio" } },
                  "behaviours": [
                    { "type": "health", "attribute": "hp",
                      "on_death": [{ "add": "dust", "amount": "rand(drops, 3, 5)" }] },
                    { "type": "follow_path", "path": "@spawn", "speed": "self.speed", "scale": "self.speed_scale",
                      "on_arrive": [{ "add": "tombstones", "amount": "-1" }, { "despawn": "self" }] } ] }
  }
}
```

- `lex` is the format version. A document with a newer version is refused with `unsupported_version`; older
  versions are migrated on load by the compiler (none exist yet; each migration will be a pure function from
  version N to N+1, listed in `specula-lex`, so an old file always loads and saves forward).
- `tick_hz` is the simulation rate (default 60; Sepulcra uses 30). Durations convert with it.
- Item ids are `[a-z][a-z0-9_]*`, at most 64 characters. Exported documents list items sorted by id with keys
  sorted, so the text form is canonical and diff-friendly, and the document hash ignores whitespace and key order.
- `notes` (string) is allowed on every item and on the document, and never affects the hash of the compiled rules.

### Values, units and expressions

Every numeric field has a **unit**. A bare number is read in the field's human unit; a string is an expression.

| Unit | Write | Stored as | Literal |
|---|---|---|---|
| `int` | whole numbers (counts, dust) | the number | `5` |
| `points` | HP, damage, mana | milli-points | `10pt` |
| `distance` | metres | mm | `7m` |
| `speed` | metres per second | mm/s | `2.2m/s` |
| `duration` | seconds | ticks | `0.8s`, `24t` |
| `ratio` | 0..1, percent or a multiplier | permille | `35%`, `1.35x` |

A value that isn't a whole number of the stored unit (0.25 s at 30 Hz is 7.5 ticks) is rounded half away from zero
and reported as an `inexact` warning that names both nearest exact values.

Expressions are short strings: `"res.dust >= arg.stone.cost"`, `"waves.ticks_left * 50% / 1s"`,
`"rand(drops, 3, 5)"`. They are parsed, type-checked and constant-folded at compile time:

- **Types:** numbers with a unit, booleans, entities, archetypes and sites. `+ - < >` need matching units (a bare
  `0` matches anything); `x * 35%` scales by a ratio; `distance / distance` is a plain number; anything else is a
  `unit_mismatch` error with a suggestion (`self.range > 7` → "write 7m"). This catches the classic data bug, a
  metre value where millimetres were meant.
- **Integer semantics:** `/` truncates toward zero; ratio products truncate (`x * 35%` is `x * 350 / 1000`);
  overflow saturates; division by zero gives 0. Both are reported as run-time diagnostics, never panics.
- **Names:** `self`, `other`, `target`, `arg.<name>` (command arguments), `res.<resource>`, `$<param>`, `tick`,
  `magnitude` (inside effects), `event.value` (in rules), `waves.current|total|remaining|ticks_left|all_cleared`.
  Members: an entity's attributes and `alive`, `archetype`, `next`, `path_remaining`; an archetype's base attributes
  and `next`.
- **Functions:** `min`, `max`, `clamp`, `abs`, `if(c, a, b)`, `exists(x)`, `has(x, tag)`, `count(tag)`,
  `dist(a, b)`, `occupied(site, tag)`, `rand(stream, lo, hi)`, `chance(stream, ratio)`.
- **Which names exist where** is checked per field: `target` exists in attack payloads, projectile hits and rules on
  events that carry one; `arg` only in commands; `magnitude` only in effects. The error says where it is available.

### Item kinds

| Kind | What it is |
|---|---|
| `param` | A named, typed, ranged number (`type`, `value`, `min`, `max`). Used as `$name` anywhere. The balance lab tunes these. |
| `resource` | A global counter: `start`, `min`, `max`, `unit`. Changes clamp to the range. |
| `path` | A polyline (`points` in metres, optional `loop`). |
| `sites` | Named positions (`points: {"top:8,16": [x, y, z]}`): build spots and spawn points. |
| `archetype` | An entity template: `tags`, `attributes`, `behaviours`, optional `machine`, `next` (upgrade chain), `extends`, `abstract`, `on_spawn`. |
| `effect` | GAS-style: `duration` (`instant`, a duration or `infinite`), `period`, `modifiers`, `stacking`, `max_stacks`, `grant_tags`, `block_tags`, `on_apply`, `on_period`, `on_expire`. |
| `machine` | A state machine: `initial`, `states` with `behaviours` (which are active), `on_enter`, `on_exit`, `transitions` (`when`, `after`, `on`). |
| `waves` | An encounter schedule: `start`, `rng` stream, `waves` with `duration`, `on_start`, `scale` (attribute ratios for spawns) and `groups` (`archetype`, `count`, `at`, `start`, `every`). |
| `timer` | `start`, `every`, `count`, `when`, `do`. |
| `rule` | A trigger: `on` an event, filtered by `tags` and `when`, `do` actions. |
| `command` | Player (or agent) input as data: typed `args`, `cost`, `require` (each with a rejection `code`), `do`. |
| `outcome` | `result` (`won`, `lost` or any name), `when`, `priority`. The first true outcome ends the run. |
| `native` | A declared escape hatch: `hook` (the native function's name), `reason` (required), `phase`. |

**Archetypes and inheritance.** `extends` copies the parent's tags (unioned), attributes (child overrides by name)
and behaviours (child replaces a parent's behaviour with the same `name`, which defaults to the type). Tier chains
are archetypes linked by `next`: `warding_1 → warding_2 → warding_3`.

### Behaviours (components)

Every behaviour has a `type`, an optional `name`, and typed fields; numeric fields are expressions evaluated in the
entity's context (constants fold at compile time).

| Type | Phase | Fields |
|---|---|---|
| `health` | (on change) | `attribute`; `on_death` actions. The entity dies when the attribute drops to 0 or below. |
| `follow_path` | move | `path` (an id, or `@spawn` for the path it was spawned on), `speed`, `scale`, `on_arrive`. Exposes `path_remaining`. |
| `patrol` | move | `path`, `speed`, `scale`, `mode` (`loop`, `pingpong`). |
| `chase` | move | `targets` (tag filter), `range`, `speed`, `scale`, `reach`, `on_reach`. |
| `attack` | act | `targets`, `range`, `cooldown`, `policy` (`first`, `last`, `closest`, `strongest`, `weakest`), `delivery` (`instant`, `pulse`, `projectile`), `projectile` (`archetype`, `set`), `muzzle`, `blocked_by` (tags), `payload`. |
| `projectile` | projectiles | `speed`, `homing`, `splash`. Carries the attack's payload, evaluated when fired. |
| `spawner` | spawn | `archetype`, `every`, `count`, `start`. |
| `lifetime` | effects | `duration`, `on_expire`. |
| `native` | its declared phase | `hook`, `args`. |

**Payloads** are what an attack does to what it hits: `{"damage": "10pt"}` (an instant hit on the target's health
attribute) and `{"effect": "slow", "magnitude": "35%", "duration": "1.5s"}`. Magnitudes are evaluated when the attack
fires, in the attacker's context, and travel with the projectile: selling a tower mid-flight changes nothing.

**Targeting**: eligible targets are alive, match the tag filter (`["enemy", "!flying"]`) and are within `range`
(3D, from the entity's position). `first` is the least `path_remaining`; `strongest` the most health (then first);
ties always go to the lowest entity id.

### Actions

`add` / `set` a resource, `change` an attribute (`"change": "self.invested", "add": "..."`), `spawn` (archetype, `at`
a site, path, `self` or `arg.site`, `count`, `set` attributes), `despawn` (removed, no death), `kill` (dies,
`on_death` runs), `damage`, `apply` an effect, `emit` an event, `become` another archetype in place (keeps the id,
position and behaviour state; `keep` lists attributes to carry over), `next_wave`, `goto` a machine state, `outcome`,
`if` / `then` / `else`, and `native`.

### Effects: attributes, modifiers, stacking, tags

- An attribute's **current value** is its base value with the active effects' modifiers applied: all `add`s, then
  `mul`s (each truncating), then the last `set`. Modifier values are evaluated once, when the effect is applied (with
  `magnitude`), so reading an attribute never evaluates expressions.
- **Instant** effects change the base value (damage, healing, gains). **Periodic** effects apply their modifiers to
  the base every `period` (damage over time). **Duration** effects are active while `tick < expires`.
- **Stacking** per target and effect: `strongest` (one instance; a stronger or equal magnitude replaces it and the
  expiry becomes the later of the two: Sepulcra's slow), `refresh` (one instance, timer restarts), `stack` (one
  instance, up to `max_stacks`, modifiers count once per stack), `independent` (separate instances up to
  `max_stacks`, the oldest replaced), `ignore` (a running instance blocks new ones).
- **Tags**: archetype tags plus tags granted by active effects. `block_tags` make a target immune; an attack's
  `blocked_by` stops it firing (stuns); a cooldown is a short effect that grants a tag.

### Events, rules, timers and machines

Built-in events: `spawned`, `died`, `removed`, `arrived`, `damaged`, `hit`, `effect_applied`, `effect_expired`,
`wave_started`, `wave_cleared`, `state_entered`; any other name is a custom event raised by `emit` (the validator
warns about `on` names that nothing emits). Events are queued and dispatched to rules and `on` transitions after
each phase, in the order they happened; a tick that raises more than 100,000 events stops dispatching and reports
`event_storm`.

## Running it: the tick

Integers, ordered containers, ids in creation order and named RNG streams (`specula_core::rng::Rng::named(seed,
stream)`), so the same rules, seed and commands give the same state hash on every tick and every machine. Each tick
runs these phases in this order (fixed and documented, because replays depend on it):

1. **Commands**, in the order given. Each is atomic: arguments resolve, then `cost` and `require` are checked in
   order (the first failure is the rejection, with its code), then `do` runs.
2. **Waves**: start the next wave when its timer runs out. Starting a wave runs `on_start`, then schedules every
   spawn of every group, drawing spawn points from the wave stream as it goes.
3. **Spawns** that are due, by (tick, order scheduled).
4. **Timers** that are due, by item id.
5. **Spawners** (summons), by entity id.
6. **Movement** behaviours, by entity id.
7. **Act** behaviours (attacks), by entity id.
8. **Projectiles**, by entity id.
9. **Effects**: periodic effects, expiries, lifetimes, then machine transitions.
10. **Cleanup**: the dead and despawned are removed; waves whose spawns are all done and gone are cleared.

Entities created during a phase join from the next phase. Outcomes are checked whenever something they could read
changed (after each command, after each entity's step, after each event and at the end of the tick); the first true
one (by `priority`, then id) ends the run on the spot: the rest of that tick is skipped, except cleanup, and every
later command is refused with `game_over`. Death runs `on_death` at once (so drop rolls happen in hit order); the
`died` event reaches rules after the phase.

**The state hash** covers the tick, the outcome, every RNG stream, resources, the wave schedule, pending spawns and
every entity's archetype, position, attributes, effects, behaviour state and machine state, plus the compiled rules'
hash. It is a 64-bit `StableHasher` over a fixed order.

## The escape hatch: native hooks

When no building block fits, a rule set declares a `native` item:

```json
"flow_route": { "kind": "native", "hook": "sepulcra.flow_route", "phase": "move",
                "reason": "ground enemies follow Dijkstra flow fields that change with walls; Lex has no graph navigation yet" }
```

and uses it as a behaviour (`{"type": "native", "hook": "flow_route"}`) or an action (`{"native": "flow_route",
"args": {...}}`). The interface Lex expects from the **module host** lane:

- A host registers `NativeHook` implementations by hook name before a run. A rule set that names a hook nobody
  registered still validates (with a `native_unbound` warning), and running it fails with that name.
- A hook gets a narrow, deterministic `NativeCtx`: the tick, the entity it runs for (read and write attributes,
  position, tags), resources, `spawn` and `despawn`, events, and the named RNG streams. No clock, no I/O, no
  unordered containers: the same calls give the same results.
- Every declared hook carries its `reason`. `lex_validate` lists them, and every run reports how often each was
  called, so the project's escape-hatch log is data: the most-used reasons are the next Lex features.
- A hook's build hash belongs in the journal like any module reload (DETERMINISM.md rule 8).

## Room for Navigation & AI

Movement is a behaviour type, so navigation slots in without changing the language:

- **`navigate`** (the Navigation & AI lane): `{"type": "navigate", "to": "<site or tag>", "speed": ..., "mesh":
  "<navmesh id>"}`, with `path_remaining` filled from the planner, so targeting policies keep working.
- **Behaviour trees and utility AI** become item kinds (`tree`, `utility`) whose leaves are Lex actions and
  conditions; an archetype names one the way it names a `machine`. Machines already gate behaviours by state, which
  is the hook a tree needs ("run this behaviour while this node is active").
- Until then, a game whose movement needs graph search (Sepulcra's flow fields with walls) uses a native hook.

## Tools (the engine side)

Lex documents live in the engine as world entities: the set is a group at `/lex/<set>`, holding the header in its
props, and each item is a child `/lex/<set>/<item id>` with the item in its props (`{"lex": {...}}`). So every
item has a version (`expect_version`), path claims cover a set or one item, every edit is journaled with its author,
`world_undo` undoes it, `world_changes` shows it, and `world_save` writes it to the world's text files. Edits should
go through `lex_edit` (which validates); a `world_set` on the props bypasses validation, and `lex_validate` reports
the result.

| Tool | What it does |
|---|---|
| `lex_schema` | The language as data: item kinds, fields with types and units, behaviours, actions, functions, events. Paged by `topic`. |
| `lex_list` | Rule sets, or one set's items (id, kind, version, a one-line summary), paged and filtered by kind. |
| `lex_get` | Items in full, with versions (at most 50 per call). |
| `lex_edit` | Atomic `put` / `patch` (field paths) / `delete` ops with `expect_version`; the whole set is validated after the edit, and an edit that adds errors is refused with them (`allow_errors` stores a draft). |
| `lex_validate` | Errors and warnings for a set (or an inline document), with the compiled counts and the declared native hooks. |
| `lex_simulate` | Compiles a set (with `params` overrides) and runs it headless for N ticks or until an outcome, from a fresh state or a saved session; applies `commands` at ticks; returns a bounded summary, `expect` assertions, `watch` samples and the state hash. Runs on the calling agent's thread, so many agents simulate in parallel without stalling the window. |
| `lex_import` / `lex_export` | A whole document in or out (canonical text). |

Simulation sessions (`save_as`, then `from`) let an agent run to a point, look, add commands and continue, or branch
what-ifs from one state. A session remembers its rules' hash; continuing it after the rules changed is refused with
`rules_changed`, with the fix (start fresh) in the error.

## Validation: errors that say what to change

Each diagnostic is data:

```json
{ "severity": "error", "code": "unit_mismatch", "item": "warding_1", "path": "behaviours[1].range",
  "message": "a distance is expected here, but 7 has no unit",
  "expected": "distance (e.g. 7m)", "got": "7", "suggestion": "write \"7m\"" }
```

- **Codes** are stable: `syntax`, `unknown_field`, `missing_field`, `wrong_type`, `unknown_ref`, `wrong_kind`,
  `unit_mismatch`, `out_of_range`, `duplicate`, `cycle`, `not_available` (a name used where it doesn't exist),
  `inexact` (warning), `unused` (warning), `native_unbound` (warning), `unsupported_version`.
- **Suggestions**: the closest valid name for every unknown field, reference or function; the exact literal to
  write for unit errors; the valid range for out-of-range values; for expression errors, the column.
- The validator collects every error it can in one pass (it doesn't stop at the first), capped at 200 per call.

## Parameters for optimisation

`param` items are the balance lab's search space: each has a type (unit), a value and a range. `lex_simulate`
takes `params: {"name": value}` overrides, checked against the range, and the compiler folds them in, so a sweep is
"compile with these values, run N seeds". The run summary is the objective's input (win rate, wave reached,
resources over time via `watch`).

## Proof: Sepulcra in Lex (partial, this lane)

`games/sepulcra/lex/slice.lex.json` expresses a slice of Sepulcra in Lex: ground waves from the middle gorge along
the central lane, the warding stone, brazier cairn and rune pillar (bolts, lobs with splash, pulses that slow), three
tiers each, grave-dust income, costs, drops, selling, calling waves early, tombstones and win/lose. The test
`games/sepulcra/tests/lex_parity.rs` runs it next to the hand-written sim on the same content and commands and
compares, tick by tick, dust, tombstones, waves, the outcome and every enemy's position and HP.

What the slice leaves out, and why (each is a later lane or a native hook): flow fields that change with walls
(graph navigation), climbing and range across faces (the slab's faces), lane jumps, flyers' seeded circling,
roots (nature), and the colossus's summons from wherever it stands.

## Later

- A schema-guided three-way merge of items (M4 conflict help) and a text diff in refused edits.
- More stacking and modifier channels as games need them (GAS "aggregators").
- Behaviour trees and utility AI (Navigation & AI lane), and `navigate`.
- Live Lex in the window: the engine stepping a set on its own clock and presenting entities, once the module host
  defines how a game binds presentation to simulation.
