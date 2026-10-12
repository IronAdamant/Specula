# Specula engine — plan

**Specula** (Latin: *watchtower*) — a **zero-dependency, agent-native game engine and editor in Rust**.
Multiple AI agents (and humans) connect to one live world and build different parts of it in real time.

Supersedes [SPECULA_PLAN.md](SPECULA_PLAN.md) (Bevy-era plan). Bevy-era code is preserved at tag
**`bevy-tooling-final`** (`68c9d87`).

---

## Decisions (2026-10-12)

| Decision | Choice | Notes |
|---|---|---|
| Dependencies | **Rust `std` only — no third-party crates** | Still depends on the OS and GPU driver (Win32, `vulkan-1.dll`, …). Dev-time tools (Vulkan SDK, `glslc`) allowed; nothing third-party ships. |
| Graphics | **Vulkan**, behind a thin render-hardware interface | Explicit state, validation layers, native headless, multithreaded command recording. Apple later via native Metal backend (not MoltenVK, to stay zero-dep). |
| Platforms | **Cross-platform**, Windows first | Windows (Win32) → Linux (X11/Wayland) → macOS (Cocoa via objc runtime FFI + Metal). |
| Game logic | **Hot-reloaded Rust modules** | Engine owns all state; modules are reloadable code behind a C ABI. Zero deps → module builds in seconds. |
| Dimensions | **2D and 3D from the start** | One render graph; 2D = orthographic cameras + sprite/tile/vector pipelines. |
| Repo | Specula becomes the engine | `IronAdamant/Specula`, branch `main`. |

## Principles

1. **The agent protocol is the primary interface.** Agents and humans are equal clients; the human editor is built *on* the
   protocol, not the other way round.
2. **The game tells, the tool asks, the model judges.** Ground truth (subjects, bounds, screen rects) comes from the engine.
3. **Every change is a command** — logged with agent id, applied at frame boundaries, undoable, replayable.
4. **Concurrency is designed in**: claims, per-entity versions, subscriptions. Two agents never silently overwrite each other.
5. **Sight is first-class**: per-agent offscreen cameras, same-frame capture, exact bounds, time control.
6. **Text-first persistence**: worlds diff and merge in git; agents may also edit files directly.
7. **Structured errors everywhere** (protocol, validation layers, shader compile, module build) — agents debug from data.
8. **Zero third-party crates**, enforced by CI.

## Architecture

```
 Claude Code ─┐                       ┌── human window (a view, not the owner)
 Grok Build  ─┼─ MCP over HTTP ──► specula (world server) ──► Vulkan ──► offscreen agent cameras
 more agents ─┘   localhost              │                              └► PNG captures
                                         ├─ world: entities · commands · journal · claims · versions
                                         ├─ logic host: hot-reloaded Rust modules (C ABI)
                                         └─ persistence: text world files (git-friendly)
```

### Crates (all `std`-only)

| Crate | Role |
|---|---|
| `specula-core` | math, ids, JSON, PNG (inflate/deflate), CRC/Adler, logging, time |
| `specula-platform` | windows, input, events, timers — Win32 first |
| `specula-gpu` | hand-written Vulkan FFI subset + render-hardware interface |
| `specula-render` | render graph; 2D (sprites/tiles/vector) and 3D (meshes/materials/lights/shadows) pipelines; agent cameras |
| `specula-world` | entities, components, commands, journal, claims, versions, subscriptions, text persistence |
| `specula-protocol` | HTTP/1.1 server, JSON-RPC, MCP (tools/resources/notifications), agent sessions |
| `specula-assets` | glTF, PNG, later audio formats + TrueType |
| `specula-logic` / `specula-sdk` | module host (C ABI, hot reload) / SDK crate game modules depend on |
| `specula` (bin) | world server + editor; `specula export` builds a standalone game without the agent/editor layers |

M0 may start with fewer crates and split as they grow.

### Agent protocol (MCP over HTTP)

- Engine hosts MCP (Streamable HTTP subset) on `127.0.0.1`; each connection is a session with an agent id.
- Claude Code: `claude mcp add --transport http specula http://127.0.0.1:<port>/mcp`.
- Tool families: `world_*` (spawn, set, delete, query, claim, release, undo), `sight_*` (frame, subjects, orbit, camera),
  `time_*` (pause, step, resume), `logic_*` (build, reload, errors), `asset_*` (import, studio). Port 7471.
- Subscriptions via MCP notifications: entity changes inside an agent's watched subtree/region.

### Concurrency model

- Entity ids: stable `u64` + readable path (`/world/village/house_03`).
- **Claims** on subtrees or spatial regions; writes outside your claim are rejected (or require a co-claim).
- **Optimistic versions**: each write names the version it read; stale writes are rejected with the current state.
- **Journal**: append-only, agent-attributed; powers undo, replay, determinism checks, "who built this".

## Milestones

Each milestone ends with something agents can use.

### M0 — Loop end-to-end (Windows, Vulkan)

- [x] Workspace reset: remove Bevy-era crates from `main` (preserved at `bevy-tooling-final`); CI zero-dep check
      (`Cargo.lock` must contain only workspace packages)
- [x] `.cargo/config.toml`: `[target.x86_64-pc-windows-gnu] rustflags = ["-C", "link-self-contained=yes"]` — link with the
      Rust toolchain's own linker only (proven 2026-10-12; lands with the workspace reset because Bevy-era `tempfile` needs `ktmw32`)
