# Specula engine — plan

**Specula** (Latin: *watchtower*) — a **zero-dependency, agent-native game engine and editor in Rust**.
Multiple AI agents (and humans) connect to one live world and build different parts of it in real time.

Supersedes [SPECULA_PLAN.md](archive/bevy-era/SPECULA_PLAN.md) (Bevy-era plan). Bevy-era code is preserved at tag
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
9. **Swarm-first** (2026-10-13). The goal is for agent swarms to take a game from design doc to shipped build, letting
   solo devs and small studios compete with large companies. Humans edit by hand only if they want to.
10. **Deterministic simulation**: fixed tick, seeded RNG, own maths functions, state hashes
    ([design/DETERMINISM.md](design/DETERMINISM.md)).
11. **Efficient by default**: games must run well on integrated GPUs and handhelds. Performance is visible to agents
    and budgeted; the Intel iGPU is the reference low-end device.

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

The checklist, with the reasoning for each step, is in **[ROADMAP.md](ROADMAP.md)**: M0 (done), M1 world and
multi-agent (next), M2 rendering, M3 logic/physics/time, M4 asset studio, M5 audio/text/UI, M6 ship, plus platform
and swarm tracks. This file keeps the decisions, architecture and the record of what was found along the way.

## M0 record (2026-10-12)

**M0 adversarial review (2026-10-12) — fixed**
- Top-down / bottom-up cameras rendered blank (`look_at_rh` degenerate) → fallback up axis (north up); `eye == target`
  rejected
- Fixed 2 km far plane blanked large worlds while sight reported them on screen → **reversed-Z, infinite far
  plane**, fixed 0.05 m near (a first attempt that fitted near/far to the world let a distant "sun" push the near
  plane out to 10 m — caught on re-review); depth order tested in both modes on the GPU
- Subjects partly behind the camera (a floor under a first-person camera) reported off screen → boxes clipped against
  near/far planes; `nearest_depth` (0..1) replaced by `distance_m`
- Offscreen targets cached per size forever (51 MB each) → LRU of 4
- Tool calls stalled while the window was dragged → window pumped on its own thread
- Timed-out calls were still applied later → engine drops commands past their deadline (25 s < 30 s caller timeout)
- Captures overwritten across runs → per-run folders `captures/run-<secs>-<pid>/`, newest 20 finished runs kept
  (exact `run-<digits>-<digits>` names only, never a running engine's, pruned only after the port bind), `--captures`
- 2000-entity frames returned 458 KB packets → response holds the `max_subjects` most relevant (default 50); full list
  in the saved packet
- Unbounded positions/scales (1e30, scale 1e39) broke the camera → validated (±1e6 m, scale 0.001..1e5), names ≤ 128
  chars without control characters; ids only consumed by successful spawns
- `window_check.py` could write into a live engine on 7471 → own port 7491 and captures folder
- CI never built the windows-gnu dev toolchain → `windows-gnu` CI job with the self-contained linker
- Disconnected agents listed forever → sessions idle > 30 min hidden from listings (still valid if the agent
  returns); non-finite JSON-RPC ids rejected
- Session cap (1024) evicted the oldest-*created* session, so an active agent could be pushed out → evicts the
  least recently used (round-3 finding)
- A second engine on a taken port flashed a window → port bound before the window opens
- `world_set` with both `id` and `name` silently ignored `name` → rejected, pointing to `rename`
- `world_set` (change an entity in place, id kept) pulled forward from M1 for game work

**M0 follow-ups (done 2026-10-12)**
- `zlib_decompress`/`decompress` capped at `DEFAULT_MAX_OUTPUT` (256 MiB); `_with_limit` variants for other caps
- Ctrl+C / Ctrl+Break / console close shut the engine down cleanly in both modes (`specula_sys::interrupt`)
- GPU failure paths: `run_sync` drains the queue and resets its fence on any wait failure; a failed window
  submit replaces the frame's fence + semaphore and rebuilds the swapchain
- Echo example uses `specula_core::base64`

## Dev environment

- GPU: NVIDIA RTX 4090 Laptop + Intel UHD (two vendors to test device selection). `vulkan-1.dll` present.
- LunarG Vulkan SDK 1.4.363.0 installed at `C:\VulkanSDK\1.4.363.0` (validation layers, `glslc`) — dev-time only.
- Rust: toolchain `stable-x86_64-pc-windows-gnu`. **Linker = the one bundled in the Rust toolchain** (`-C link-self-contained=yes`),
  so LLVM-MinGW/WinLibs on PATH and MSVC are irrelevant. Own linker: optional long-term track, not before M0.
- Vulkan loader on this machine reports 1.4.341 (RTX 4090) / 1.3.271 (Intel).

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
| 2026-10-12 | M0 complete after 3 adversarial review rounds (`badb857`) |
| 2026-10-13 | Milestones moved to ROADMAP.md (checkboxes + reasoning, swarm track, Claude's **(new)** proposals); HANDOFF.md added |
| 2026-10-13 | Owner accepted every proposal; swarm track made primary; foundations F1–F5 (swarm-first, determinism, low-end performance, physics/animation designed in, humans optional); design research in `docs/design/` |
| 2026-10-13 | Engine survey (`docs/research/ENGINE_SURVEY.md`) + UI design; gaps added to the roadmap (➕); path to production (phases A–D, v1.0 = PC game on Steam for Windows + Steam Deck) |
| 2026-10-13 | Rust track: `docs/research/RUST_AT_SCALE.md` (measured build baseline; forbid unsafe outside FFI crates; SAFETY and determinism as lints; compiler diagnostics as data; build service) |
