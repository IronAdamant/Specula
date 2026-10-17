# Design: Lex, gameplay as data

Status: designed and built 2026-10-17 (M3, the Lex lane): the `specula-lex` crate (parse, validate, compile, a
deterministic runtime) and the engine's `lex_*` tools. Grown 2026-10-18 by the whole-Sepulcra port (the building
blocks marked *(Sepulcra port)* below, [Grown from Sepulcra](#grown-from-sepulcra-2026-10-18)) and live Lex in the
engine ([Live Lex](#live-lex-lex_live)). Foundations **F1** (swarm-first) and **F2** (determinism).
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
- `notes` (string) is allowed on the document, every item and every behaviour, action and nested object the schema
  defines, and never affects the rules hash.

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
  Members: an entity's attributes and `alive`, `archetype`, `next`, `path_remaining`, `x`, `y`, `z` (its position);
  an archetype's base attributes and `next`; a site's `x`, `y`, `z` (`arg.site.x`) *(Sepulcra port)*. `x`, `y` and
  `z` are reserved: no attribute may be named so.
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
| `sites` | Named positions (`points: {"top:8,16": [x, y, z]}`): build spots and spawn points. Several items merge; a command's site arg can be limited to one item (`{"type": "site", "of": "build_spots"}`, else `wrong_site`) *(Sepulcra port)*. |
| `archetype` | An entity template: `tags`, `attributes`, `behaviours`, optional `machine`, `next` (upgrade chain), `extends`, `abstract`, `on_spawn`, `order` (an int expression evaluated once, after `on_spawn`: behaviour phases then run entities by (order, id), e.g. towers firing in grid order) *(Sepulcra port)*. |
| `effect` | GAS-style: `duration` (`instant`, a duration or `infinite`), `period`, `modifiers`, `stacking`, `max_stacks`, `grant_tags`, `block_tags`, `on_apply`, `on_period`, `on_expire`. |
| `machine` | A state machine: `initial`, `states` with `behaviours` (which are active), `on_enter`, `on_exit`, `transitions` (`when`, `after`, `on`). |
| `waves` | An encounter schedule: `start`, `rng` stream, `waves` with `duration`, `on_start`, `scale` (attribute ratios for spawns) and `groups` (`archetype`, `count`, `at`, `start`, `every`, `with`). A group's `with` sets attributes per spawn, evaluated when the wave is scheduled, right after that spawn's point is drawn, in attribute-name order (so `rand(waves, ...)` draws interleave with the point draws the same way in every run, whatever the key order) *(Sepulcra port)*. Several waves items join in id order into one schedule (`waves_01`, `waves_02`, ...), so a long schedule fits the 16 KB item limit; only the first sets `start` and `rng`. |
| `timer` | `start`, `every`, `count`, `when`, `do`. |
| `rule` | A trigger: `on` an event, filtered by `tags` and `when`, `do` actions. |
| `command` | Player (or agent) input as data: typed `args`, `cost`, `require` (each with a rejection `code`), `do`. |
| `outcome` | `result` (`won`, `lost` or any name), `when`, `priority`. The first true outcome ends the run. |
| `native` | A declared escape hatch: `hook` (the native function's name), `reason` (required), `phase`, `streams` (the random streams it draws from) and `data` (the data items it reads) *(Sepulcra port)*. |
| `data` | A table for native hooks: any JSON in `value` (a map, a curve), at most 16 KB like every item. The rules' hash covers it *(Sepulcra port)*. |

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
| `attack` | act | `targets`, `when`, `range`, `cooldown`, `policy` (`first`, `last`, `closest`, `strongest`, `weakest`), `delivery` (`instant`, `pulse`, `projectile`), `projectile`, `muzzle`, `blocked_by` (tags), `payload`. `projectile` is `{archetype, with}`: the projectile archetype and attribute values set on it when fired. `when` is an extra test per candidate (`self` the attacker, `target` the candidate), e.g. Sepulcra's range across faces; splash ignores it *(Sepulcra port)*. |
| `projectile` | projectiles | `speed`, `homing`, `splash`. Carries the attack's payload, evaluated when fired. |
| `spawner` | spawn | `archetype`, `every`, `count`, `start`, `with` (attribute values for each summon, evaluated for the spawner) *(Sepulcra port)*. |
| `lifetime` | effects | `duration`, `on_expire`. |
| `native` | its declared phase | `hook`, `args`, `on` (named action lists the hook runs with `trigger`, `self` the entity: e.g. `on.arrive` costs a life) *(Sepulcra port)*. |

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
path continues that path from where it stands (summons), and joins its wave. **Any spawn that joins a wave gets
that wave's `scale`**, whoever spawned it (the schedule, a summoner, a rule): a boss's summons are as tough as the
wave around them *(Sepulcra port; before, only scheduled spawns were scaled)*.

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
are dispatched in the same pass. A tick keeps and dispatches at most 100,000 events; later ones are only
counted (`event_overflow`), so a rule that re-emits its own event can't hang or flood a run.

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

When some archetype declares `order`, phases 5 to 7 (and natives declared for the effects phase) run entities by
(order, id) instead of id; the per-entity effects, lifetimes and machines of phase 9 stay in id order.

Entities created during a phase join from the next phase. **Outcomes** are checked whenever something they could
read changed: those that read only resources, waves and the tick right after each entity's step, so a loss stops
the tick right after the step that caused it; those that scan entities (`count`, `occupied`) at the end of each
phase, so they can't make a tick quadratic in the number of entities. The first true one (by `priority`, then id)
ends the run: the rest of that tick is skipped, except cleanup, and every later command is refused with
`game_over`. Death runs `on_death` at once (so drop rolls happen in hit order); the `died` event reaches rules at
the end of the phase, while the dead entity can still be read.

Run-time problems never stop a run: arithmetic saturates, a division by zero gives 0, a missing entity or attribute
aborts the action list it happened in, and each is counted under `runtime_diagnostics` (with the first 20 messages)
in the run's report.

**Limits**, so no rule set (and no agent's typo) can crash the engine or make a call unbounded. Each is reported
as data when it bites:

| Limit | Value | When exceeded |
|---|---|---|
| Live entities | 100,000 | the spawn fails (`entity_limit`) |
| Spawns per action | 10,000 | clamped |
| Spawns one wave schedules | 100,000 | compile error (`too_many`) |
| Spawns waiting in the wave queue | 200,000 | a wave that would pass it starts without its groups (`spawn_limit`) |
| Nested action lists (on_spawn spawning itself, on_apply reapplying, on_enter re-entering) | 32 | that list stops (`recursion_limit`) |
| Work per tick: actions run plus entity, tag and effect checks (attack scans, splash, chase, effect applications, `count`, `occupied`); commands have a separate budget of the same size | 5,000,000 each | phases: the rest of the tick's behaviours, rules and effects are skipped and `count` / `occupied` stop; commands: the rest are refused, and the tick's phases still run in full (`work_limit`) |
| Events kept and dispatched per tick | 100,000 | counted only (`event_overflow`) |
| Expression length / nesting | 1,000 characters / 64 levels | compile error (`too_many`) |
| Coordinates | ±10⁹ m | compile error (`out_of_range`) |
| Sites, tags, attributes, RNG streams | 60,000 each | compile error (`too_many`) |
| Items per set | 10,000 | compile error |
| Item size (compact JSON) | 16,000 bytes (an item is one world entity) | compile error (`too_large`), so a set that validates can always be stored |
| Run state: one cell per entity plus one per attribute and behaviour, and a few per effect instance | 2,000,000 cells | the spawn or the effect fails (`state_limit`) |
| Effect instances per entity / modifiers per effect / `max_stacks` | 8 / 8 / 8 | the effect is skipped (`effect_limit`) / compile error |
| Tags per filter, grant_tags, block_tags, blocked_by | 16 | compile error (`too_many`) |
| Attributes / behaviours per archetype, payload entries | 64 / 32 / 16 | compile error (`too_many`) |
| Waves / distinct events | 10,000 / 60,000 | compile error (`too_many`) |

A negative cost is refused (`invalid_cost`) rather than paying the player. A command's rollback snapshots the
state, so a command is charged its state size to the commands' own work budget: once it is spent, the tick's
remaining commands are refused with `work_limit` (send them next tick), and the tick's phases still run with their
whole budget. Tag checks are charged by the tags and effects they
scan. So the per-tick work limit, the state budget and the size caps together bound both one call's time and one
run's memory (with 64 attributes per entity a run stops spawning near 30,000 entities, about 30 MB).

The wall clock is read as work is spent, so a deadline interrupts a tick part-way: the run is then **incomplete**
(`complete: false`): no hash and no outcome are reported, `expect` is not judged and the run can't be saved, since
where it stopped depends on the machine.

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
- *(Sepulcra port)* **Binding.** `NativeHook::bind(&self, rules, native)` runs once per run (`Sim::new`): a hook
  resolves attribute, tag and stream names to ids (`Rules::attr`, `tag`, `stream`) and reads its `data` items there,
  and returns the bound hook (or refuses the run with its reason). Hooks that run per entity per tick use the id
  API: `get` / `base` / `base_many` / `set` / `set_many` (one lookup for many attributes), `has`, `alive`,
  `position`, `each_tagged` (no allocation), `range` / `chance` on a declared stream, `wave`.
- *(Sepulcra port)* **Triggers and refusals.** `cx.trigger("arrive")` runs the behaviour's `on.arrive` actions at
  once (so a hook decides *when*, the rules say *what*: tombstones stay data). In a command, `return
  Err(cx.refuse("enemy_in_the_way", "..."))` rolls the command back and refuses it with that code.
- *(Sepulcra port)* **Caching.** A bound hook is shared by a run's clones, so what it derives from the state (flow
  fields from walls) is keyed by `cx.generation()`: a pair (the state value's identity, a counter bumped when an
  entity appears, leaves, changes archetype or gains or loses an effect instance). Every clone, including a
  command's rollback snapshot, is a new identity, so a key can never match two different states. Positions and
  attribute values are not covered.
- *(Sepulcra port)* **Hosts.** `Sim::new_hosted` leaves hooks without an in-process implementation to a
  `NativeHost` passed to `step_hosted` / `apply_hosted` (live Lex calls module hooks through it, synchronously, in
  rule order). A host can't touch the run's state.
- A hook's build hash belongs in the journal like any module reload (DETERMINISM.md rule 8).

**With the module host (docs/design/MODULES.md, merged).** Discovery is wired: every Lex snapshot carries
`LogicHost::hooks()` (`module/name`), so `lex_edit` and `lex_validate` warn with `native_unbound` naming the
running hooks when a set's `native` names one no module provides, and with `native_headless` when a module does
provide it (so the agent knows `lex_simulate` will still refuse the set). The
Calls are **not** made from
`lex_simulate`, on purpose: a module hook runs against the shared engine World (its Cx reads and writes world
entities), while `lex_simulate` is headless, on a protocol thread, over Lex's own integer state. Calling hooks
there would write a what-if run's effects into the real world and make runs depend on world state outside the rule
set. They are made by **live Lex** ([below](#live-lex-lex_live)): there the run's `NativeHost` calls
`call_hook(hook, args, reason, "lex:<set>", world, tick, hz, commit)` with a commit that journals the changes as
the run's tick (MODULES.md item 6). Headless runs use natives registered in-process: the hosted game's
(`Game::lex_natives`, e.g. Sepulcra's), which `lex_simulate`, `lex_live` and validation all see (no
`native_unbound` / `native_headless` warning for them).

## Room for Navigation & AI

Movement is a behaviour type, so navigation slots in without changing the language:

- **`navigate`** (the Navigation & AI lane): `{"type": "navigate", "to": "<site or tag>", "speed": ..., "mesh":
  "<navmesh id>"}`, with `path_remaining` filled from the planner, so targeting policies keep working.
- **Behaviour trees and utility AI** become item kinds (`tree`, `utility`) whose leaves are Lex actions and
  conditions; an archetype names one the way it names a `machine`. Machines already gate behaviours by state, which
  is the hook a tree needs ("run this behaviour while this node is active").
- Until then, a game whose movement needs graph search (Sepulcra's flow fields with walls) uses a native hook:
  Sepulcra's `sepulcra.ground` keeps its state in plain attributes (`c_from`, `c_to`, `prog`, `elen`, ...) and sets
  `path_remaining`, so a `navigate` behaviour can replace it without changing anything else in the rule set.

## Tools (the engine side)

Lex documents live in the engine as world entities: the set is a group at `/lex/<set>`, holding the header in its
props, and each item is a child `/lex/<set>/<item id>` with the item in its props (`{"lex": {...}}`). So every
item has a version (`expect_version`), path claims cover a set or one item (region claims don't: rule data has
no place in the world, so a claim around the origin never blocks a rule edit). `/lex` is reserved for the Lex
tools: `world_spawn`, `world_set`, `world_delete` and `world_batch` refuse anything that would put an entity in
`/lex`, move one out, or touch rule data (`reserved`, naming the lex tools), so nothing can hide there from a
region claim; `world_undo` still undoes Lex edits. every edit is journaled with its author,
`world_undo` undoes it, `world_changes` shows it, and `world_save` writes it to the world's text files (the `/lex`
subtree gets its own file, one line per item in path order, so rule changes diff cleanly). Edits should go through
`lex_edit` (which validates); a `world_set` on the props bypasses validation, and `lex_validate` reports the result.
The world hash covers the rule sets, so a replay that changes the rules diverges where it should.

**Threads.** The main thread only reads (snapshots, pages of summaries) and writes (one atomic batch through the
claims and the journal). Everything costly runs on the calling agent's protocol thread: an edit takes a snapshot,
applies its ops, validates the set before and after (uncapped, so a set with hundreds of errors can still be fixed
one at a time), and sends a commit that carries the entity id and version of every item it validated against.
Edits to one set queue behind a per-set lock (held on protocol threads only, never by the main thread; an edit
that waits more than 10 s answers `busy` with nothing applied, inside the caller's timeout), so agents
editing different items of a set don't fail each other: 8 agents × 5 edits to their own items of a 500-item set
all succeed. If something outside `lex_edit` changed the set meanwhile (`world_set`, `world_undo`), or an item
was deleted and put again (a new entity, caught by its id), the commit is refused and the edit is redone on the
new state (four tries, then `busy`). Measured on a 2,000-archetype set in a debug build: an edit takes about
0.3 s on its own thread while `world_list` from another agent answers in 8 ms (median, 42 ms worst).

**Damaged sets.** A set changed outside `lex_edit` (an item with an invalid id, a broken header) still lists,
validates (its load problems lead the diagnostics) and edits (a badly named item can be deleted); `lex_simulate`
and `lex_export` refuse it until it is fixed, so nothing runs it at the wrong tick rate or without some items.
`notes` is ignored by the rules hash only where the schema puts notes: an attribute, site, state or arg the author
named `notes` is data.

| Tool | What it does |
|---|---|
| `lex_schema` | The language as data: item kinds, fields with types and units, behaviours, actions, payloads, functions, events, units and the tick's phases, by `topic`. |
| `lex_list` | Rule sets, or one set's items (id, kind, version, a one-line summary), paged and filtered by kind. |
| `lex_get` | Items in full, with versions (at most 50 per call); unknown ids come back with the closest id. |
| `lex_edit` | Atomic `put` / `patch` (field paths like `behaviours[0].range`) / `delete` ops with `expect_version` (0: must not exist yet); creates the set when needed. The whole set is validated after the edit, and an edit that adds errors is refused with every new error (`allow_errors` stores a draft; errors already there don't block fixing others). `dry_run` only reports. |
| `lex_validate` | Every error and warning for a set (or an inline document), the items by kind, the params (the tunable search space) and the declared native hooks with their reasons. |
| `lex_simulate` | Compiles a set (with `params` overrides) and runs it headless for up to N ticks or until an outcome, from a fresh state or a saved run; applies `commands` at ticks; returns a bounded report: outcome, resources, waves, entities by archetype, event counts, refusals, run-time diagnostics, native calls, `expect` results, `watch` samples (at most 200) and the state hash. Runs on the calling agent's protocol thread, so many agents simulate in parallel without stalling the window; the wall clock is read as work is spent, and a call that passes 20 s stops part-way and comes back `complete: false`, without a hash or an outcome, and can't be saved: run fewer ticks per call, splitting a long run into complete calls joined with `save_as` / `from`. |
| `lex_import` / `lex_export` | A whole document in (one validated, journaled edit; `replace` deletes items it doesn't have) or out (canonical text to save as `*.lex.json`). |
| `lex_session` | Saved runs: list them, inspect one's entities (paged; attributes in human units, effects, machine state) or delete one. |
| `lex_live` | Runs a stored set live in the engine (see [Live Lex](#live-lex-lex_live)): `start`, `command`, `status`, `journal`, `stop`. |

Saved runs (`save_as`, then `from`) let an agent run to a point, look, add commands and continue, or branch
what-ifs from one state. A run remembers its rules' hash; continuing it after the rules changed is refused with
`rules_changed`, with the fix in the error. At most 4 runs per agent session (a session's own oldest run makes
room) and 16 in all are kept, and at most
8 million state cells across them: a save never evicts another session's run, and is refused with `runs_full`
when only others' runs are in the way; `lex_session inspect` pages are capped (32 attributes
per entity listed, about 200 KB per page) and built outside the shared list's lock. Only the session that
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

## Proof: Sepulcra in Lex

**The slice (2026-10-17).** `games/sepulcra/lex/slice.lex.json` expresses one gorge, one lane and three stones; the
test `lex_parity.rs` runs it next to the hand-written sim and matches every tick (kept as the smallest example).

**The whole game (2026-10-18).** `games/sepulcra/lex/sepulcra.lex.json` (the rules, written by hand) plus
`lex/slab_01.lex.json` (the level: the slab as a `data` item and the sites the rules name, generated from
`data/slab_01.json` by `examples/lex_slab.rs`; a test keeps them in step) are all of Sepulcra: every layout, the four
stones in three tiers (attributes per tier, one attack per stone kind), roots from wave 5 on tier II+ stones, walls
(placing, refusing a wall under an enemy, breaking them when walled in, removing them with an HP-scaled refund),
climbing, range across faces (each ground stone's attack `when`), lane jumps, wraiths circling the ring, the colossus
and its summons, ten waves with HP scales and incomes, grave-dust, drops, selling, calling waves early,
tombstones, win and lose. `tests/lex_parity.rs` runs it next to the hand-written sim on the same content and
commands and compares **every tick**: dust, tombstones, wave, outcome, which commands were refused, every enemy's
position (mm) and HP (milli-HP), every projectile's position, every stone's cell and tier, every wall's cell and HP.

**Results (2026-10-18), identical on every tick:**

- every built-in layout (centre-heavy, no anti-air, full block, none) on seeds 1 and 7 (in `cargo test`);
- a "chaos" player that tries everything, often wrongly (stones on lanes, the graveyard and cliff rock, upgrades
  past the last tier, selling nothing, walls under enemies' feet and on cells they are stepping into, walls
  everywhere, removing walls, calling waves), on 6 seeds with 20 to 60 refusals each, and with 4,000 starting dust
  on 4 seeds so whole games play out: walls broken (27 in all), walls attacked (191), lane jumps (13) and summons
  (10) all happen, still identical;
- the release sweep (`LEX_SWEEP_SEEDS=1000 cargo test -p sepulcra --release --test lex_parity -- --ignored
  sweep --nocapture`): **4,100 whole games, 46.2 million ticks, in 74 s on 32 threads**: centre-heavy won 999 of
  1,000 (as DESIGN.md's balance table says), no anti-air, full block and no defence lost all 1,000, and 100 rich
  chaos games (95 lost, 5 won).

**Through the tools alone.** `scripts/sepulcra_lex_tools.py` starts the game headless and, over MCP with the lex
tools only, imports the slab, writes the rules item by item with `lex_edit` (42 edits; the set is a draft until
the last lands, then valid with no warnings), checks the export equals the two documents, and simulates the games
in `lex/proof.json` (centre-heavy seed 7 won at tick 21852, no anti-air seed 1 lost at 11860, full block seed 2
lost at 6882) to their recorded hashes. Changing warding damage 10 → 11 changes the game; `world_undo` brings the
hash back; a notes edit changes nothing. Then it plays the centre-heavy game **live** (`lex_live`, commands at
their ticks under `time_control step`): the live run ends at the same tick with the same hash, its journal replays
headless to that hash, and its entities are world entities.

**Cost.** 8 µs a tick for the whole game in a release build (0.5 µs for the hand-written sim, ×15): a 12-minute
game in about 170 ms (`cargo test -p sepulcra --release --test lex_parity -- --ignored bench_full --nocapture`).
The movement hooks read and write each enemy's state in one lookup (`base_many` / `set_many`) and rebuild flow
fields only when the walls change.

**The escape-hatch log** (each native's `reason` is in the set; `lex_validate` lists them):

| Native | Phase | Why no building block fits |
|---|---|---|
| `sepulcra.ground` | move | Dijkstra flow fields on the slab's cell graph that change with walls; breaking the wall on the cheapest route when none is open; climbing speed; roots; lane jumps by a seeded chance. Lex has no graph navigation yet (the navigation lane's `navigate`). |
| `sepulcra.air` | move | Flyers circle the air ring a drawn number of hops in a drawn direction, then dive: a path generated from seeded choices. |
| `sepulcra.spot` | (action) | A stone's cell (it fires in cell order), face and rim distances: facts of the slab's geometry, which sites don't carry. The range rule itself is Lex (`when`). |
| `sepulcra.wall_clear` | (action) | "No wall under an enemy" is a question about the movement graph's edges, which only the movement hook knows. |

What each says Lex should grow next: graph navigation (with walls as costs, breaching and jumps as edge kinds),
generated paths (a ring with a drawn start, direction and length), and sites that carry values (a cell id, a
face), which would retire `spot`.

**Unavoidable differences (the honest list):**

- **Projectile speeds** are the hand-written sim's whole millimetres per tick (it truncates 22 m/s to 733 mm a
  tick; Lex carries the remainder like every movement, so the set says `21.99m/s`).
- **Refusal codes** differ where the models differ (the tests compare *which* commands are refused): a stone on a
  lane is `wrong_site` (the arg is limited to `build_spots`) where the sim says `not_buildable`; an upgrade of an
  empty cell is `no_entity` there, `no_stone` here; Lex checks a command's `cost` before its `require`s, so a poor
  player placing on an occupied spot hears `not_enough_dust` first.
- **Bookkeeping**: Lex's waves count `died` and `removed`; the sim's leaks and strandings are both removals.
- **State hashes** differ (each hashes its own layout); parity is judged on everything observable, listed above.
- **Events**: presentation events (a wall's quarter marks, a leap, "attacking a wall") are not Lex events; the hooks
  could `emit` them when a presentation needs them.
- **Stone order** needs `order: "self.cell"` (stones fire in cell order in the sim, in id order by default in Lex).

### Grown from Sepulcra (2026-10-18)

Each feature is general, validated, bounded and tested (`crates/specula-lex/tests/extensions.rs`): `data` items;
native `streams`, `data`, binding, the id API, `trigger` with a native behaviour's `on` lists, `refuse`,
`generation`, hosts; attack `when`; archetype `order`; group `with`; spawner `with`; wave scale for every spawn
that joins a wave; site args limited to one sites item (`of`); `x`, `y`, `z` of entities and sites. Native args are
capped at 16 (evaluated into a fixed buffer, no allocation per call). `RUNTIME_VERSION` stays 1: the only result
change is the wave scale of spawns that join a wave without being scheduled (summons), which no rule set used before.

## Live Lex (`lex_live`)

The engine runs a stored set live (`crates/specula/src/lex_live.rs`): compiled on the caller's thread, then stepped
on the main thread every engine tick, after the hosted game and the modules, by as many Lex ticks as its own
`tick_hz` has accumulated (a 30 Hz set under a 60 Hz engine steps every other tick), so `time_control` pauses, steps
and fast-forwards it with everything else. At most 4 runs at once.

- **Presentation.** Each live entity matching a `present` rule (`{tags, spawn}`, first match wins; default: every
  entity as a 0.5 m cube) is a world entity `/lex_live/<set>/e<id>` (props `lex_live: {id, archetype}`), moved to
  the entity's position each tick and deleted when it ends; at most 2,000 per run (the rest are counted, not
  shown). These writes are the rules' output, like a hosted game's: journaled replay-only under `lex:<set>`,
  coalesced per tick, exempt from claims. Starting and stopping are the caller's edits (claim-checked, undoable).
- **Commands** (`action: command`) apply at once at the run's current tick, exactly as `lex_simulate` applies a
  command stamped with that tick, and are journaled with it. `journal` pages them as `lex_simulate` commands, so
  `lex_simulate {seed, params, ticks, commands}` reproduces the live hash.
- **Natives.** Hooks the hosted game implements run in-process; any other declared hook is a module hook, called
  through the module host with the native's `reason` and caller `lex:<set>`; its world changes are journaled as the
  run's tick. A failing hook is a run-time diagnostic (in a command: a refusal), counted in `status`.
- **Live = headless.** A module hook can't change the run's state, only the world, so with the same seed, params and
  commands at the same ticks, a live run and `lex_simulate` reach the same hash (tested in `lex_live.rs`, and on a
  whole game of Sepulcra by `scripts/sepulcra_lex_tools.py`).
- **Input**: a hosted game turns its input into the set's commands (Sepulcra's `--rules lex`, below); generic input
  bindings are later work.

**Sepulcra on Lex in the window.** `sepulcra --rules lex` (the app, after `--`) runs the game's rules from the
set: the Lex run decides every player and agent command and the outcome, while the hand-written sim steps in
lockstep as the reference and feeds the scene, HUD and sound; every tick the two are compared (a difference is a
desync, shown in `play_state` with its tick). `play_record` adds the Lex hash at the recording's end and
`play_replay` replays the recording through Lex too. `python scripts/sepulcra_play.py --adapter integrated --rules
lex` plays a whole game to a win on the Intel iGPU with parity on every tick, the replay verified and the Lex hash
replayed (2026-10-18: won at tick 21852, 21,852 ticks compared, 0 validation errors).

## Later

- A schema-guided three-way merge of items (M4 conflict help) and a text diff in refused edits.
- More stacking and modifier channels as games need them (GAS "aggregators").
- Behaviour trees and utility AI (Navigation & AI lane), and `navigate`.
- Input bindings for live runs (window and gamepad input mapped to a set's commands without a hosted game), and
  presenting a live run with the game's own looks instead of `present` rules.
- A cell-graph `navigate` behaviour (the navigation lane) to replace `sepulcra.ground` and `sepulcra.air`.
