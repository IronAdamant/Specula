# Design: save games

Status: designed and built 2026-10-18 (M3 lane "save games and the balance lab"); review round 1 fixed the same day
(last-wave saves, crafted-save panics, loads off the engine thread, ownership by identity), and Sepulcra's player
saves grew slots, autosaves and a saves screen. Foundations **F2** (determinism)
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
| Lex | a run's state | Sepulcra's `--rules lex` run is in the game's section, live runs (`lex_live`) in their own (see below) |

## What a save holds

A save is taken **between ticks**, on the engine thread, at tick `T` (ticks completed). Sections:

| Section | Contents | Written by |
|---|---|---|
| `world` | the world's files exactly as `world_save` would write them (every persistent entity, the environment, the id counter, world time, cameras, Lex rule sets under `/lex`), packed | `persist::plan` + `render_plan` |
| `game` | the hosted game's own bytes, in its own versioned format (Sepulcra: the sim `State` codec, the run's recording, the build order being followed, its pause and speed, presentation time) | `Game::save_state` |
| `physics` | the bridge's state (layer table, what it last saw of every entity, the scale each collider was built at, characters' carried velocities, entities it couldn't simulate) and the space's hidden state (every body's exact pose, velocities, mass frame, sleep timer and island, target, fat box; every contact pair with its points, impulses and SAT memory; the moved set, awake set, islands, characters) | `Physics::save_state`, `Space::save_state` |
| `logic` | each module's state store, RNG streams, seed and load count; events waiting for the next tick; per running module its build hash, generation, disabled state and input (agents' holds, the latched edges, and since the section's version 2 what waits for the next tick: whether each hold is a press no tick has read yet, holds let go before a tick read them, and every press in order, told to the module as `press`; version 1 still loads, without them) | `LogicHost::save_state` |
| `nav` | navigation's hidden state as versioned JSON (floats as bits): each navmesh's crowd (agents' positions, velocities, polygons, corridors, crossings, filters) with the navmesh hash and build counters it belongs to, every mind's running state and blackboard, agents' runtime goals and facing, the mind round robin, and the nav hash. Navmeshes are rebuilt from the world, except their build state, saved as data: the dirty and unbuilt tiles and the polygons of tiles built from geometry that has changed since (a load builds only the up-to-date tiles, puts the stale ones back and keeps the queues, so per-tick rebuilds go on exactly as in the saving run). Nothing is built for a save (it is never an input to the run); only a navmesh switching to new settings, or a load still being prepared, is refused (`nav_building`, retry: the game's autosave is retried) | `Nav::check_saveable`, `Nav::save_state`, `nav::encode_section` |
| `lex_live` | each live Lex run (section version 1): its Lex state (snapshot v2) and Lex hash, the set's document as compiled with the run's params and seed, its `present` rules and input bindings, its anchor on the engine clock, credit, work, debt, a halt, input and hook counters, the newest journal entries that fit 16 MiB (the count left out kept), and what it presents (each shown entity's Lex id, rule and last position); agents' holds are left out (sessions aren't saved; the save says how many) | `lex_live::Live::save_state` |
| `sounds` | the sounds the rules have asked for, in order, each with how loud it was to be heard (the last 512, and how many came before them): what a run reports as `run.sounds` and `run.sounds_heard`. There only when one was asked for; a save without it puts none back | `ModuleSounds::save_section` |
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

**Saves from before 2026-10-22** name their modules' builds with the old formula (a hash of every byte of the
library); the build hash now leaves out the link time stamps (DETERMINISM.md, *Module builds in the journal*). Loading
such a save reports each module as "another build" even when it is the same code. The module state still loads; only
the note is wrong. Save again to record the new hashes.

## Loading

A load runs in two halves. **On a worker thread**: read the file, decode it, run the migrations, parse and check
every section (the world through `world_load`'s validation, the module state, and the game's state through
`Game::state_parser`, a `Send` parser the game hands over when the load starts) and rebuild physics against the
save's world. **On the engine thread**: only the swap, a few milliseconds (a game without a parser has its section
parsed at the swap; Sepulcra has one).

**What a crafted load can cost.** Everything a file can claim is capped, but within the caps a crafted save can
still keep one worker core busy for a few seconds and hold about 1.3 GB of memory (the decoded sections, the world
built from them, physics rebuilt against it) while it is read and refused. Only one load runs at a time per engine,
so that is the bound per engine; the engine thread and other agents' calls aren't held.

**Not during a playtest.** A playtest owns the clock and the world until it ends, so loads are refused while one
runs, with `force` too, at the start and again at the swap (`playtest_running`, with `retry_after_ms`); the player's
Load from the menu says "Can't load while a playtest is running". Agents' calls keep being
answered while a big save is read (measured below), and the engine thread never inflates a crafted file. Before the
swap the engine checks again that nothing changed meanwhile (claims taken, unsaved edits made since the load started).

`save_read` checks everything that can fail **before anything live changes**: the file (size, framing, checksum,
every section's CRC, migrations), the game and tick rate (`wrong_game`, `wrong_tick_rate`), the world (loaded through
the same validation as `world_load`), the game's state (`Game::parse_state`: Sepulcra refuses a save made with other
content, `content_changed`), the module state, and physics, which is restored into a scratch `Physics` against the
save's world (bodies rebuilt in id order with the saved layer bits and build scales, then the hidden state put back
and the space's hash compared with the saved one: `save_mismatch` if they differ). A save from a build with another
`dmath` or Lex runtime version still loads, with a note that its hashes can't be compared (the state is right; the
arithmetic that produced the saved hashes may differ). Then, in one go: the clock jumps to
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
version          u32 LE (savegame::VERSION, 2)
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
- **Version 2** (2026-11-06): the `world` section's files are `specula.world/v2`, which record each entity's
  identity (`born`, the spawn lineage it was born in: docs/design/EDITOR.md, "Stable entity identity"). The v1 to v2
  migration changes nothing; a version 1 save's world files load with fresh identities, so notes and critiques on its
  entities from before are detached (`games/sepulcra-app/tests/saves.rs` reads a committed version 1 save).
