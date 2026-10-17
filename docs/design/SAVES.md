# Design: save games

Status: designed and built 2026-10-18 (M3 lane "save games and the balance lab"). Foundations **F2** (determinism)
and **F1** (swarm-first tools). Roadmap item: "Save games" in [ROADMAP.md](../ROADMAP.md). Rules it follows:
[DETERMINISM.md](DETERMINISM.md); the parts it saves are designed in [MODULES.md](MODULES.md),
[PHYSICS.md](PHYSICS.md) and [LEX.md](LEX.md).

**Goal:** with determinism, a save is a serialisation of simulation state. Loading one makes the engine continue
**bit-identically**: the same `world_hash` and the same subsystem hashes on every later tick as the run that saved,
which the desync detector (`desync_compare`) checks. World files ([persist.rs](../../crates/specula/src/persist.rs))
stay what they are, the editor's diff-friendly format; a save game is a different thing: everything the simulation's
future depends on, in one file.

## Why world files aren't saves

A world file holds what the world shows. The simulation remembers more:

| Where | What a world file loses | Effect of reloading a world file |
|---|---|---|
| The hosted game | its whole simulation (Sepulcra's `State`: enemies, waves, dust, RNG streams), its pause and speed, the run's recording | the game starts over |
| Physics (`specula-physics`) | exact quaternions (the world stores Euler degrees), warm-start impulses, sleep timers and islands, the per-pair polytope memory (SAT features, part of the physics hash since review round 2), fat broadphase boxes, kinematic targets, the characters' ground memory, and the scale each collider was built at | the run continues deterministically but **diverges** from the one that saved (shown by `a_plain_rebuild_drifts...` and `a_saved_world_and_physics_continue_bit_identically` in the tests) |
| The module host | each module's state store and RNG stream positions (MODULES.md: "comes with save games"), the events emitted in the last tick (delivered at the next), agents' held input | modules start empty |
| The clock | the tick | tick numbers restart, so traces and journals don't line up |
| Lex | a run's state | (no live Lex yet: see below) |

## What a save holds

A save is taken **between ticks**, on the engine thread, at tick `T` (ticks completed). Sections:

| Section | Contents | Written by |
|---|---|---|
| `world` | the world's files exactly as `world_save` would write them (every persistent entity, the environment, the id counter, world time, cameras, Lex rule sets under `/lex`), packed | `persist::plan` + `render_plan` |
| `game` | the hosted game's own bytes, in its own versioned format (Sepulcra: the sim `State` codec, the run's recording, the build order being followed, its pause and speed, presentation time) | `Game::save_state` |
| `physics` | the bridge's state (layer table, what it last saw of every entity, the scale each collider was built at, characters' carried velocities, entities it couldn't simulate) and the space's hidden state (every body's exact pose, velocities, mass frame, sleep timer and island, target, fat box; every contact pair with its points, impulses and SAT memory; the moved set, awake set, islands, characters) | `Physics::save_state`, `Space::save_state` |
| `logic` | each module's state store, RNG streams, seed and load count; events waiting for the next tick; per running module its build hash, generation, disabled state and input (agents' holds, the latched edges) | `LogicHost::save_state` |
| (header) | tick, tick rate, game name, the world hash and every subsystem hash at the save, entity count, engine version, `dmath::VERSION`, Lex runtime version, who saved it, a label | `save_tools` |

