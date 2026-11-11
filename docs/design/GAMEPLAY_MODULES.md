# Design: reusable gameplay modules (inventory, abilities, dialogue, quests)

Status: built 2026-11-07 (M5 lane "gameplay modules"). Builds on [MODULES.md](MODULES.md) (the module host, the SDK,
the C ABI) and follows [DETERMINISM.md](DETERMINISM.md). Proof: `scripts/gmods_check.py`.

**Goal:** four gameplay systems most games need, written as ordinary SDK modules an agent can read, copy and change:
an inventory, abilities, dialogue and quests. Each is data first (a JSON file in the module's folder), keeps all its
state in the world, and talks to the others only through the world and events, so any one can be swapped out. A demo
module builds a small scene that uses all four.

| Module (folder) | What it does | Data | Its state in the world |
|---|---|---|---|
| `inventarium` (games/inventarium) | stacks, slots and weight; give, take, move, pick up | `items.json` | `props.inventory` on any entity |
| `facultas` (games/facultas) | abilities: cooldowns, resource costs, required items, effects (vfx, sound, damage, status, events) | `abilities.json` | `props.abilities`, `props.status` on the caster; effect entities under `/facultas` |
| `colloquium` (games/colloquium) | dialogue graphs: nodes, conditional choices, effects, localisable lines | `dialogue.json` | `props.colloquium` on the actor; line texts under `/colloquium` |
| `quaestio` (games/quaestio) | quests: objectives that count items, events and signals; rewards | `quests.json` | `props.quests` on the actor; a tracker text under `/quaestio` |
| `exemplum` (games/exemplum) | the demo: a graveyard yard with the Sexton, bones and wisps | `scene.json` | everything under `/exemplum` |

Latin names, as the owner likes: *inventarium* (inventory), *facultas* (ability), *colloquium* (conversation),
*quaestio* (a seeking), *exemplum* (example).

## Rules every gameplay module follows

- **Data in the module's folder,** embedded with `include_str!` (compile-time paths stay in the folder). Edit it and
  `logic_build` reloads it in about a second; bad data refuses the load (`start` fails with `bad_data` naming the
  place, e.g. `items.json: items.bone.stack must be a whole number 1..9999`) and the running build stays.
  `Value::parse_json` (SDK) parses it; it refuses text over 256 KiB or nested deeper than 64 before building a tree.
- **Stateless code, state in the world.** Inventories, casters, talks and quest records are entity `props` with a
  `format` number: visible in `world_query`, saved with world files and save games, journaled, undoable, replayed by
  `world_hash {verify}`. The state store holds only small bookkeeping (who is talking, effect entities to expire,
  event names worth listening to).
- **Integers where numbers add up.** Weights are grams, resources are thousandths, cooldowns and timed statuses are
  tick numbers. Resources regenerate by arithmetic on the tick, never a write per tick, so an idle caster costs
  nothing. Floats appear only for positions and radii, through `dmath`.
- **Idle ticks are nearly free.** A module parses its data only when a tick has something to do: an input action
  pressed, or an event it listens to. Otherwise it makes zero or one request (a state read) per tick.
- **Talk by events, never by calling each other.** Requests and notices are events (`[a-z][a-z0-9_]{0,63}`, delivered
  next tick to every module), named `<module>_<what>`:

| Event | From | Data | Meaning |
|---|---|---|---|
| `inventarium_give`, `inventarium_take` | anyone | `{owner, item, count, partial?}` | please change an inventory |
| `inventarium_changed` | inventarium | `{owner, item, delta, count}` | an inventory changed (`count` = now held) |
| `inventarium_picked`, `inventarium_refused` | inventarium | ... `code` | a pickup; a request that failed, and why |
| `facultas_learn`, `facultas_request` | anyone | `{caster, ability}` | please teach / cast |
| `facultas_cast`, `facultas_hit`, `facultas_defeated`, `facultas_learned`, `facultas_refused` | facultas | `{caster, ability, target?, tag?, hp?, code?}` | what an ability did |
| `colloquium_start` | anyone | `{actor, graph, with?}` | please start a talk |
| `colloquium_node`, `colloquium_choice`, `colloquium_ended` | colloquium | `{actor, graph, node, line}` | where a talk is |
| `quaestio_start`, `quaestio_signal`, `quaestio_abandon` | anyone | `{actor, quest | signal}` | please start / progress / drop |
| `quaestio_started`, `quaestio_progress`, `quaestio_completed`, `quaestio_refused` | quaestio | `{actor, quest, objective?, value?, target?}` | quest news |

  **Notices are trusted only from their module.** Any module may emit any event, so a listener credits a notice
  only when `Event::from` is the module its name starts with (`facultas_defeated` from `facultas`); quaestio answers
  a forged one with `quaestio_refused {code: "forged"}` and credits nothing. Requests ("please ...") are open to
  every module by design.

  A module may also *read* another's documented props (abilities read `props.inventory` for a required item; dialogue
  conditions read `props.inventory` and `props.quests`), but only the owning module writes them.