- **Bounded, structured errors.** File ≤ 1 GiB, header ≤ 1 MiB, ≤ 32 sections, each decoded section ≤ 512 MiB
  (decompression stops at the declared size), every count checked against the bytes left before anything is
  allocated. **Decompression bombs** are refused from the section table, before inflating anything: the sections may
  decode to at most 1,024 times the file's size plus 16 MiB, and 512 MiB in all (`too_large`). Very repetitive worlds
  compress a few hundred to one (3,000 entities with the same 16 KB prop: 48 MB of text in a file under 2 MB) and
  stay compressed; only data past the cap would be written raw. Whatever passes is inflated on the load's worker.
  The hosted game caps its own section too (Sepulcra: state and recording at most 2 MiB each, refused at write
  time when a run would pass them, refused at load time before parsing). A damaged file is a `SaveError` with a code (`not_a_save`, `truncated`, `checksum`, `corrupt_section`,
  `invalid_header`, `too_large`, `unknown_codec`, `trailing`, `unsupported_version`, `no_migration`), the section, the
  byte offset, and what to do.
- **Fuzzed.** Unit tests cut every save at every length, flip bits everywhere, and run thousands of random edits with the
  checksum fixed up so the parsers behind it are exercised; nothing panics. The game, physics, logic and Lex sections
  are fuzzed the same way, and decoded state is checked against the rules it will run under (indices into maps, kinds,
  tiers, waves, paths, effects; ranges of positions, amounts, ticks, ids and counters; counters cross-checked with the
  lists they count, such as a wave's pending and alive enemies), so a hand-made save is an error, never a panic in a
  later tick. The Sepulcra and Lex fuzzers also write extreme values (0, 1, -1, MIN, MAX) into 1-8-byte fields and then
  **step hundreds of ticks** after every load that is accepted (`SPECULA_FUZZ_ROUNDS` runs more); the crash file a
  review found is a test (`games/sepulcra-app/tests/data/crash_1_57.specsave`). Waves are 1-based in saves, as in the
  sims: a save in the last wave, or between the last wave's clearing and the victory, loads.
- **Atomic writes.** Temporary file, `sync_all`, rename: a crash leaves the old save or the new one.

## Tools

| Tool | What it does |
|---|---|
| `save_write {name, label?, overwrite?, force?}` | Saves at the current tick. The engine thread takes the state and owned copies of the world (Sepulcra's world: ~3 ms); rendering, compression and writing run on a worker thread, which answers when the file is on disk. At most two saves are written at once (each holds a copy of the world): a third is `busy` with `retry_after_ms`. An existing save is replaced only with `overwrite`, and another agent's save (or one whose header can't be read) also needs `force`. |
| `save_read {name, force?, inspect?}` | Loads (see above). Refused while other agents hold claims (`claimed`: it replaces everything they work on), while a playtest runs (`playtest_running`) and when the live world has unsaved changes (`force` discards them). One load at a time; a save being written or read is `busy`. `inspect` streams the file and checks its framing, header, section table and checksum **without decompressing anything** (the header plus 64 KB of memory, whatever the file); at most four run at once (`busy` beyond). |
| `save_list {cursor?, limit?}` | Saves, newest first (reads only headers). Never waits for a save being written: that one is listed with `writing: true`. |
| `save_delete {name, force?}` | Only the agent that wrote a save may delete it, unless `force`. |

**A saves folder holds at most 4 GiB** of saves: a save that would pass it is refused (`saves_full`, with the bytes
used, the number of saves and the cap); saving over a save counts only the new one. Saves being written reserve
their size until their file is in place, so two writers at once can't pass the cap together.

**Ownership is an identity, not a name.** A save's header keeps who wrote it twice: the display name, and an owner key
derived from the writer's identity (`agent_hello`'s identity, or `game:<name>` for the hosted game's own saves), so an
agent that takes another's display name can't overwrite or delete its saves without `force`.

**Players** save and load from Sepulcra's **Saved games** screen (pause menu, then *Save / Load*; it works with a pad,
a mouse, touch or the keys, F6): three slots (`sepulcra_slot_1..3`) and two autosaves (`sepulcra_autosave_1..2`),
each showing what it holds (wave and tombstones), time played and when it was saved (UTC); empty slots say so and
their Load is greyed. Saving over a used slot, loading (the run in play is lost) and deleting each ask first, with
*No* as the default. **Autosave** writes the next autosave in turn as each wave starts (wave 1 to autosave 1, wave 2
to autosave 2, ...): **on by default with a window, off headless** (agents' runs and playtests don't fill the disk),
and Settings switches it. The game asks the engine through `Game::take_requests` (`GameRequest::Save`, `Load`,
`List`, `Delete`) and hears back through `Game::request_done`: saves and loads run in the background like agents',
the screen says "Saving..." or "Loading..." meanwhile, and a toast says how it went ("Game saved", "Autosaved",
"Game loaded: wave 3 of 10", or the error). After a load the game waits, paused under the menu. Agents press the same
buttons with `play_ui` (`saves`, `save_1`, `load_2`, `delete_3`, `load_auto_1`, `confirm_yes`).

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
  (world 73 KB, game 2 KB, physics 19 KB, logic 0.2 KB before compression), 5-6 ms to write; loading takes about 50 ms
  (37 ms reading and checking on the worker, 7 ms swapping on the engine thread). With 10,000 extra static entities:
  90 KB on disk (2.0 MB of world text), loading about 350 ms of which 48 ms on the engine thread; another agent's
  `world_list` answers in 20 ms (median, 77 ms worst) while such loads run. A 300 KB file of 300 MiB of zeros is
  inflated on the worker and refused (no world in it); a save carrying a 32 MiB recording is refused by Sepulcra's
  cap on the worker (156 ms, the engine thread untouched); eight inspections at once all answer; a load during a
  playtest is refused, for an agent and for the player. The script also drives the player's Saved games screen through `play_ui`
  (save two slots, a confirmed delete, a refused overwrite, a confirmed load back to the same state hash, a wave-start
  autosave) and keeps its captures (`--keep`).

## Lex

**Sepulcra's `--rules lex` mode** saves its Lex run in the game's section (section version 2): the run's
`Sim::save_state` bytes, plus the parity counts and any desync, restored against the built-in rule set (its hash must
match) and checked against the reference state at load. A version 1 section has no Lex part, so the run replays the
recording through Lex to the saved tick, as before. A load resumes the Lex run exactly (same Lex hash, parity on;
`a_lex_mode_game_saves_and_loads_with_its_lex_state`).

**Live Lex runs (`lex_live`)** are saved in the `lex_live` section (above). A load parses it on the save reader's
worker against the loaded world: it compiles each saved document with the run's params (the rules' hash must be the
one the state was saved with, else `lex_rules_changed`), restores the state for a hosted run (module hooks run
through the host), checks the Lex hash (`save_mismatch`), and **adopts** what the run presents from the loaded world
by path (`/lex_live/<set>/e<id>`), never respawning it, so the world hash goes on as the saving run's. The clock is
restored to the save's tick, so each run's anchor holds as saved. At the swap the runs live before the load stop
(their world is gone; the answer lists them) and the save's take over; the engine's own `[lex_live]` limits stay.
Agents' holds are not saved (they end with their sessions): the save notes how many it left out, and a load starts
with none, so a hold still on at the save makes the two runs differ from there. The live runs are a subsystem hash
(`lex_live`: each run's Lex hash, anchor, credit, debt and counters) in the save's header and in the desync
detector's traces. `scripts/save_check.py` saves Sepulcra with a live run beside it (commanded, and pressed through
an input binding), loads it in a fresh engine, and `desync_compare` matches every tick after, `lex_live` included;
`lex_live.rs` tests a save loaded into another engine stepping 300 ticks with the same world and live hashes, the
section round-tripping byte for byte, holds left out, and damaged sections (cut at every length, bits flipped) being
errors or runs that step, never panics. The run state itself: `Sim::save_state` / `Sim::restore` write and check a
run's whole `State` against the rules it runs under (the rules hash, params folded in, must match: `rules_changed`),
and a restored run has the same `Sim::hash`, steps identically and charges the same work per tick
(`games/sepulcra/tests/lex_saves.rs`: the slice saved mid-wave, three seeds, 4,000 ticks; the whole game, with
natives, phase `order` and wave groups' `with`, saved every 2,500 ticks).

**Lex state versions.** Version 2 adds entities' `order` and pending spawns' `with` (both in the state hash, for
rules that use them). Version 1 reads forward: its runs had neither, so they restore with the defaults and hash as they
did (the hash covers both only for rules that use them); rules that use them refuse a version 1 state. A restored
state is a new state value (a fresh instance; caches miss once), and when granted tags next run out is derived from
its effects.

## Later

- A title screen's Continue (the newest of the slots and autosaves).
- Saving the swarm's coordination with a save (task board, claims) if the owner wants "save the whole studio"; today
  the board is saved with world folders.
- Save thumbnails (a capture in the header) for load screens.
