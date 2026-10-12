# Changelog

All notable changes to Specula are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — 0.3.0: native engine, M0

Specula is now a zero-dependency, agent-native game engine (Rust `std` + OS/driver FFI only).
See [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md).

### Added

- `specula-core`: JSON, math (Vulkan conventions), base64, CRC-32/Adler-32, DEFLATE/zlib, PNG encode/decode
- `specula-sys`: sole owner of `kernel32` imports; runtime library loader (`dynlib`)
- `specula-platform`: Win32 windows and input events
- `specula-gpu`: runtime-loaded Vulkan 1.3 renderer (dynamic rendering, synchronization2), headless targets +
  readback, window swapchain, validation capture
- `specula-protocol`: HTTP/1.1 server and MCP (Streamable HTTP, revision 2025-11-25) for agents
- `specula` world server: tools `world_spawn`, `world_list`, `world_delete`, `sight_frame`, `engine_info`
- CI on Windows and Linux; `Cargo.lock` must contain workspace crates only

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