- [x] All OS/driver FFI via `#[link(name = "…", kind = "raw-dylib")]` — no import libraries from anyone
      (proven: `vulkan-1` raw-dylib → `vkEnumerateInstanceVersion` = 1.4.341, no SDK)
- [x] Crate contracts (public APIs as stubs) for core, platform, gpu, protocol; `specula-core::json` implemented
- [x] `specula-core`: math, base64, CRC32/Adler32, DEFLATE (real compression) + inflate, PNG encode + decode
- [x] Win32 window + message loop (raw FFI)
- [x] Vulkan: instance, debug messenger (validation when SDK present), physical-device selection (list both GPUs),
      device/queues, offscreen render target, Win32 surface + swapchain
- [x] One pipeline: unlit coloured mesh; SPIR-V built with `glslc` at dev time, checked in
- [x] Readback → PNG
- [x] HTTP server + MCP subset: `initialize`, `tools/list`, `tools/call`
- [x] Tools: `world_spawn` (cube/quad, transform, colour), `sight_frame` (PNG + subject list) — underscores: Claude Code
      rejects dots in tool names
- [x] Proof: two MCP sessions (Python client) built one scene in the live engine; and **Claude Code itself**
      (`claude -p --mcp-config .mcp.json`, 2026-10-12) connected over HTTP, called engine_info / world_spawn /
      sight_frame, saw the frame and confirmed the cube's rect within 1–2 px; 0 validation errors incl. shutdown
- [x] `specula-sys`: single owner of kernel32 imports (windows-gnu drops a second crate's raw-dylib imports of the same DLL)

**M0 findings to carry forward**
- Occlusion: `on_screen` ≠ visible (a crate hidden behind the tower still reports a full rect) → M2 ID buffer with
  per-entity visible-pixel counts.
- MCP: revision 2026-07-28 drops `initialize`/sessions; we serve 2025-11-25 (+06-18, 03-26). Revisit when clients move.
- Sphere rects are loose (projected from the bounding cube's corners: ~7 px padding on a 30 px orb) → shape-aware
  bounds or ID-buffer extents in M2. Independent skeptic check: all six rects within 1–2 px of a fresh projection.
- Auto-framing fits the bounding box diagonal, so flat wide scenes look small in frame; tighten in M2.

### M1 — World and multi-agent

- [ ] Entity store, components, command queue applied at frame boundary
- [ ] Journal + undo/replay; sessions with agent ids
- [ ] Claims, versions, conflict errors; subscriptions/notifications
- [ ] Text persistence (load/save world)
- [ ] Proof: two agents build disjoint parts of one world concurrently; a conflicting write is rejected cleanly

### M2 — Rendering

- [ ] Render graph; 3D meshes, PBR-lite materials, directional/point lights, shadow maps
- [ ] 2D: sprites, tilemaps, vector shapes; ortho cameras
- [ ] PNG decode (inflate); glTF import (meshes, materials, textures)
- [ ] Per-agent offscreen cameras; `sight.subjects` exact screen rects; `sight.orbit`

### M3 — Logic and time

- [ ] Module host (C ABI), `specula-sdk`; build → reload without losing world state; structured build errors
- [ ] Input (keyboard/mouse/gamepad via Win32/XInput); `time.pause/step/resume`; fixed-timestep simulation

### M4 — Asset studio

- [ ] Stage world (neutral backdrop, fixed lighting, scale refs); contact sheets; stats; lineup/before-after
- [ ] `style.toml` hard checks (budget, scale, palette)

### M5 — Audio, text, UI

- [ ] WASAPI output + mixer; WAV (then OGG/Vorbis decode)
- [ ] TrueType rasterizer + text rendering; immediate-mode UI (used by the human editor)

### M6 — Ship

- [ ] `specula export` → standalone game executable + assets, agent/editor layers stripped
- [ ] Then: the original game

### Platform track (after M2, in parallel)

- [ ] Linux: X11 (then Wayland wire protocol), Vulkan surface
- [ ] macOS: Cocoa via objc runtime FFI; Metal backend behind the render-hardware interface

## Dev environment

- GPU: NVIDIA RTX 4090 Laptop + Intel UHD (two vendors to test device selection). `vulkan-1.dll` present.
- **Needed:** LunarG Vulkan SDK (validation layers, `glslc`) — dev-time only.
- Rust: toolchain `stable-x86_64-pc-windows-gnu`. **Linker = the one bundled in the Rust toolchain** (`-C link-self-contained=yes`),
  so LLVM-MinGW/WinLibs on PATH and MSVC are irrelevant. Own linker: optional long-term track, not before M0.
- Vulkan loader on this machine reports 1.4.341; SDK to install: 1.4.363.0 (core only, no optional libraries).

## Risks

| Risk | Mitigation |
|---|---|
| Scale (engine is multi-year) | Milestones each usable by agents; game starts at M3, grows with the engine |
| Vulkan synchronization bugs | Validation + sync validation in debug; small render graph owns barriers |
| Hot-reload ABI fragility | Engine owns all state; C ABI boundary; modules stateless between reloads |
| Hand-written FFI mistakes | Bind only what we use; validation layers; unit tests on struct layouts |
| Shader compiler is a dev-time tool | Accept for now; own shading language → SPIR-V later |
| Apple without MoltenVK | Native Metal backend behind the same interface |

| Date | Change |
|---|---|
| 2026-10-12 | Engine direction decided; plan created |
| 2026-10-12 | M0 built by 4 parallel subagents + integration; end-to-end MCP proof passed |
| 2026-10-12 | M0 audit: full read of every crate; clippy clean; Claude Code proof; packets saved beside PNGs; GPU teardown checked |
