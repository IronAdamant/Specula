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
  overflow saturates; division by zero gives 0 and is counted in the run's diagnostics; nothing panics. A constant
  division by zero is a compile error.
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
| `attack` | act | `targets`, `range`, `cooldown`, `policy` (`first`, `last`, `closest`, `strongest`, `weakest`), `delivery` (`instant`, `pulse`, `projectile`), `projectile`, `muzzle`, `blocked_by` (tags), `payload`. `projectile` is `{archetype, with}`: the projectile archetype and attribute values set on it when fired. |
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

Each action is an object with exactly one verb: `add` / `set` a resource (`{"add": "gold", "amount": 5}`),
`change` an attribute's base (`{"change": "self.invested", "by": "..."}` or `"to"`), `spawn` (archetype or an
archetype expression like `arg.stone`, `at` a path, a site, `arg.site` or an entity such as `self`, `count`, `with`
attribute values), `despawn` (removed, no death), `kill` (dies, `on_death` runs), `damage`, `apply` an effect
(`magnitude`, `duration`), `emit` an event, `become` another archetype in place (keeps the id, position, effects and
the state of behaviours with the same name and type; `keep` lists attributes to carry over), `next_wave`, `goto` a
machine state, `outcome`, `if` / `then` / `else`, and `native`. An entity spawned at another entity that follows a
path continues that path from where it stands (summons), and joins its wave.

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
refuses `on` names that nothing emits, with the closest name). Events are queued and dispatched to rules and `on`
transitions at the end of each phase (and after each command), in the order they happened; events raised by rules
are dispatched in the same pass. A tick that raises more than 100,000 events stops dispatching and reports
`event_storm`, so a rule that re-emits its own event can't hang a run.

Rules see `self` (the event's subject), `other` (the instigator: the killer, the attacker, the effect's source)
and `event.value`. Timers fire at `start`, then every `every`, at most `count` times, skipping firings while
`when` is false. A machine's transitions are polled in the effects phase (the first whose `after` and `when` both
hold fires; `on` transitions fire when that event reaches the entity); `on_exit`, then `on_enter`, run on a switch.

## Running it: the tick

Integers, ordered containers, ids in creation order and named RNG streams (`specula_core::rng::Rng::named(seed,
stream)`), so the same rules, seed and commands give the same state hash on every tick and every machine. Each tick
runs these phases in this order (fixed and documented, because replays depend on it):

1. **Commands**, in the order given. Each is atomic: arguments resolve, then `cost` and `require` are checked in
   order (the first failure is the rejection, with its code), then `do` runs. If an action fails at run time (a
   missing entity, say), the whole command is rolled back and refused with `runtime_error`.
2. **Waves**: start the next wave when its timer runs out. Starting a wave runs `on_start`, then schedules every
   spawn of every group, drawing spawn points from the wave stream as it goes.
3. **Spawns** that are due, by (tick, order scheduled).
4. **Timers** that are due, by item id.
5. **Spawners** (summons), by entity id.
6. **Movement** behaviours, by entity id.
7. **Act** behaviours (attacks), by entity id.
8. **Projectiles**, by entity id.
9. **Effects**: native behaviours declared for this phase, then per entity: periodic effects, expiries, lifetimes,
   machine transitions.
10. **Cleanup**: events are dispatched, the dead and despawned are removed, waves whose spawns are all done and gone
    are cleared.

Entities created during a phase join from the next phase. **Outcomes** are checked whenever something they could
read changed: those that read only resources, waves and the tick right after each entity's step, so a loss stops
the tick right after the step that caused it; those that scan entities (`count`, `occupied`) at the end of each
phase, so they can't make a tick quadratic in the number of entities. The first true one (by `priority`, then id)
ends the run: the rest of that tick is skipped, except cleanup, and every later command is refused with
`game_over`. Death runs `on_death` at once (so drop rolls happen in hit order); the `died` event reaches rules at
the end of the phase, while the dead entity can still be read.

Run-time problems never panic and never stop a run: a division by zero gives 0, a missing entity or attribute
aborts the action list it happened in, and each is counted under `runtime_diagnostics` (with the first 20 messages)
in the run's report. Limits: 100,000 live entities, 10,000 spawns per action.

**Cost.** The Sepulcra slice runs at about 1 µs per tick in a release build, ten times the hand-written Rust sim
(0.1 µs): a 12-minute game in about 21 ms, so a 1000-seed sweep is about 20 core-seconds
(`cargo test -p sepulcra --release --test lex_parity -- --ignored --nocapture`).

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

- A host registers `NativeHook` implementations by hook name before a run (`specula_lex::Natives`, a map from
  hook name to `Arc<dyn NativeHook>`, passed to `Sim::new`). A rule set that names a hook nobody registered still
  validates (with a `native_unbound` warning), and starting a run fails with that hook's name.
- `NativeHook::run(&self, cx: &mut NativeCtx) -> Result<(), String>` (an error is counted in the run's
  diagnostics, like any run-time problem). `NativeCtx` is narrow and deterministic: `tick`, `tick_hz`, `entity`
  (the entity it runs for), `arg(name)`, `attr` / `set_attr`, `pos` / `set_pos`, `set_path_remaining` (so targeting
  policies keep working for native movement), `has_tag`, `with_tag` (ids in order), `resource` / `add_resource`,
  `rand(stream, lo, hi)` on the rules' named streams, `spawn`, `despawn`, `kill` and `emit`. No clock, no I/O, no
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
`world_undo` undoes it, `world_changes` shows it, and `world_save` writes it to the world's text files (the `/lex`
subtree gets its own file, one line per item in path order, so rule changes diff cleanly). Edits should go through
`lex_edit` (which validates); a `world_set` on the props bypasses validation, and `lex_validate` reports the result.
The world hash covers the rule sets, so a replay that changes the rules diverges where it should.

