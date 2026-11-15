# Changelog

All notable changes to Specula are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — 0.3.0: native engine, M0–M2

Specula is now a zero-dependency, agent-native game engine (Rust `std` + OS/driver FFI only).
See [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md). Item-by-item detail, with the reasoning for each, is in
[docs/ROADMAP.md](docs/ROADMAP.md); the timeline of decisions is under [Project history](#project-history) below.

### M5, done (2026-11-06 to 2026-11-15)

Built by one agent, lane by lane, each lane reviewed by a model that did not write it and passed through
`scripts/gate.py` (owner, 2026-11-12: one builder; 2026-11-12: a gate must not pass by luck).

- **Exit proof** (`scripts/m5_exit.py`): assets are made once, saved, and are there to be used again, 2D and 3D,
  in a project that is not this repository. A project is made from a terminal; agents put a prop, a rigged
  creature, a tile map, sprites and a sound into it as project files; a second engine finds all of it, loads the
  saved world to the same hash and the same pictures, and the assets are reused, copied and changed, undone,
  edited by a person on disk, and written as a `.glb` that reads back. On the Intel iGPU and the RTX 4090.
- **Measured leverage** (`scripts/leverage.py`, [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md)): the same game
  jobs built through the engine and without it by the same model, judged against one list. At the sizes tested
  the engine costs more for the same result (about 5, 3.1 and 1.7 times the tokens for a small game, a medium one
  and a change to it), and why is measured: more requests, each reading the context again, and more written.
  It is the baseline M6 has to move.
- **Audio:** a hand-written Ogg/Vorbis decoder, streaming music, music as data, loudness metering, `audio_*`
- **Text:** bundled OFL fonts, GPU text, in-world text and editing, window typing with IME and clipboard, CJK
  fallback (`specula-text`)
- **UI sight tools** (`ui_tree`, `ui_frame`, `ui_act`, `ui_check`) and **timelines** with camera rails (`timeline_*`)
- **Localisation** (`specula-lingua`, `l10n_*`): string tables as project data, CLDR plural rules, a review gate;
  Sepulcra in English and German
- **Accessibility and Sepulcra's shell:** a title screen, settings saved per player, rebinding, gamepad pause,
  interface scale, colour-vision themes, reduced motion, subtitles and captions
- **The human editor on the agents' protocol** (F7): outliner, inspector, selection, gizmos, undo; the draw tools
  (sculpt, splat, paint, sketched paths); the Swarm tab (journal timeline, claims, board, review queue, costs)
  and "highlight and ask"; a window a person and an agent can share
- **Gameplay modules** as SDK examples (`inventarium`, `facultas`, `colloquium`, `quaestio`, `exemplum`)
- **The input monitor** (`security_*`): every way an agent could reach the human's keyboard, window or native
  code is classed, frozen by identity and shown at the window; lead authority comes only from `--lead NAME`
- **Tool kits by job** (`tools_catalog {kits, kit, tag}`): 21 kits; a kit lists about 13 KB where a role listed 85
- **The launcher:** a start screen with the projects worked on, and the engine found and called from a terminal
  (`specula projects`, `open`, `new`, `list`, `tools`, `call`, `stop`; `call TOOL @FILE`, `--as NAME --role ROLE`)
- **Asset export** (`model_export`): any recipe as a `.glb` with its textures, rig and clips
- **Map 1 ships the 33 approved props**, with a per-prop `blocks_build` flag
- **Rendering follow-ups:** alpha-cutout foliage and wind, real grass, render graphs prepared ahead per size
- **Reliability and performance:** GPU work, recipe writes, studio sheets, motion checks and world loads off the
  engine thread; a build request no longer reads the disk under the model library's lock; a call sent while the
  engine starts waits for the start; a stall says whether the engine worked or waited (`cpu_ms`, `lock_ms`) and
  a slow frame says what it did (`top`); the pipeline cache is the adapter's own
- **The gate** (`scripts/gate.py`): the workspace's tests, the engine's library tests five more times, once
  with every worker starting late, the speed budgets alone, clippy for Windows and Linux, both builds; a test
  that fails in some runs is FLAKY and fails it
- **Cloud swarm foundations** ([docs/design/CLOUD_SWARMS.md](docs/design/CLOUD_SWARMS.md)): the design for
  per-agent tokens and encryption from the OS, ready to build in M6
- **The final audit** (`scripts/live_checks.py`): every live check, one after the other, built first, each
  told its own ports. It found thirteen checks that had gone stale since they last ran (current again), and
  one defect of the engine that had been there since M3: **a capture that forces a level
  (`sight_frame {lod: 0}`) from a far camera now gets it** (streaming uploaded levels by size on screen
  only, so the capture drew what was on the GPU and counted the rest as coarser). Every script reads and
  writes its text, and reads its processes, as UTF-8 (a guard holds it). It also found that the scripts' own
  client waited 10 to 26 ms in about one call in seven (Python's connect with a timeout, on Windows), which
  checks read as the engine's time: the scripts connect first and set the timeout after
- **Module builds on Rust 1.100** (found by CI on the push that ended M5: this machine's Rust was one release
  older). Its Cargo keeps what rustc writes in `build/<package>/<hash>/out/` where `deps/` was; the build
  service found no dep-info there and refused every module. The dep-info it checks is now the one beside the
  build's own copy of the library, told by the library's bytes, in either layout (every copy with those bytes
  is checked, and a copy without dep-info refuses the build): in a target folder an older
  toolchain also built into, the older build's dep-info is never taken for the new library's (it was found by
  name, and before that by date). Stale build units of the new layout are dropped after a build as the old
  layout's files are. `scripts/gate.py` says which Rust it runs with and fails when a newer stable is out
  (`--older-rust`), since CI installs the newest
- **A call that is running when the server stops gets its answer** (`specula-protocol`). The server shut
  every connection's socket and then waited for the calls on them, so a call that was applied, or refused,
  had nowhere to say so. A call refused because the engine's start failed lost its refusal whenever its
  thread ran later than the engine's stop: one failure in one gate, of a test that had passed every run
  before. The server now waits for running calls and the writing of their answers (a caller that does not
  read is given 5 s), and begins nothing that arrives after the stop; the test makes the refused call late
  on purpose
- **A module build in a folder too deep for the linker says so** (`logic_build`: `build_path_too_long`, with
  the file, its length and the limit). On Windows rustc hands the linker its arguments in a file inside the
  build, and the toolchain's linker opens a path of at most 259 characters; at 260 it read none of them and
  the build failed with `liblto_plugin.dll not found`. Rust 1.100's build folders are 29 characters and the
  package's name deeper than the ones before. Found as one failed live check out of 70 on the final tree,
  in an output folder one character deeper than the last audit's: the runner refuses such an `--out`
  before it starts
- **Text files are LF in every checkout** (`.gitattributes`): the tree a change is gated in holds the bytes
  everyone else gets. A guard of the final audit matched a line with its line ending and passed only in the
  tree it was written in. In an LF tree one live check (`trace_check.py`) handed a module's source back with
  CRLF, having read and restored it as text: it handles bytes, a guard holds that form for every script
  that edits a source, and `scripts/live_checks.py` fails a check that leaves the checkout changed

### M4, done (2026-10-28 to 2026-10-30)

- **Exit proof:** a swarm (a director, four builders, an art director, a sight-skeptic and a debugger) built 33
  graveyard props that pass `style.toml`; one contact sheet went to the review queue; the owner approved it at the
  window, and they were promoted into the game world, which still won on the iGPU within budget (putting them in
  a fresh Sepulcra's map 1 is an M5 step)
- **Galleries and the owner's pick flow** (`specula-gustus`, `gallery_*`, F8): choose, don't describe; pick-driven
  tuning, the preference store (private), reference boards, style onboarding
- **Baked GI** (`specula-lux`, `gi_*`): deterministic CPU-baked probe volumes, tiers low and high, terrain and water
  included
- **Terrain and stylised water** (`terrain_*`, the `water` component)
- **Disk footprint budget and binary assets** (`disk_*`, `project_upload`)
- **Scoped tool lists** (`tools_*`): agents start with 6 tools (about 5 KB) instead of every tool (about 255 KB); the lead
  picks each agent's tools
- **Built-in tracing and an autonomous debugger role** (`trace_*`, incidents, watchdogs, `--strict-budget`)
- **Authority, jobs and provenance:** operator grants, an access table for every tool, one commit per call, drawn
  facts from a frame record
- **Bounds and workers:** schema limits, answer ceilings, a worker pool and step jobs; asynchronous capture readback;
  the remaining engine-thread stalls removed (worst window frame under heavy agent calls far lower; captures faster
  than before)
- **Fidelity T3** sheet for the five benchmark subjects
- **The final M4 audit and its fixes** (three review rounds): gallery calls wait for their answer (a dropped reply now says the outcome is unknown); every taste write (gallery apply and decide, suggestions, the configurator) is one commit, and a deferred job's commits count toward the call that started it; filmstrips measure brightness in the subject's own box; `sight_frame` leaves far outliers out of its auto framing; unchanged writes check claims; gallery `choice` parameters only on presets. The authority drill holds 119 probes

- **Asset studio:** `specula-studio` measures assets against `style.toml` (budgets, scale, palette families by exact
  Oklab search) and runs taste checks (silhouettes, thumbnail solidity, pixel heights, edge contrast, flat light)
  on an offscreen stage with light rigs and scale references. Contact sheets, line-ups, before/after marks and
  shot sets. Calls run on the caller's thread, and renders go to the engine round-robin within a budget. Six tools
- **Procedural modelling tiers 2-4:** CSG with bevels, SDF sculpts, terrain with erosion, scatter, a building
  generator and texture graphs, all under one deterministic work budget. Sepulcra's graveyard gains procedural
  headstones, a crypt and ruins. `style.toml` gains building, ruin, terrain and set categories (Claude's proposal)
- **Review queue, critique queue, merges and director tools:** review submissions with batch decisions and a modal
  review screen in the window; critiques linked to the board; three-way merge offers; `task_plan` and
  `task_progress`. Promotion and revert go through the write policy
- **Particles, effects and decals:** `specula-scintilla` effects as data; one instanced particle draw; deferred-style
  decals. Photosensitivity (WCAG 2.3.1) is checked per effect, flashing instances are never drawn, and a runtime
  flash watch measures the final frame (warn-only). Sepulcra gets fire, embers, wraith trails, impacts and dust
- **One write policy** (the owner's top process change after M3): every change to the live world goes through
  `write_policy`. Writer code gets `&mut dyn WorldWrite` (recorded writes only) and never `&mut World`; the live
  journal is written only with a key that nothing outside the policy can make; exemptions are named variants with
  reasons. A runtime backstop catches unrecorded writes, puts back engine settings and never loses project files.
  Claim holders get notices when module or live-Lex ticks write inside their claims. `logic_build` no longer picks
  clippy's dep-info file. Behaviour is unchanged: physics, joints and playtest end hashes are identical, and the swarm
  drill holds

### M3, done (2026-10-18 to 2026-10-22)

- **M3 exit on the iGPU:** `scripts/m3_exit.py` plays a whole Sepulcra game windowed on the Intel iGPU and holds
  every frame-level budget, with everyday desktop apps open (GPU p95 about 6 ms against 14). The scene pass is
  cheaper: point lights out of range are skipped, the shadow disc stops early where its rim taps agree, and the sky
  ray is computed per vertex. FXAA takes its luma from the tonemap pass. An optional window frame cap
  (`--max-fps`, `perf.toml [window] max_fps`) is off by default
- **M3 integration audit fixes:**
  - module RNG streams are capped (`rng_full`), so saves always load, and their hash is kept incrementally;
  - `lex_get` answers are capped near 64 KB, and big `lex_export`s are written to files;
  - `world_load` is refused while a playtest runs;
  - playtest replays hash live Lex runs;
  - reserved-data rules apply on every write path (games, modules, live Lex);
  - `world_load` lists and stops live Lex runs;
  - files that agents name are confined to the engine's own folders (`safe_path`: `lex_import`, `desync_compare`);
  - `anim_filmstrip` shares the world's shadow map, so it no longer makes the GPU wait idle;
  - `specula-model` is now a simulation crate

- **Swarm communication:** a task board (`task_*`: dependencies, owners, blockers, questions, a change feed; saved as
  `board.json` beside the world, with a 24 MiB budget), intents and notes (`intent_*`, `note_*`; overlaps flagged
  before any edit), conflict help on refused writes (`conflict {base, theirs, yours, overlap, rebase}`), roll-ups
  (`world_rollup`), resource leases (`lease_*`: ports, advisory GPU time, scratch folders), the window's frame floor
  under a swarm as a setting (`perf.toml [swarm]`, `swarm_settings`, lead only, with a hardware hint), and a desync
  detector (`desync_trace`, `desync_compare`, `Game::state_hashes`, `scripts/desync_check.py`). `specula-sys` gains
  `power()` and `net::tcp_listening_ports()` (runtime-loaded `iphlpapi`); `specula-gpu` gains `Gpu::memory_stats`
- **Module host:** game logic as hot-reloaded Rust `cdylib`s. New crates `specula-sdk` (what a module depends on:
  deterministic `Cx`, versioned C ABI, panics caught at the boundary) and `specula-logic` (loading, a build service
  that merges duplicate requests, Cargo diagnostics as data with machine-applicable fixes). Tools `logic_build`,
  `logic_reload`, `logic_unload`, `logic_status`, `logic_input`, `logic_hook`. Builds are vetted (path crates only;
  no build scripts, proc-macros, Cargo config or toolchain files) and run offline with `panic = "unwind"`. Example
  module `games/ambulo`; edit to reload about 1 s. Engine crates build at `opt-level = 2` in dev
- **Lex, gameplay as data:** new crate `specula-lex` (std only): rule sets as JSON with 13 item kinds (params,
  resources, paths, sites, archetypes, effects, machines, waves, timers, rules, commands, outcomes, natives), exact
  integer units, a typed expression language, GAS-like effects with stacking policies and tags, a fixed phase order,
  structured diagnostics with suggestions, and bounded work, memory and replies. Tools `lex_schema`, `lex_list`,
  `lex_get`, `lex_edit`, `lex_validate`, `lex_simulate`, `lex_import`, `lex_export`, `lex_session`; rule sets live as
  `/lex/<set>/<item>` entities (only the Lex tools write `/lex`). A slice of Sepulcra in Lex matches the hand-written
  sim tick for tick across seeds
- **Animation A3 and agent-authored motion:** graphs as data (parameters, 1D/2D blends, additive and external nodes,
  layered state machines with masks, transitions with conditions and exit phases), plays whose pose is a pure
  function of time, two-bone IK (feet on the ground with the pelvis following) and look-at; motion generators
  (`keys`, `pose`, `recoil`, `breathe`, `chain`, auto gaits); motion checks with exemptions from `style.toml`; tools
  `anim_get`, `anim_set`, `anim_state`, `anim_check`, `anim_filmstrip`. Sepulcra's creatures run on graphs with new
  clips (climb, glide, dive, pound, flinches, shuffle, jog), and the checks' fixes (a runner that hovered, colossus
  hands in the ground)
- **Locus and navigation:** new crates `specula-locus` (`world_place`: objects placed by relations, collision-checked,
  with the nearest valid alternative when a relation can't hold), `specula-via` (tiled navmeshes rebuilt per tile,
  paths, off-mesh links, area costs, crowds) and `specula-mens` (behaviour trees and utility AI as data); tools
  `nav_build`, `nav_path`, `nav_point`, `nav_agent`, `nav_info`, `ai_check`, `ai_set`, `ai_status`, `model_collider`;
  `navmesh` and `paths` overlays; a `nav` hash in `world_hash` and desync traces; nav state in save games, saved
  without changing the run
- **Live Lex runs in save games:** a `lex_live` section (state, journal tail, presentation, budgets) that loads by
  adopting the shown entities and is checked by hash
- **Sepulcra fully in Lex, and live Lex:** the whole rule set (flow fields, walls, climbing, lane jumps, flyers, roots,
  the colossus) as Lex data, tick-identical to the Rust sim; new general Lex features (data items, triggers, refusal
  details, ordered archetypes, grouped spawns); `lex_live` runs a set on the engine's tick with presentation, input
  bindings, deterministic work budgets, resumable halts and module hooks as natives; `sepulcra --rules lex` lets Lex
  decide the game, stopping with a dialog on any desync; Lex state (snapshot v2) in save games
- **Streaming and HLOD:** model and texture uploads go through a per-frame budget (`perf.toml [streaming]`), with no
  waiting on the GPU in the frame; residency follows on-screen size under a memory budget; distant cells draw as
  merged HLOD proxies built on a worker; captures wait for what they show. Tools `stream_state`, `stream_settings`,
  `stream_fly`; `perf_frame.streaming`; sight packets mark `drawn_as: hlod | not_resident`
- **Save games:** `.specsave` files hold the world, the hosted game, physics (with warm starts, sleep state and SAT
  memory) and module state, so a loaded game continues tick-identical; versioned with migrations, checksummed,
  bounded and fuzzed; decoded on a worker so loads never stall the engine. Tools `save_write`, `save_read`, `save_list`,
  `save_delete`. Sepulcra gains a Saved games screen (three slots, two rotating autosaves, pad/mouse/touch/keys)
- **Balance lab:** new crate `specula-balance` (targets as data, deterministic multi-threaded sweeps of Lex rules, a
  seeded Gaussian-process tuner, several play styles at once) and tools `balance_targets`, `balance_sweep`,
  `balance_tune`; `scripts/balance_playtests.py` cross-checks tuned points with real playtests
- **Visual history gallery:** `docs/gallery/` (55 entries from the Bevy era to today, by subject and by date, with
  honest dates and notes), `scripts/gallery.py` (add, check, dupes, drop), the specula-core `thumb` example (zero-dep
  downscaling to JPEG) and a JPEG known-answer test against an independent decoder
- **Headless playtests and swarms:** playtests as data for any game (`playtest_run`, `playtest_report`: steps that call
  any tool at a tick, expectations, `until`, replays verified by checkpoint hashes of the world, physics and the
  game's subsystems), `--paused`, `scripts/playtest_swarm.py` and `scripts/playtest_suite.py` (26 Sepulcra playtests
  across parallel engines, every replay verified)
- **Tool ergonomics:** heavy tools answer concisely unless asked (`response_format`, per call, per agent or per
  engine with `--response-format`), every answer is compact JSON and every error is `{error, message}`; audit in
  `docs/design/TOOLS.md`
- **Physics P1-P3:** new crate `specula-physics` (std only): spheres, capsules, boxes, convex hulls, static meshes
  and heightfields; a two-tree broadphase; GJK and SAT with per-pair feature memory; raycasts, shape casts, overlaps
  and triggers; a character controller (steps, slopes, ground snap, moving platforms, pushing); Soft Step rigid
  bodies with sleeping islands, a tunnelling guard and a 2D mode. Engine: collider and body components simulated on
  the fixed tick and journaled, `world_env {physics}`, tools `physics_raycast`, `physics_cast`, `physics_overlap`,
  `physics_contacts`, `physics_events`, `physics_info`, `physics_explain`, `physics_check`, `physics_impulse`, sight
  overlays `colliders` and `contacts`, a physics hash in `world_hash` and the desync trace. Existing worlds keep
  their files and hashes. `specula-model` gains `collider::fit_limited` (moving hulls are capped at 16 points)
- **Determinism hardening:** module loads, unloads and their build hashes are journaled (`Kind::Module`) and
  checked on replay and in saves; the build hash now covers the module's sources exactly, so rebuilding unchanged
  code gives the same hash (saves and replays written before 2026-10-22 report another build for the same code).
  The FP environment (MXCSR on x86, FPCR on aarch64) is checked and restored around module calls and before every
  agent command (`specula_sys::fpenv`). Determinism lints: one `clippy.toml` in each of the 10 simulation crates
  (no `HashMap`/`HashSet`, platform float maths or rounding, wall-clock time or threads), the same check on modules
  at `logic_build` (`determinism::*` diagnostics, bounded), `forbid(unsafe_code)` in every crate without FFI and
  workspace lints for SAFETY comments; `scripts/determinism_check.py`. The audit moved the remaining platform
  rounding in simulation and world-state code to `dmath`
- **Streaming fix:** a frame's first upload is always tried, and captures taken while no frame runs get their own
  budget, so a built HLOD proxy can no longer wait for its upload forever
- **Joints and ragdolls (P4, A4):** ball, hinge, distance and fixed joints with limits and motors; limp and powered
  ragdolls with hulls fitted from the rig, blending to and from animation, with a get-up; Sepulcra's shambler dies as
  a ragdoll. Ragdolls and world-anchored joints save and load bit for bit. Joint rules apply on every write path
  through a joint index. World scale is composed through each level's rotation. Restored entities never reuse a
  version, which fixes stale physics bodies after a force-undo; the record is pruned to what the journal can undo. An
  opt-in deterministic parallel physics solver (`--physics-threads`)
- **Project data through the protocol:** `project_*` tools give recipes, UI screens and themes, sounds, style files
  and module sources the claims, versions, journal (`Change::File`), undo and conflict help that entities have, with
  validation by kind and atomic disk sync (outside edits imported as `disk`, unreadable ones held). Mounts accept only
  real modules; manifests and compile-time paths are checked, the latter from rustc's dep-info after every build.
  `logic_build {fix: true}` commits fixes as the caller. Model hot reload now rebuilds every handle of a changed
  recipe (only the first was rebuilt)
- **Agent evals:** 21 tasks with outcomes the engine verifies (15 public, 6 held out), each with a reference and a
  broken solution plus 23 known cheats; `world_diff`, transcript, geometry, world-size and series verifiers;
  `scripts/evals.py` runs them across parallel engines, or with a real agent behind a proxy that enforces the task's
  tools and call limit. Results are scrubbed of private paths. Playtest matchers compare objects by key in any order
- **Privacy:** `crates/specula/tests/private_paths.rs` fails on any home-folder, temp-folder or session path, or a
  UUID, in a tracked file; `save_check.py` writes `<work>` instead of its temp folder
- **Streaming, memory ahead of need:** GPU memory blocks are allocated ahead on a worker thread and adopted at a
  flush, so a load no longer allocates on the frame (up to 164 ms before); the end of a pass is sliced into two steps
- **Streaming during loading:** the spatial index is refiled a slice at a time while the old cells serve (a big load
  used to force a 3-6 ms one-frame rebuild); upload batches use a fresh transient pool per slot, reset whole, with one
  barrier; mesh blocks grow 16-64 MB. Stalls inside the Intel driver's command recording are zoned and reported apart
  by `stream_check.py`. Alternating A/B on the iGPU: loading overruns lower than the earlier build in every pair
- **Streaming pump:** the eviction queue is scanned across frames into heaps instead of being rebuilt and sorted in
  the frame after each pass (2-3.5 ms spikes on the iGPU); a running resident total; profile zones for every pump
  stage. Alternating A/B on the iGPU: in-flight stutter 0 frames on every run, against 3-9 on the earlier build

### Pre-M3 cleanup (2026-10-17)

- Agent calls are served between presents (`schedule.rs`), not once per frame: one windowed agent gets 500-1,270
  calls/s instead of about 60-230; under a backlog the window keeps 60 fps. `perf_frame.serve`, `call_cost_ms`,
  `scripts/call_rate.py`, `swarm_drill.py --windowed`; `specula-platform` gains `timer::FineTimer` (winmm) and
  `refresh_rate_of`
- Sight: with `include_ui`, opaque HUD panels hide what's under them (`Gpu::render_with_ids_occluded`,
  `id_buffer.ui_occluders`, `MAX_ID_OCCLUDERS` / `Limit::IdOccluders`)
- The sky eases into the horizon instead of meeting it with a crisp line
- Sepulcra: selection and range-ring highlights use saturated rune cyan (the old pale mark rendered near-white)
- Docs: current README, HANDOFF and CHANGELOG; one open-owner-decisions list; tests for Markdown links and the MCP
  revision pin; CI rejects session links in new commit messages and runs clippy on Linux

### Added in M2 (2026-10-16): rendering, true sight, animation, first procedural shapes

- Render graph that owns barriers; PBR-lite and stylised materials; directional and point lights; shadow maps;
  post (tone mapping, FXAA, LUTs); sky; GPU instancing, frustum culling and LODs; `perf.toml` budgets
- 2D: sprites, tilemaps, vector shapes, orthographic cameras
- glTF/GLB import (meshes, materials, textures, skins, animations); `specula-anim` stages A1–A2 (GPU skinning, clips)
- `specula-model`: procedural modelling tier 1 (recipes, `model_build` / `model_reload`)
- Sight: ID buffer (`visible_px`, `occluded`, `screen_box_exact`), persistent named cameras, `sight_orbit`,
  `sight_compare`, `sight_query`, debug overlays in captures
- `specula-text`, `specula-ui`, `specula-input`, `specula-audio`
- *Sepulcra* vertical slice hosted through the `Game` hook, with `play_*` tools

### Added in M1 (2026-10-13): world model, multi-agent and determinism

- Text world files with crash-safe save/load and autosave; hierarchy and paths; tags, props, reserved physics
  components
- `world_batch`, `world_query`, per-entity versions (`expect_version`), claims (leases on subtrees and regions)
- Agent identity (`agent_hello`), agent-attributed journal with `world_history` and `world_undo`, `world_changes`
- Fixed-tick clock, seeded RNG streams, deterministic maths (`dmath`), `world_hash`, journal-replay test in CI
- `perf_frame` (GPU timestamps per pass), `time_control`; HTTP 503 with `Retry-After` at the connection limit;
  `scripts/swarm_drill.py` at 16 and 64 agents

### Added in M0 (2026-10-12)

- `specula-core`: JSON, math (Vulkan conventions), base64, CRC-32/Adler-32, DEFLATE/zlib, PNG encode/decode
- `specula-sys`: sole owner of `kernel32` imports; runtime library loader (`dynlib`)
- `specula-platform`: Win32 windows and input events
- `specula-gpu`: runtime-loaded Vulkan 1.3 renderer (dynamic rendering, synchronization2), headless targets +
  readback, window swapchain, validation capture
- `specula-protocol`: HTTP/1.1 server and MCP (Streamable HTTP, revision 2025-11-25) for agents
- `specula` world server: tools `world_spawn`, `world_list`, `world_delete`, `sight_frame`, `engine_info`
- CI on Windows and Linux; `Cargo.lock` must contain workspace crates only
- `world_set` (change an entity in place); `sight_frame` `max_subjects`, `distance_m`, camera `near_m`/`far_m`
- `scripts/window_check.py`: window-path regression check on every GPU
- Engine: window on its own thread; per-run capture folders (`--captures`); Ctrl+C / console close shut down cleanly

### Fixed (M0 adversarial review)

- Top-down cameras, large worlds and partly-behind-camera subjects in sight; unbounded target cache; stale timed-out
  commands; capture overwrites; unvalidated huge inputs; session expiry (see docs/ENGINE_PLAN.md)

### Removed

- All Bevy-era crates, templates, demo games and skills (preserved at tag `bevy-tooling-final`; docs in
  `docs/archive/bevy-era/`)

## [0.2.x] — Bevy era (as Grok-Bevy)

### Added

- **MCP query aliases:** `Name` / `Transform` / `GlobalTransform` → Bevy 0.19 FQNs (`component_paths`)
- **`bevy_env_check`** stamps `specula_version`, `server_binary`, `reload_hint` (stale install detection)
- Launch spawn message includes **child `pid=`**
- send_keys movement smoke recipe in agent-loop + TROUBLESHOOTING

### Changed

- Capture tool description is 2D/3D neutral (primary window)
- Default `bevy_brp_query` components: Name + Transform (via aliases)

### Previously unreleased (prior)

- **G6 install ergonomics:** templates embedded in `specula` via `include_dir`; scaffold falls back to cache extract without monorepo / `SPECULA_TEMPLATE_ROOT`
- **Optional `physics` feature** on 2d/3d templates and demos (`avian2d` / `avian3d` **0.7**); default features unchanged

## [0.2.0] — 2026-07-19

### Added

- **External greenfield dogfood fixes (Wave A/B):**
  - Template/demo `AssetPlugin` dual-mode: debug `CARGO_MANIFEST_DIR/assets`, release relative `assets/`, env `BEVY_ASSET_ROOT`
  - Loading fail-forward timeout (~12s) so stuck asset loads do not block BRP forever
  - MCP **`bevy_wait_brp`**; **`bevy_launch_app`** non-blocking by default (`wait_secs=0`), sets package cwd
  - `docs/PHYSICS.md` + skill pins: **avian2d/avian3d 0.7** for Bevy 0.19 (kits remain transform-based)
  - TROUBLESHOOTING: BRP method table, launch/wait, B0001, template root, Loading stuck
  - Scaffold/template `AGENTS.md`: asset root + B0001 notes
  - `mcp-config` prints `SPECULA_TEMPLATE_ROOT` when templates are discoverable

- **Game factory (v0.3 / alpha) G1–G4:**
  - `docs/GAME_DOD.md`, `docs/ROADMAP.md`, `docs/PACKAGING.md`
  - Skills `bevy-demo-game`, `bevy-package`; MCP demo/package workflows
  - Templates `game-2d` / `game-3d` are short demos (collect + hazard + Victory/GameOver)
  - In-repo dogfood: `games/demo-2d`, `games/demo-3d` (`cargo run -p demo_2d|demo_3d --features remote,capture`)
  - `scripts/package-demo.sh` — release binary + `assets/` → `dist/` (+ zip)

- **Production games (v0.2):** skill pack, templates, multi-kind scaffold, ship docs, MCP routing
  - `.grok/skills/bevy-production` / `bevy-2d-game` / `bevy-3d-game` / `bevy-agent-loop`
  - `templates/game-2d` and `templates/game-3d` playable vertical slices (states, movement, disk assets)
  - `specula scaffold --kind 2d|3d|demo` copies templates (source of truth) + project `AGENTS.md`
  - `docs/ASSET_CONVENTIONS.md`, `docs/SHIPPING.md`, `docs/PRODUCTION_GAMES.md`
  - MCP `initialize.instructions` covers production skills and scaffold kinds
- MCP **prompts**: `start_2d_game`, `start_3d_game`, `iterate_scene`, `prepare_ship` (`prompts/list` + `prompts/get`)
- MCP tool **`bevy_workflow`**: goal → ordered skills + tools/CLI steps (`new_2d`, `new_3d`, `verify_scene`, `ship`, `add_sprite`)
- Root **`AGENTS.md`**; README production section; PROGRESS v0.2 checklist

### Changed

- Workspace crate version **0.2.0** (aligns package with PROGRESS v0.2/v0.3 narrative)
- Default scaffold kind is **`2d`** (production); `demo` is the BRP cube fixture (`templates/sample-app`)
- Positioning: `templates/sample-app` is BRP **integration fixture**, not a production game template
- `bevy_workflow` documented as **router, not autopilot**

## [0.1.0] — 2026-07-17

### Added

- **`specula` CLI** with:
  - `doctor` / `env-check` — cross-platform Bevy readiness (Rust/Cargo, OS guidance, optional compile probe)
  - `mcp` — stdio MCP server for Grok Build agents
  - `mcp --delegate-brp-mcp` — exec into installed `bevy_brp_mcp`
  - `scaffold` — generate a BRP-enabled Bevy sample
  - `brp` — discover / query / mutate / screenshot / wait helpers
  - `compat` and `mcp-config` — version matrix and Grok registration snippets
- **`specula-env`** — injectable, unit-tested environment detection library
- **`specula-brp`** — BRP HTTP client, named targets, PNG capture → MCP image adapter
- **`templates/sample-app`** — Bevy 0.19 scene with `remote` / `capture` features and headless BRP smoke binary
- Docs: README fast-start, CONTRIBUTING, troubleshooting, multi-platform CI
- Dual MIT / Apache-2.0 licensing (matching Bevy)

### Compatibility

| Bevy | bevy_brp_mcp | bevy_brp_extras |
|------|--------------|-----------------|
| 0.19 | 0.22.1       | 0.22.1          |

## Project history

How the project got here, in decisions (moved from HANDOFF.md, which now covers only the current state).

| When | What |
|---|---|
| Before 2026-10 | The owner built **Grok-Bevy**: Bevy-based tooling that gave AI agents "eyes" on 3D scenes. The many sight plans and findings from that era are in `docs/archive/bevy-era/`. |
| 2026-10-12 | Claude assessed the repo. It was renamed **Specula** (Latin: *watchtower*). |
| 2026-10-12 | **Pivot.** The owner wanted the engine fully native and zero-dependency, with no Bevy. The Bevy code is preserved at tag `bevy-tooling-final`. |
| 2026-10-12 | Choices made: **Vulkan**, cross-platform with Windows first, **hot-reloaded Rust modules** for game logic, **2D and 3D** from the start, and the Specula repo becomes the engine. |
| 2026-10-12 | **M0 built** by 4 parallel subagents (one per crate, in git worktrees), with Claude integrating. Then two full audits and three adversarial review rounds; every finding was fixed. |
| 2026-10-12 | **M0 complete** at commit `badb857`. CI is green on windows-msvc, windows-gnu and ubuntu. |
| 2026-10-13 | ROADMAP.md and HANDOFF.md written. |
| 2026-10-13 | Engine survey and UI design written. The roadmap gained a **path to production**: v1.0 = a commercial PC game on Steam for Windows and Steam Deck; Phase A M1 → Phase B playable slice (lanes built in parallel by subagent swarms) → Phase C production → Phase D ship. |
| 2026-10-13 | The owner accepted **every** Claude proposal: physics, skeletal animation, headless playtests, batch edits, procedural modelling, review queue, swarm track. The swarm track became primary, foundations F1–F5 were added, and Claude researched and wrote the four design docs in `docs/design/`. Next: the owner's game project, then M1. |
| 2026-10-13 | **Lava-lamp fidelity benchmark** (the owner's first asset challenge): three lamps from basic to "as real as possible", built in the engine. It added per-frame GPU data (buffer device address), materials, point lights, glass, blob shadows, lathe meshes, tone mapping and a ray-marched lava lamp. Run it with `python scripts/lava_lamps.py`. The NVIDIA and Intel captures match within 3/255. |
| 2026-10-13 | **Swarm session (owner: "advance through M1 and M2 while building the TD game").** Wave 1, all reviewed then merged: capture speed (parallel deterministic PNG, ~25x faster), GPU (frame stats, instancing, culling, pipeline warm-up and cache), World (hierarchy, versions, batch, query, crash-safe text saves), determinism (`dmath`, correctly rounded f32; Android CI check), Sepulcra's headless simulation. Then the engine got a `Game` hook, a fixed-tick clock, `perf_frame` and `time_control`. Wave 2 lanes: render (shadows, post, sky, overlay), text, procedural modelling, input, the Sepulcra app, journal/claims. |
| 2026-10-13 | **First game chosen:** the tower defence *Sepulcra* (over a cozy castle builder and a squad RPG). Platforms: Windows + Steam Deck; mobile later, so foundation **F6 (portable by design)** was added. A **Fidelity track** was added to the roadmap: benchmark assets rebuilt at each tier, from greybox up to Unreal-class realism. |
| 2026-10-13 | **M1 complete:** world model, claims, journal and determinism; the exit proof had 4+ agents building separate districts at once. |
| 2026-10-16 | **M2 complete:** rendering, the ID buffer and sight tools, 2D, glTF, animation A1-A2, procedural modelling tier 1, and the Sepulcra slice integrated. |
| 2026-10-17 | M3 planned from the agent game-making research (Lex, Locus, animation A3-A4, physics P1-P4 and more), waiting for the owner's go-ahead. Pre-M3 cleanup approved from an outside review. |