Physics is **synced with the world before saving** (what the next tick's sync would do), so the save describes one
consistent state. The saving run continues from exactly that state. (A run that never saved would sync the same edits
one tick later, inside the next tick's sync; the two differ only when the game and an agent change the same entity
between the same two ticks.)

**Not in a save** (and why): presentation the game rebuilds from its state (transient entities are never saved or
hashed); the human's device state (keys physically held); agents' sessions, claims, the journal's history and the task
board (a save is the simulation, not the swarm's coordination; the journal starts over from the loaded world, so
`world_hash {verify}` and `desync_compare {b: "replay"}` work from the load); module **code** (a save names each
module's build hash; a load reports a missing or different build, DETERMINISM.md rule 8).

## Loading

`save_read` checks everything that can fail **before anything live changes**: the file (size, framing, checksum,
every section's CRC, migrations), the game and tick rate (`wrong_game`, `wrong_tick_rate`), the world (loaded through
the same validation as `world_load`), the game's state (`Game::parse_state`: Sepulcra refuses a save made with other
content, `content_changed`), the module state, and physics, which is restored into a scratch `Physics` against the
save's world (bodies rebuilt in id order with the saved layer bits and build scales, then the hidden state put back
and the space's hash compared with the saved one: `save_mismatch` if they differ). Then, in one go: the clock jumps to
tick `T`, the world is replaced, physics, modules and the game take their state, and the hashes are compared again
with the header's; the answer says per subsystem whether it matches (`match: true` is the expected answer).

The game's `apply_state` must **adopt** the entities the world already holds, not rebuild them: new entities would
move the id counter and the world hash away from the saving run. Sepulcra's `Scene::adopt` finds `/units/stones/<cell>`,
walls, roots and tombstones by path and rebuilds only the transient presentation (`/live`, `/cursor`).

A loaded world isn't a world folder: autosave stops (`world_save {name}` keeps it as world files).

## The format

`<saves dir>/<name>.specsave` (`--saves`, default `./saves`):

```text
"SPECSAVE"       magic, 8 bytes
version          u32 LE (savegame::VERSION, 1)
header length    u32 LE (at most 1 MiB)
header           JSON: the metadata and the section table [{name, codec, bytes, stored, crc32}]
sections         each section's stored bytes (zlib, or raw under 256 bytes), in table order
crc32            u32 LE of everything before it
```

- **Exact.** Floats are written as their bits (`specula_core::bytes`), integers as they are: determinism rule 9.
- **Versioned, with migrations.** The framing never changes; a version changes the header's fields and the sections'
  contents. `savegame::MIGRATIONS[i]` is a pure function from a version `i + 1` save to version `i + 2`; reading runs
  the chain, so old saves load forward; a save newer than the build is refused (`unsupported_version`). Sections have
  versions of their own (physics, logic, the game's, Lex's), each refused cleanly when unknown.
- **Bounded, structured errors.** File ≤ 1 GiB, header ≤ 1 MiB, ≤ 32 sections, each decoded section ≤ 512 MiB
  (decompression stops at the declared size), every count checked against the bytes left before anything is
  allocated. A damaged file is a `SaveError` with a code (`not_a_save`, `truncated`, `checksum`, `corrupt_section`,
  `invalid_header`, `too_large`, `unknown_codec`, `trailing`, `unsupported_version`, `no_migration`), the section, the
  byte offset, and what to do.
- **Fuzzed.** Unit tests cut every save at every length, flip bits everywhere, and run thousands of random edits with the
  checksum fixed up so the parsers behind it are exercised; nothing panics. The game, physics, logic and Lex sections
  are fuzzed the same way, and decoded state is checked against the rules it will run under (indices into maps, kinds,
  tiers, waves, paths, effects; ranges of positions and amounts), so a hand-made save is an error, never a panic in a
  later tick.
- **Atomic writes.** Temporary file, `sync_all`, rename: a crash leaves the old save or the new one.

## Tools

| Tool | What it does |
|---|---|
| `save_write {name, label?, overwrite?}` | Saves at the current tick. The engine thread takes the state and owned copies of the world (Sepulcra's world: ~3 ms); rendering, compression and writing run on a worker thread, which answers when the file is on disk. An existing save is replaced only with `overwrite` (the error names who wrote it). |
| `save_read {name, force?, inspect?}` | Loads (see above). Refused while other agents hold claims (`claimed`: it replaces everything they work on) and when the live world has unsaved changes (`force` discards them). `inspect` only checks and describes the file. |
| `save_list {cursor?, limit?}` | Saves, newest first (reads only headers). |
| `save_delete {name, force?}` | Only the agent that wrote a save may delete it, unless `force`. |

**Players** save and load from Sepulcra's pause menu (Escape or the pad's Start, then *Save game* / *Load game*): menus
work with a pad, mouse, touch or keys (F6). The game asks the engine through `Game::take_requests` (`GameRequest::Save`,
`Load`, `List`) and hears back through `Game::request_done`; it shows a toast and, after a load, stays paused under
the menu. One slot for now (`sepulcra_slot_1`); agents press the same buttons with `play_ui`.

## Proof and numbers

- `crates/specula-physics/src/snapshot.rs`, `crates/specula/src/physics_save.rs`: a restored space (rebuilt in
  another insertion order) and a restored world+physics step bit-identically for 300 and 155 ticks; a plain reload
  drifts.
- `crates/specula/src/logic/tests.rs` (`a_saved_host_continues_exactly`), `games/sepulcra/src/sim_save.rs`,
  `games/sepulcra-app/tests/saves.rs` (a whole Sepulcra run saved mid-wave and loaded into another game instance: the
  same world hash and subsystem hashes on each of 3,000 later ticks), `games/sepulcra/tests/lex_saves.rs`.
- **Live:** `scripts/save_check.py` (ports 7600-7601): Sepulcra seed 7 with 24 physics crates and the ambulo module
  walking, saved at tick 600 in engine A, loaded in a fresh engine B; `desync_compare` of the two runs' traces over the
  next 900 ticks reports `match: true` with world, `sim`, `economy`, `enemies`, `defences`, `projectiles`, `physics`
  and `logic` compared. Measured (2026-10-18, release, headless, i9-13900HX, shared): the save is 19 KB on disk
  (world 73 KB, game 2 KB, physics 19 KB, logic 0.2 KB before compression), 5-6 ms to write; loading takes 26 ms. With
  10,000 extra static entities: 90 KB on disk (2.0 MB of world text), loading 150 ms.

## Lex

Lex doesn't run live in the engine yet (LEX.md: "live Lex in the window" is a later lane), so there is no live Lex
state to save today: rule sets themselves are world entities and are in every save's `world` section. The run state is
ready for it: `Sim::save_state` / `Sim::restore` write and check a run's whole `State` against the rules it runs under
(the rules hash, params folded in, must match: `rules_changed`), and a restored run has the same `Sim::hash` and steps
identically (`games/sepulcra/tests/lex_saves.rs`: the slice saved mid-wave, three seeds, 4,000 ticks). When live Lex
lands, its sets' states become a `lex` section, one `Sim::save_state` per running set.

## Later

- More player slots and a load screen with labels (save_list has the data); a title screen's Continue.
- Saving the swarm's coordination with a save (task board, claims) if the owner wants "save the whole studio"; today
  the board is saved with world folders.
- Background *loads* (reading and checking on a worker thread; only the swap on the engine thread).
- Save thumbnails (a capture in the header) for load screens.