| Tool | What it does |
|---|---|
| `lex_schema` | The language as data: item kinds, fields with types and units, behaviours, actions, payloads, functions, events, units and the tick's phases, by `topic`. |
| `lex_list` | Rule sets, or one set's items (id, kind, version, a one-line summary), paged and filtered by kind. |
| `lex_get` | Items in full, with versions (at most 50 per call); unknown ids come back with the closest id. |
| `lex_edit` | Atomic `put` / `patch` (field paths like `behaviours[0].range`) / `delete` ops with `expect_version` (0: must not exist yet); creates the set when needed. The whole set is validated after the edit, and an edit that adds errors is refused with every new error (`allow_errors` stores a draft; errors already there don't block fixing others). `dry_run` only reports. |
| `lex_validate` | Every error and warning for a set (or an inline document), the items by kind, the params (the tunable search space) and the declared native hooks with their reasons. |
| `lex_simulate` | Compiles a set (with `params` overrides) and runs it headless for up to N ticks or until an outcome, from a fresh state or a saved run; applies `commands` at ticks; returns a bounded report: outcome, resources, waves, entities by archetype, event counts, refusals, run-time diagnostics, native calls, `expect` results, `watch` samples (at most 200) and the state hash. Runs on the calling agent's protocol thread, so many agents simulate in parallel without stalling the window; a call stops after 20 s of wall time and says at which tick (continue with `save_as` / `from`). |
| `lex_import` / `lex_export` | A whole document in (one validated, journaled edit; `replace` deletes items it doesn't have) or out (canonical text to save as `*.lex.json`). |
| `lex_session` | Saved runs: list them, inspect one's entities (paged; attributes in human units, effects, machine state) or delete one. |

Saved runs (`save_as`, then `from`) let an agent run to a point, look, add commands and continue, or branch
what-ifs from one state. A run remembers its rules' hash; continuing it after the rules changed is refused with
`rules_changed`, with the fix in the error. 16 runs are kept engine-wide (oldest dropped); only the session that
saved a run can overwrite or delete it.

## Validation: errors that say what to change

Each diagnostic is data:

```json
{ "severity": "error", "code": "unit_mismatch", "item": "warding_1", "path": "behaviours[1].range",
  "message": "a distance is expected here, but 7 has no unit",
  "expected": "distance (e.g. 7m)", "got": "7", "suggestion": "write \"7m\"" }
```

- **Codes** are stable: `syntax`, `unknown_field`, `missing_field`, `wrong_type`, `unknown_ref` (a warning for a
  tag nothing has), `wrong_kind`, `unit_mismatch`, `out_of_range`, `duplicate`, `cycle`, `not_available` (a name
  used where it doesn't exist), `invalid`, `inexact` (warning), `native_unbound` (warning),
  `unsupported_version`, `too_many`.
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
`games/sepulcra/tests/lex_parity.rs` runs it next to the hand-written sim on the same content
(`lex/slice_waves.json` with the built-in slab, stones and enemies) and commands, and compares every tick: dust,
tombstones, the wave, the outcome, which commands were refused, and every enemy's and projectile's position (to
the millimetre) and every enemy's HP (to the milli-HP). **Result (2026-10-17): identical on every tick** of full
runs for five seeds (all won, some with leaks: the boss-free slice leaks shamblers and runners on some seeds), and
for undefended runs that lose; a one-point change to one rule (warding damage 10 → 11) is caught. The scenario
places, is refused for lack of dust, upgrades all three stones, calls waves early, sells and places again.

How the slice had to be written to match (the honest list):

- **Projectile speeds** are the hand-written sim's whole millimetres per tick: it truncates 20 m/s to 666 mm a
  tick, Lex carries the remainder like every movement, so the slice says `19.98m/s`.
- **Firing order**: the hand-written sim fires stones in cell order, Lex in entity (creation) order; the scenario
  places stones in increasing cell order, so the orders agree. A full port either keeps that rule or gives Lex a
  sort key per archetype.
- **Rejection codes** differ where the two models differ: an upgrade with no stone on the cell is `no_stone` there
  and `no_entity` here (the test compares which commands were refused, not their codes).
- **Wave bookkeeping**: Lex counts `died` and `removed`; the hand-written sim's leaks are Lex's removals.

What the slice leaves out, and why (each is a later lane or a native hook): flow fields that change with walls
(graph navigation: the Navigation & AI lane, or a `sepulcra.flow_route` native until then), climbing and range
across faces (the slab's faces need a face-aware range check: a native or a later `range` option), lane jumps
(graph navigation), flyers' seeded circling (a path generated from a seeded choice: a later path feature or a
native), roots (nature: an aura effect on cells, needs cells as data), walls, and the colossus's summons (the
`spawner` behaviour continues the summoner's path, which covers it once movement is on the graph).

## Later

- A schema-guided three-way merge of items (M4 conflict help) and a text diff in refused edits.
- More stacking and modifier channels as games need them (GAS "aggregators").
- Behaviour trees and utility AI (Navigation & AI lane), and `navigate`.
- Live Lex in the window: the engine stepping a set on its own clock and presenting entities, once the module host
  defines how a game binds presentation to simulation.