- **Hooks for agents and Lex,** every one with a JSON Schema and structured errors (`logic_hook {hook:
  "inventarium/give", args: {owner: "/ambulo/player", item: "bone", count: 3}, reason}`), plus `status` / `inspect` /
  `state` hooks that change nothing.
- **Input as actions** (`specula.input.v1`, keyboard and pad both, agents through `logic_input`): `interact` (E /
  pad West: pick up), `ability_1..3` (Q, R, F / bumpers, North), `talk` (E, Enter / pad West), `choice_1..4` (1-4 /
  d-pad).
- **"What is near me" reads the tag index** (`Cx::query_near(tag, at, radius, limit)`, SDK; the `query` op with
  `near`): pickups, damage targets and talkers come from the engine's tag index within reach, so the cap counts only
  what is in reach and a press never walks the world (a defeated target is retagged `defeated`). More than 65,536
  entities on one tag is refused (`limit`).
- **Refusals change nothing.** A cast is checked (known, ready, affordable, and within the call's sound budget,
  `specula_sdk::MAX_SOUNDS_PER_CALL`) before it is paid for; what runs after paying can only fail unexpectedly, and
  that error goes up so the host rolls the whole call back. Effect entities are unnamed, so casts of one effect in
  one tick never collide.
- **Caps everywhere, checked before work** (common.md "cost before work"): item kinds 256, slots 64, stack 9,999,
  abilities 64 with 8 effects, dialogue graphs 64 × 256 nodes × 4 choices, conditions 32 parts and 6 deep, quests 64 ×
  16 objectives, actors in talks 16 and in quests 64, event requests handled per tick 64-256, targets per damage 256,
  effect entities alive 64. A count, a slot or a weight past its cap is refused with the cap in the message.

## Text: keys, not translations

Every line a player reads is `{key, text}`: a string-table key (`dialogue.sexton.greet`, `item.bone.name`,
`quest.bones.title`, `ability.flare.name`) and its English text. The simulation stores and compares **keys only**, so
a run is the same in every language and a replay doesn't depend on the player's locale. World texts the modules show
(dialogue lines and choices, the quest tracker) carry their keys in `props.line` / `props.keys` beside the English
text they draw.

**Seam with the localisation lane (m5-l10n):** when string tables land, the text component (or the module) resolves
`key` with the player's locale and falls back to `text`. The modules' keys are listed in their data files, so
`l10n_check` can find missing and unused ones. Until then the English text is shown.

## Sound: `Cx::sound` (new in the SDK, ABI unchanged)

`cx.sound(name, at, gain)` asks the engine to play a sound recipe: the module's own `sounds/<name>.json` first, then
the project's assets. It is **presentation only** (crates/specula/src/module_sounds.rs):

- the host queues a call's sounds only when the call commits (a failed or refused call plays nothing; a staged Lex
  hook's sounds wait for Lex's answer); at most 16 per call and 64 waiting;
- the engine renders a recipe once, on a worker (file ≤ 64 KiB, sound ≤ 10 s, cache ≤ 64 sounds and 8 M samples,
  4 renders at once), and plays it on the `sfx` bus as `module:<name>` (`audio_played {by: "module:facultas"}`);
- a missing recipe, a render error, audio being off or a render finishing more than 0.5 s late is skipped and traced
  (`module.sound` under `module:<name>`), never an error, and never seen by any module, the hash or a save.

