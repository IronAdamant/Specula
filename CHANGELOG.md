# Changelog

All notable changes to Specula are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — 0.3.0: native engine, M0–M2

Specula is now a zero-dependency, agent-native game engine (Rust `std` + OS/driver FFI only).
See [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md). Item-by-item detail, with the reasoning for each, is in
[docs/ROADMAP.md](docs/ROADMAP.md); the timeline of decisions is under [Project history](#project-history) below.

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
| 2026-10-13 | ROADMAP.md and this handoff written. |
| 2026-10-13 | Engine survey and UI design written. The roadmap gained a **path to production**: v1.0 = a commercial PC game on Steam for Windows and Steam Deck; Phase A M1 → Phase B playable slice (lanes built in parallel by subagent swarms) → Phase C production → Phase D ship. |
| 2026-10-13 | The owner accepted **every** Claude proposal: physics, skeletal animation, headless playtests, batch edits, procedural modelling, review queue, swarm track. The swarm track became primary, foundations F1–F5 were added, and Claude researched and wrote the four design docs in `docs/design/`. Next: the owner's game project, then M1. |
| 2026-10-13 | **Lava-lamp fidelity benchmark** (the owner's first asset challenge): three lamps from basic to "as real as possible", built in the engine. It added per-frame GPU data (buffer device address), materials, point lights, glass, blob shadows, lathe meshes, tone mapping and a ray-marched lava lamp. Run it with `python scripts/lava_lamps.py`. The NVIDIA and Intel captures match within 3/255. |
| 2026-10-13 | **Swarm session (owner: "advance through M1 and M2 while building the TD game").** Wave 1, all reviewed then merged: capture speed (parallel deterministic PNG, ~25x faster), GPU (frame stats, instancing, culling, pipeline warm-up and cache), World (hierarchy, versions, batch, query, crash-safe text saves), determinism (`dmath`, correctly rounded f32; Android CI check), Sepulcra's headless simulation. Then the engine got a `Game` hook, a fixed-tick clock, `perf_frame` and `time_control`. Wave 2 lanes: render (shadows, post, sky, overlay), text, procedural modelling, input, the Sepulcra app, journal/claims. |
| 2026-10-13 | **First game chosen:** the tower defence *Sepulcra* (over a cozy castle builder and a squad RPG). Platforms: Windows + Steam Deck; mobile later, so foundation **F6 (portable by design)** was added. A **Fidelity track** was added to the roadmap: benchmark assets rebuilt at each tier, from greybox up to Unreal-class realism. |
| 2026-10-13 | **M1 complete:** world model, claims, journal and determinism; the exit proof had 4+ agents building separate districts at once. |
| 2026-10-16 | **M2 complete:** rendering, the ID buffer and sight tools, 2D, glTF, animation A1-A2, procedural modelling tier 1, and the Sepulcra slice integrated. |
| 2026-10-17 | M3 planned from the agent game-making research (Lex, Locus, animation A3-A4, physics P1-P4 and more), waiting for the owner's go-ahead. Pre-M3 cleanup approved from an outside review. |