The request op is additive (`sound`), so the ABI version stays 1; an older engine answers `unknown_op`.

## What a module can reach (security)

A module is **trusted native code**: it runs inside the engine's process with the owner's user rights, and only the
owner or a lead the operator granted may build, load or unload one; any other agent's `logic_build` files a proposal
the lead approves ([MODULES.md](MODULES.md), *Who may load a module*). Nothing below is a sandbox: the source checks
are lints, defence in depth behind that rule. (A sandbox for untrusted modules is an M6 decision: MODULES.md, *M6*.)
What Specula does:

- **Through the SDK** a module reaches: the world (as `module:<name>`, under the write policy's reserved-data rules,
  journaled), its own state store and RNG streams, its input **actions** for the tick (never keys), events, the
  character controller, log lines, traces and sounds. Every request is bounded (10,000 per call, reply sizes, event
  sizes).
- **The window's keyboard is never a module's.** Modules get actions latched per tick from the bindings in their
  action map; while the window's operator types into a text (`text_edit {window_focus}`, TEXT.md) the typing takes
  those keys first, and under an engine screen (editor, review, gallery) modules get no input at all. A module can't
  open a text field or read typed text; dialogue choices are actions.
- **`logic_build` lints native reach** (`specula_logic::build`):
  - `os_crate`, before Cargo runs: the module's normal dependencies include an engine crate that calls the operating
    system (`specula-sys`, `-platform`, `-input`, `-gpu`, `-audio`, `-protocol`, `-logic`, `specula`). Dev-dependencies
    (tests) don't count: they never reach the library.
  - `native_code`, after the build, over every file rustc compiled for the module and every other crate except the
    engine's own SDK and core (trusted by canonical path): foreign functions however the ABI is spelled, `#[link]`,
    `link_name`, any mention of inline assembly, any `unsafe`, `std`'s `fs`, `process`, `net`, `os` and `env`, sockets;
    files too many or too big to check. Each of the review's eight bypasses of the first scan is a permanent fixture
    (`crates/specula-logic/tests/hostile_modules.rs`).
- **What a loaded module can still do:** whatever safe Rust can do that the lint doesn't recognise, through a
  compiler soundness hole, or by racing the files during the build; loop without making requests (no preemption).
  That is why loading is the owner's or a granted lead's decision.

## The demo: `exemplum`

`scripts/gmods_check.py` builds the six modules with `logic_build` (ambulo for walking, then the four, then
exemplum) and plays it through, as an agent would with `logic_input` and `logic_hook`:

1. Walk to the Sexton, talk (E), "Can I help?" (1), "I'll find them." (1): quest "The Sexton's Bones" starts and the
   Sexton hands over his lantern.
2. Pick up three bones (E): the tracker counts them.
3. Flare (Q) near the wisps twice (cooldown 2.5 s, 35 stamina): two wisps are banished.
4. Back to the Sexton: "About those bones..." (1), "Here are three bones." (1): the bones go, the quest completes, a
   silver key and the "ward" ability (R) are the rewards.

Checked: the run's hashes match a second engine's run of the same script (determinism across two runs),
`world_hash {verify}` replays the journal to the live hash, a save written mid-quest loads and finishes the same, and
`logic_build` reports no diagnostics for any of the five crates.

## Tests

Each module's crate tests its rules as plain functions and then runs the module in the engine's real module host
(`ModuleCode::of`, linked in: same ABI, messages and rollback). The demo's tests (`games/exemplum/src/tests.rs`)
link all six modules (each crate's `linked` feature leaves out its exported entry point so they fit in one test
binary) and play the quest through, twin runs, and a save and load mid-quest. `specula-logic`'s lint and build tests
cover the reach checks (a keylogger's `extern` block, `#[link]` in a `cfg_attr`, inside a macro, in a helper crate).

## Writing your own

Copy a module's folder, rename the package and module, change the data. Keep: `#![deny(unsafe_code)]`, the simulation
`clippy.toml`, `[profile.dev.package.<name>] debug = 0, strip = "debuginfo"`, state in props with a `format`, events
named `<module>_<what>`, `{key, text}` for player-facing text, caps checked before work, and `crate-type = ["cdylib",
"rlib"]` with a `linked` feature only if tests link it beside other modules.
