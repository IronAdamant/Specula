# Specula roadmap: M0 onwards

The plan for building Specula, a zero-dependency, agent-native game engine. Each step has a checkbox and the reason it
is there. Decisions, architecture and risks are in [ENGINE_PLAN.md](ENGINE_PLAN.md). The context for a new conversation
is in [HANDOFF.md](../HANDOFF.md). Design research is in [design/](design/), and the engine comparison is in
[research/ENGINE_SURVEY.md](research/ENGINE_SURVEY.md). Items marked ➕ were added by that survey (2026-10-13).

**Legend:** `[x]` done and verified · `[ ]` to do.

Everything in this file was agreed with the owner on 2026-10-13. That includes the items Claude proposed: physics,
skeletal animation, headless playtests, batch edits, procedural modelling, the review queue and the swarm track.
**Nothing is optional.** This is a long-term project, and the owner accepts how long the road to production is.

---

## The owner's direction (2026-10-13)

> "This is meant to allow games to be iterated rapidly via AI agent swarms."
>
> "For devs that just want AI agents such as yourself and your swarm to complete entire game... This is it."
>
> "I'd like it to make it easier to make games that are actually optimised to run on lower platforms, as too many games
> are made through Unreal Engine and it's caused a lot of efficiency issues."

What that means for the engine:
- **Swarm-first.** An agent swarm must be able to take a game from design doc to shipped build. The swarm track is the
  **primary** track, and every milestone serves it.
- **Humans are optional, never locked out.** Devs edit art or anything else only if they want to, with a Rust UI they
  can click, point, draw and drag in (3D gizmos included). Every human edit goes through the same protocol as agents,
  so it is journaled and undoable too.
- **Efficient by default.** Games made with Specula should run well on modest hardware: integrated GPUs and handhelds.
  The engine treats performance as something agents can see and budgets they must meet.
- **Taste is part of the product.** The engine gives agents a structured way to apply taste (a style bible, contact
  sheets, critique loops) and gives the owner the final say.

## Why this order: what a swarm needs

| # | What a swarm needs | Without it | Where it lands |
|---|---|---|---|
| 1 | **See the work.** Agents check what they built against ground truth. | Agents report success on blank or broken frames. | M0 (sight); M2 (occlusion, exact bounds) |
| 2 | **Edit safely in parallel.** Many agents in one world, none overwriting another. | Swarms corrupt each other's work; humans referee. | **M1** |
| 3 | **Iterate fast.** Change logic, reload and test in seconds; playtest headless at scale. | Agents wait on rebuilds; QA is manual. | M3 |
| 4 | **Produce content at volume.** Assets and levels built and checked by agents. | The art pipeline needs a large team. | M2 (tier 1), M4 |
| 5 | **Cheap human direction.** One person sets taste and rules; agents are held to them. | The human reviews everything and becomes the bottleneck. | M4 (style, review queue), M5 (editor) |

---

## Path to production: how to get there fastest

**Production v1.0 means:** a commercial PC game, built mostly by agent swarms on Specula, shipped on Steam for Windows
and Steam Deck (Linux), passing Steam Deck Verified (full controller support, legible text, at least 30 fps at 800p).
Consoles, mobile, web and multiplayer come after v1.0 (see the engine survey, §4).

**Milestones are feature groups, not a strict sequence.** After M1, the work splits into **lanes** that subagent
swarms build in parallel, because each lane is its own crate behind a contract. The **phases** below set what must be
true before moving on. Each phase takes only the slice of each lane that the owner's game needs at that point.

```
Phase A  Foundation          M1 (sequential: world, claims, journal, determinism, perf_frame)
            │
Phase B  Playable slice      ┌ Render lane    M2 basics: materials, shadows, post, instancing, text atlas
            (lanes in        ├ Sim lane       M3 core: module host, SDK, input actions, P1–P2 physics, saves
             parallel)       ├ Anim lane      A1–A2
            │                ├ Content lane   procedural tier 1, stage, contact sheets
            │                ├ Interface lane UI T1–T3, menus, WAV audio
            │                └ Swarm lane     task board, role templates, playtest swarms
            │   Gate: a 10–15 minute playable slice of the owner's game, within budget on the integrated GPU,
            │         with a headless replay suite passing
Phase C  Production          Every lane to the game's full needs: nav/AI, P3–P4, A3–A4, tiers 2–4, GI baking,
            │                VFX, streaming, audio buses, editor, localisation, accessibility, Linux/Deck
            │   Gate: content-complete game; budgets met on the iGPU and the Deck
Phase D  Ship                export, packaging, Steam (owner decision), Deck Verified, soak tests, release
```

**What makes it fast:**
1. **The game is the forcing function.** Build each feature only to the depth the game needs, without breaking a
   foundation. The first game should be scoped to the engine's strengths: stylised, shape-driven art, procedural
   environments, single-player, Latin-script languages first. That alone removes years of work: no complex shaping,
   no netcode, no photoreal pipeline.
2. **Swarms build the engine, not just the game.** Each lane is a crate with a contract, as in M0 (4 subagents in
   worktrees). Phase B runs 5–6 lanes at once, and an independent reviewer agent attacks each lane before its gate.
3. **Verification is automatic.** Headless tests, replay hashes, `perf_frame` budgets and `ui_check` let agents prove
   their work without a human in the loop for every change. The owner reviews at gates and in the review queue.
4. **Everything is data.** Recipes, worlds, animation graphs, UI screens, effects and string tables are all text
   files. Agents edit text far faster and more reliably than binary assets.

**Owner decisions that affect speed:**
- [ ] **First game's scope:** genre, art style, size and languages. This sets every lane's depth for Phase B.
  Criteria, questions and scoring: [GAME_SELECTION.md](GAME_SELECTION.md). **Genre chosen 2026-10-13:** a 3D tower
  defence on a shattered slab, working title *Sepulcra* ([games/sepulcra/DESIGN.md](../games/sepulcra/DESIGN.md)).
  Art style and launch languages are still open.
- [ ] **Steamworks** (`steam_api64.dll`, runtime-loaded like Vulkan): allowed as an optional platform service? It's
  needed for achievements, cloud saves and Steam Input glyphs.
- [x] **v1.0 platforms:** Windows + Steam Deck (owner, 2026-10-13). macOS stays on the platform track for after v1.0.
  **Mobile (Android, iOS) comes later**, after v1.0, once the owner is ready to learn it. Foundation F6 keeps the
  engine portable so that port stays cheap.

---

## Foundations: in force from M1 onwards, in every milestone

These are not milestones. They are rules every milestone is built and reviewed against, because each one is expensive
or impossible to bolt on later.

- [ ] **F1. Swarm-first.** Every new tool is designed for many concurrent agents:
  - bounded, paged responses
  - attributable to an agent
  - safe under claims and versions
  - usable in a batch
- [ ] **F2. Determinism.** The same seed and the same inputs give the same simulation, bit for bit.
  - Fixed timestep and a seeded engine RNG.
  - No iteration over randomly-ordered `HashMap`s in simulation code, and our own trig functions.
  - State hashes taken every tick.

  Design: [design/DETERMINISM.md](design/DETERMINISM.md).
  *Why from the start:* headless playtests, replays, bug reproduction and future multiplayer all depend on it, and
  retrofitting determinism means rewriting every system.
- [ ] **F3. Low-end performance.** The reference low-end device is the Intel integrated GPU in the dev machine
  (`--adapter integrated`). Every milestone's exit proof runs on it as well as on the RTX 4090.
  - Performance is visible to agents (`perf_frame`) and budgeted in `perf.toml`, so a budget miss is an error.
  - Pipelines are built at load time, never mid-game. That avoids the shader-compilation stutter common in big-engine
    games.
- [ ] **F4. Physics and animation designed in.** Before they're implemented, the world model reserves what they need:
  - a fixed simulation tick
  - a transform hierarchy that can hold skeletons
  - collider and body components
  - a skinned vertex format

  Designs: [design/PHYSICS.md](design/PHYSICS.md), [design/ANIMATION.md](design/ANIMATION.md).
- [ ] **F5. Humans optional, never locked out.** Every engine feature is reachable through the protocol first. The
  human UI is a client of that protocol.
- [ ] **F6. Portable by design, mobile included** (owner, 2026-10-13: mobile later, but make a later port easy now).
  Nothing in the engine may assume a desktop, x86 or a keyboard:
  - **Platform seam.** Window/surface, input, file access and app lifecycle go through `specula-platform` traits.
    Mobile apps get suspended, lose their GPU surface and read assets from a package, not loose files. Supporting
    *suspend/resume* and *surface lost/recreated* from M1 costs little on Windows and is painful to retrofit.
  - **Input is actions, not keys** (M3), with **touch as a first-class device** next to mouse and gamepad. UI layout
    handles DPI, aspect ratio and safe areas (M5).
  - **Tile-GPU friendly rendering.** Android has Vulkan natively, but its GPUs are tile-based. The render graph uses
    correct load/store ops, avoids needless mid-frame readbacks, and keeps to a mobile-safe Vulkan feature subset.
    iOS uses the Metal backend already planned for macOS.
  - **Compressed textures by target** (BC on PC, ASTC/ETC2 on mobile), chosen by the asset packer, never hardcoded.
  - **Determinism across x86 and ARM** (F2): our own maths functions, and a replay-hash test on an ARM target when
    one is available (Apple Silicon, Android).
  - **CI guard:** `cargo check` for `aarch64-linux-android`, so every crate keeps compiling for mobile, as rule 4 does
    for non-Windows.
  - **Known hard part:** linking for Android and iOS without external tools (the NDK linker, Xcode signing) collides
    with hard rule 3. That gets researched before the port; don't solve it now.

---

## Primary track: the swarm (grows with every milestone)

The goal is a swarm that can build an entire game while the owner directs. Each item lands with the milestone in
brackets.

- [ ] **Agent identity and roles** (M1): sessions carry agent id, name and role; `agent_hello`.
- [ ] **Claims, versions and journal** (M1): the safety layer (see M1).
- [ ] **Batch edits, `world_batch`** (M1): many changes per call, all or nothing.
  *Why:* fewer round trips means fewer tokens, and the world never holds half-finished work.
- [ ] **Response budgets** (M1, enforced from then on): every tool response is bounded and paged.
  *Why:* swarm cost scales with tokens per call.
- [ ] **Task board** (M1–M2): `task_post`, `task_take`, `task_done`, with tasks linked to claims.
  *Why:* claims protect data; the board shares intent, so two agents don't both build the tavern.
- [ ] **Role templates** (M1, growing): builder, reviewer, playtester, art director, sight-skeptic, performance
  auditor. They live in `.claude/agents/`, with Grok equivalents when used.
- [ ] **Headless playtest swarms** (M3): many headless engines on separate ports, each run by an agent testing one path,
  with results gathered into one report.
  *Why:* this is where swarms beat studios. QA scales with agents, not headcount.
- [ ] **Review queue** (M4): agents submit finished work with captures attached; the owner approves or rejects batches.
- [ ] **Director pattern** (M4–M5): one lead agent turns the game design doc into tasks, assigns roles and tracks
  progress on the board, with the owner as final reviewer.
  *Why:* this is the "agents complete the entire game" path.
- [ ] **Auth before any non-localhost bind** (before any remote agent): per-agent tokens, with claims tied to tokens.
- [ ] **Scale drills** (every milestone from M1): 16, then 64 concurrent agents. Measure throughput, conflict rate,
  token cost and reject reasons.
- [ ] **Cost and progress dashboard** (M5): tokens per agent, tasks done, rejects. Shown in the editor.

---

## M0: loop end-to-end (Windows, Vulkan). Done 2026-10-12

**Goal:** an AI agent connects to a live, native engine with no third-party code, builds something and sees it.
**Why first:** it proves the riskiest part (zero-dep Vulkan, Win32 and MCP all hand-written) before anything is built on
top of it.

- [x] **Workspace reset to zero dependencies.** CI fails if `Cargo.lock` lists anything outside the workspace.
  *Why:* zero-dep is only real if a machine enforces it.
- [x] **Rust toolchain's own linker** (`link-self-contained=yes`).
  *Why:* no MinGW, MSVC or LLVM install is needed. The owner wanted a custom linker; the toolchain linker gives the same
  "no third party" result today, and a custom linker stays on the long-term list.
- [x] **All OS and driver calls via `raw-dylib`**, each DLL declared by exactly one crate.
  *Why:* no import libraries from anyone. Windows-gnu silently drops a second crate's imports of the same DLL, so
  `specula-sys` owns `kernel32`.
- [x] **`specula-core`**: JSON, math, base64, CRC/Adler, DEFLATE and inflate, PNG encode and decode.
  *Why:* every later system needs these, and there are no crates to borrow.
- [x] **Win32 window and message loop** (raw FFI), pumped on its own thread.
  *Why:* dragging a window would otherwise freeze every agent.
- [x] **Vulkan 1.3**: runtime-loaded, validation and sync validation, both GPUs listed, offscreen targets, swapchain.
  *Why:* explicit state and validation give agents structured errors to debug from.
- [x] **One pipeline** (lit, coloured mesh), **reversed-Z with an infinite far plane**.
  *Why:* worlds of any size keep depth precision. A first attempt fitted the near plane to the world, and a distant
  object broke nearby ones.
- [x] **Readback to PNG** plus a JSON "sight packet" of subjects, screen boxes and distances.
  *Why:* the image is for the model's eyes; the packet is ground truth to check those eyes against.
- [x] **HTTP/1.1 and MCP server** (`initialize`, `tools/list`, `tools/call`).
  *Why:* MCP is how Claude Code, Grok and other clients plug in with no adapter.
- [x] **Tools**: `world_spawn`, `world_set`, `world_list`, `world_delete`, `sight_frame`, `engine_info`. Names use
  underscores because Claude Code rejects dots.
- [x] **Proof:** two Python MCP sessions built one scene. Claude Code itself connected, built, looked and confirmed
  boxes to within 1–2 px, with 0 validation errors.
- [x] **Hardening:** three adversarial review rounds; `scripts/window_check.py` on every GPU; CI on windows-msvc,
  windows-gnu and ubuntu.
  *Why:* the owner wanted nothing left open before M1. The reviews found 3 blockers and 13 other issues, all fixed. The
  list is in ENGINE_PLAN.md.

**Carried forward from M0** (known limitations, not bugs):
- [ ] **No occlusion.** A hidden object still reports a screen box. → M2 ID buffer.
- [ ] **Sphere boxes are about 30% loose**, because the boxes are projected bounding boxes. → M2.
- [ ] **One far-away object shrinks an auto-framed view to a dot.** → M2: frame by clusters or ignore outliers.
- [ ] **Throughput is vsync-paced** (about 60 calls/s per agent with a window open). → M1: batch tool, and commands
  applied separately from present.
- [ ] **A 257th open connection gets a TCP reset** instead of an HTTP 503. → M1.
- [ ] **No persistence.** Everything built is lost when the engine closes. → **first item of M1**.
- [ ] **Watch the MCP revision.** MCP revision 2026-07-28 drops `initialize` and sessions; we serve 2025-11-25 (plus
  06-18 and 03-26). Revisit when clients move.

---

## M1: world model, multi-agent and determinism. Next

**Goal:** many agents build one world at the same time, safely. The world survives restarts. The simulation clock is
deterministic from day one.
**Why now:** swarm need #2, plus foundations F2 and F4. Every later milestone multiplies agent output, and without
claims and versions more agents just means more collisions.

**Persistence first:**
- [x] **Text world files: save and load** (`world_save`, `world_load`, autosave). Deterministic ordering; one file per
  top-level subtree so git diffs stay small and different agents' areas don't merge-conflict.
  *Why first:* the game project starts now, and greybox work must not vanish on exit.
  *Done 2026-10-13:* `world.json` is the single commit point (generation files, fsync, crash-tested at every step); autosave on a worker thread; bit-exact floats.
- [x] **Hierarchy and paths**: parent/child entities, readable paths (`/world/village/house_03`), local transforms.
  *Why:* claims, persistence, skeletons (F4) and agents' mental models all work on trees.

**Entity model:**
- [x] **Components**: transform, mesh, material/colour, name, tags, and free-form `props` (JSON) for game data until M3
  modules define real components. Collider and body components are reserved (F4).
  *Why:* agents need to attach meaning ("door", "spawn_point") before logic exists.
  *Done 2026-10-13:* tags, props, reserved collider and body; values validated by the world itself.
- [ ] **Commands applied at tick boundaries**, separately from present, so throughput isn't vsync-paced.
- [x] **`world_batch`**: many commands in one call, applied atomically.
- [x] **`world_query`**: filter by path prefix, tag, region (AABB) or creator; paged results.
  *Why:* `world_list` returns everything, and at 10k entities that overflows an agent's context.

**Determinism groundwork (F2):**
- [x] **Fixed-tick simulation clock**: the tick number is the only time the simulation knows. Rendering interpolates.
  *Done 2026-10-13:* `game::Clock` (pause, step, scale; `time_control`). Hosted games tick on it; rendering gets `alpha`.
- [x] **Seeded engine RNG** in `specula-core`, with a separate stream per system.
  *Done 2026-10-13:* `specula_core::rng` (PCG32, named streams, integer chances).
- [x] **Deterministic maths** in `specula-core`: our own `sin`, `cos`, `atan2`, `exp`, `ln`, and no reliance on the
  platform maths library.
  *Why:* Rust's `f32::sin` and similar call the platform maths library. The msvc and gnu builds link different ones,
  so results can differ in the last bit between our own CI jobs.
  *Done (2026-10-13):* `specula_core::dmath`. The `f32` functions are correctly rounded:
  - one-argument functions: an exhaustive 2³² scan, with the near-midpoint cases checked by a 120-digit oracle;
  - atan2, pow and hypot: random samples checked by the same oracle.

  The `f64` ones are within 1–2 ulp. Golden bits are pinned by the tests. `math.rs` uses it for its trig.
- [x] **Ordered containers in simulation code.** No dependence on `HashMap` iteration order, which Rust randomises
  per process.
  *Done 2026-10-13:* the world uses ordered maps; Sepulcra's sim bans `HashMap`/`HashSet` with clippy.
- [x] **`world_hash`**: a hash of simulation state per tick.
  *Done 2026-10-13:* `world_hash` tool with a per-tick ring of (tick, hash) and a replay `verify`.
- [x] **Journal-replay test**: replaying a journal reproduces the same hash, in CI.
  *Done 2026-10-13:* `crates/specula/tests/journal_replay.rs` (GPU-free, pinned hash, runs on every CI job).

**Safe concurrency:**
- [x] **Agent identity**: session → agent id, name and role (from MCP `clientInfo` or `agent_hello`).
  *Done 2026-10-13:* `agent_hello`; attribution is `created_by`/`modified_by`.
- [x] **Journal**: append-only, agent-attributed, and tick-stamped. `world_history`, and `world_undo` scoped to your
  own changes by default.
  *Why:* swarms make mistakes at volume, so undo has to be surgical. Tick stamps make the journal a replay log (F2).
  *Done 2026-10-13:* two bounded rings (agents, game); game changes coalesced per tick; undo as a unit with conflict errors.
- [x] **Versions**: per-entity versions with an optional `expect_version`. A stale write is rejected with the current
  state.
- [x] **Claims**: on a subtree path or a region, as leases with a time limit that expire if the agent disappears.
  Writes outside your claim are rejected unless co-claimed.
  *Done 2026-10-13:* paths and regions, leases, session-bound identities; checked step by step for batches, undo, game tools and ancestor moves (4 adversarial review rounds).
- [x] **Change feed**: `world_changes(since)` for polling, plus MCP notifications where clients support them.
  *Done 2026-10-13:* polling with cursors and gap reporting; MCP push notifications still to do.

**Performance from the start (F3):**
- [x] **`perf_frame`**: CPU tick time, GPU time per pass (timestamp queries), draw count, triangle count, GPU memory.
  *Why:* agents can only optimise what they can see.
  *Done 2026-10-13:* per-pass GPU timestamps, CPU record/submit, draws, triangles, culled, pipelines after warm-up, memory and heap budgets, frame pacing, sim tick cost, structured budget errors.
- [x] **Reference run on the Intel integrated GPU** in the exit proof.
  *Done 2026-10-13:* `perf_frame` and `scripts/perf_baseline.py` run on both GPUs.

**Scale:**
- [x] **Connection limit returns HTTP 503**, not a reset. Scale drill with 16+ concurrent agents.
  *Done 2026-10-13:* 503 with Retry-After; `scripts/swarm_drill.py` at 16 and 64 agents with a 22-case attack phase (0 leaked).

**Exit proof:**
- [x] 4+ agents (Claude Code subagents) build separate districts at once.
  *Done 2026-10-13:* four Claude subagents built a chapel, a market, a farm and a watchtower (297 entities) in
  one world with claims; one tried five intrusions into another's district (all refused, incl. a rename-then-delete
  batch and moving the shared parent); save → restart → load gave the same hash and identical pixels. It also
  found that scripted agents locked themselves out of their own claims → resume tokens (identity-keyed claims).
- [x] A deliberately conflicting write is rejected cleanly.
- [x] Save → restart → load gives a pixel-identical render and the same `world_hash`.
  *Done 2026-10-13:* `scripts/world_roundtrip.py`: identical hash, subjects and PNG bytes.
- [ ] A journal replay matches its hash on both the msvc and gnu builds.
- [x] It runs on both GPUs.
- [x] An adversarial review passes.
  *Done 2026-10-13:* every M1 lane passed lane-reviewer rounds before merging.

---

## M2: rendering, true sight, animation, first procedural shapes

**Goal:** worlds that look like games, sight agents can trust, animated characters, and original shapes beyond cubes.
**Why here:** after M1 the bottleneck becomes "does it look right, is it really visible, and can agents make real
shapes?"

- [x] **Render graph** that owns barriers and transitions.
  *Done 2026-10-14:* `specula-gpu::graph`; every pass declares its reads/writes; sync-validation clean.
- [x] **Materials** (PBR-lite plus a stylised/vertex-colour path), **directional and point lights**, **shadow maps**.
  *Started 2026-10-13 for the lava-lamp benchmark:*
  - per-frame data through a buffer device address
  - a `standard` material (GGX highlights, metal, emission) and up to 16 point lights
  - blended `glass` and `blob_shadow`, sorted far to near (nested shells handled)
  - ACES tone mapping with dithering, and an adjustable sun and ambient light
  *Done 2026-10-13:* standard/glass/blob materials, vertex colours, 16 point lights, a fitted or cascaded directional shadow map with PCF (render lane).

  Still to do: shadow maps, textures, the vertex-colour path, and a real bloom pass (the lamp's halo is analytic).
  *Why the stylised path:* it's cheap on low-end GPUs (F3) and suits agent-made content.
- [x] **GPU instancing, frustum culling, and LOD selection.**
  *Why:* swarms build big worlds, and low-end hardware must stay at frame rate.
  *Done 2026-10-13:* instancing (10k cubes in 1 draw call; 2.7 ms GPU on the Intel iGPU) and CPU frustum culling. LOD selection still to do.
- [ ] **2D**: sprites, tilemaps, vector shapes, orthographic cameras.
- [x] PNG decode (done early, in M0).
- [x] **glTF import**: meshes, materials, textures, skins, animations. GLB included.
  *Done 2026-10-14:* `specula-gltf` import/export, our own JPEG codec in `specula-core`; skins → rigs, animations → clips.
- [ ] **Skeletal animation, stages A1–A2** ([design/ANIMATION.md](design/ANIMATION.md)): GPU skinning, clip playback,
  crossfades, filmstrip sight.
  *Progress 2026-10-13:* rigs in recipes, `specula-anim` clips/blending/generators/IK, CPU skinning with GPU-ready matrices, filmstrip sight. GPU skinning still to do.
- [x] **Procedural modelling, tier 1** ([design/PROCEDURAL_MODELLING.md](design/PROCEDURAL_MODELLING.md)):
  - recipes and primitives
  - extrude, lathe, sweep
  - bevel, array and mirror
  - auto-LOD and auto-collider
  *Done 2026-10-13:* `specula-model` recipes with hard limits and fidelity-checked auto-LOD; 21 Sepulcra assets (contact sheet in docs/fidelity).

  *Why pulled forward from M4:* the game needs real shapes early, and tier 1 is small.
- [ ] **ID buffer**: per-entity visible-pixel counts and exact screen extents. Fixes occlusion and loose boxes.
- [ ] **Per-agent persistent cameras**; `sight_orbit` (turntable) and `sight_compare` (before/after).
- [ ] **Debug overlays in captures**: grid, axes, bounds, labels, claim regions, skeletons.
- [x] **`perf.toml` budgets**: frame time on the reference low-end GPU. A budget miss is reported as an error.
  *Done 2026-10-14:* `--perf FILE` / `./perf.toml` (`perf.example.toml` documents every budget, per-adapter overrides); misses are structured errors in `perf_frame` and `perf_capture`.
- [ ] ➕ **Post-processing**: tonemapping, bloom, FXAA/SMAA, colour grading; optional SSAO.
  *Progress 2026-10-13:* HDR bloom, ACES tonemap, FXAA (colour grading and SSAO still to do).
- [x] ➕ **Analytic sky and height fog.**
  *Done 2026-10-13:* analytic sky with a moon disc, exponential height fog.
- [x] ➕ **Fixed pipeline set**: materials are parameters into a few uber-shaders, all pipelines are built at load
  with an on-disk Vulkan pipeline cache, and a pipeline created mid-game is a `perf_frame` error.
  *Why:* this avoids Unreal-style shader stutter by design (survey §3).
  *Done 2026-10-13:* all pipelines built at load with a validated on-disk pipeline cache; `pipelines.created_after_warm` is a perf_frame error.
- [x] ➕ **Profiling tools**: CPU zones, frame capture and memory tracker, exposed as tools.
  *Done 2026-10-14:* CPU zones (`perf_zones`), frame capture with a Chrome trace (`perf_capture`), memory tracker (`perf_memory`).

**Exit proof:**
- [ ] A lit, shadowed scene with imported and procedural assets and an animated character.
- [ ] Sight reports 0 visible pixels for a fully hidden object.
- [ ] A 10k-instance world holds its budget on the Intel integrated GPU.

---

## M3: logic, physics and time. The game becomes playable

**Goal:** agents write gameplay code, reload it live, and playtest headless at scale.
**Why here:** swarm need #3. The game moves from diorama to game.

- [ ] **Module host**: game logic as Rust `cdylib`s behind a C ABI. The engine owns all state; build → reload without
  losing the world; structured build errors.
- [ ] **`specula-sdk`**: the one crate game modules depend on. It exposes only deterministic APIs (tick, engine RNG),
  enforcing F2.
- [x] **Input**: keyboard and mouse (Win32), gamepads (XInput, runtime-loaded). Input is recorded into the journal.
  *Done 2026-10-13:* Win32 keyboard/mouse/pointer + XInput (runtime-loaded); per-tick action recordings in `specula-input`.
- [x] **Time**: `time_pause`, `time_step`, `time_resume`, and `time_scale` for fast-forward headless runs.
  *Done 2026-10-13:* `time_control` pause/resume/step/scale.
- [ ] **Physics, stages P1–P3** ([design/PHYSICS.md](design/PHYSICS.md)):
  - P1: queries and triggers
  - P2: kinematic character controller
  - P3: rigid bodies with the Soft Step solver
- [ ] **Animation, stage A3**: state machines and blend trees as text data, events, root motion, two-bone IK.
- [ ] **Headless playtests**: `play_*` tools to script inputs, run N ticks, assert on state, and record a replay plus
  captures.
  *Progress 2026-10-13:* Sepulcra's `play_*` tools and `scripts/sepulcra_play.py` play and verify whole games.
- [ ] **Desync detector**: when two runs' hashes diverge, report the first tick and the component that differs.
- [x] ➕ **Input actions with rebinding** ("jump", not "Space"), with device-correct glyphs.
  *Done 2026-10-13:* `specula-input`: actions as JSON, rebinding with conflicts, pad/touch cancel, device glyphs.
- [ ] ➕ **Navigation and AI**: navmesh generation from world geometry, pathfinding, crowd avoidance; behaviour trees
  and utility AI as data.
  *Why:* nearly every game with NPCs needs this, and the original plan had no navigation at all.
- [ ] ➕ **Save games** (moved earlier from M6): with determinism, a save is a serialisation of simulation state.
- [ ] ➕ **Async streaming with per-frame budgets, plus HLOD**: no synchronous loads on the simulation thread.
  *Why:* traversal stutter is Unreal's other well-known problem (survey §3).
- [ ] ➕ **2D physics variant** if the game is 2D.

**Exit proof:**
- [ ] An agent writes a module that adds a playable character.
- [ ] Another agent changes that module while the world keeps running.
- [ ] A headless suite of 20+ replays runs across parallel engines and passes.
- [ ] The playable build holds budget on the integrated GPU.

---

## M4: asset studio, style and review

**Goal:** agents produce consistent, on-style assets at volume, checked automatically and approved in batches.
**Why here:** swarm needs #4 and #5. Content is where small studios lose to big ones.

- [ ] **Stage world**: neutral backdrop, fixed lighting, scale references (a 1.8 m human, a 1 m cube).
- [ ] **Contact sheets** (many angles in one image), **line-ups** (asset beside siblings), **before/after**.
- [ ] **Style bible and `style.toml`**:
  - The bible holds the per-game palette, shape language and proportions.
  - `style.toml` holds the measurable rules: triangle and texture budgets, scale ranges, palette distance, naming.
  - A violation is an error.
- [ ] **Taste loop**: silhouette, value and colour passes at thumbnail size; critique against the bible; revise. See
  the procedural modelling design.
- [ ] **Procedural modelling, tiers 2–4**: booleans, SDF modelling, scatter and building generators, terrain,
  procedural materials.
- [ ] **Review queue**: submissions with captures; the owner approves or rejects batches, and a rejection returns to
  the agent with its note.
- [ ] **Physics, stage P4**: joints and ragdolls, which connect to skeletons.
- [ ] ➕ **Baked global illumination**: lightmaps or probe volumes, baked by agents.
  *Why:* this is the low-end-friendly answer to Lumen.
- [ ] ➕ **Particle/VFX system**: CPU first, GPU later. Cosmetic only, so it's outside determinism. Effects are data.
- [ ] ➕ **Terrain renderer** (LOD + splat materials), **decals**, **stylised water**.

**Exit proof:**
- [ ] A swarm builds a themed asset set (30+ props) that passes `style.toml`.
- [ ] It is presented as one contact sheet in the review queue.
- [ ] The approved assets go straight into the game world.

---

## M5: audio, text, UI and the human editor

- [ ] **Audio**: WASAPI output and a mixer; WAV, then OGG/Vorbis decode (hand-written); 3D positional sound.
  *Progress 2026-10-13:* `specula-audio`: WASAPI, mixer, 3D, recipe synthesiser, WAV (OGG/Vorbis still to do).
- [ ] **Text, stages T1–T4** ([design/UI.md](design/UI.md)): TrueType parser, CPU atlas for small text, Slug-style
  GPU text for scalable and in-world text, layout and input, then scripts.
  *Progress 2026-10-13:* T1–T2: `specula-text` TrueType parser, rasteriser, SDF, glyph atlas, layout.
- [x] **Immediate-mode UI with flexbox-style layout**, used for game UI and the editor. UI screens and themes are data.
  *Done 2026-10-13:* `specula-ui`: screens and themes as JSON, GPU overlay path matching the software preview.
- [x] ➕ **Gamepad-first focus navigation**, device-correct glyphs, legibility checks (Steam Deck rules).
  *Done 2026-10-13:* gamepad/keyboard spatial focus, prompt glyphs, Deck legibility in `ui_check`.
- [ ] ➕ **UI sight tools**: `ui_tree`, `ui_frame`, `ui_act`, `ui_check`.
  *Progress 2026-10-13:* `ui_check` and `ui_tree` exist in the crate; MCP tools come with engine integration.
- [ ] ➕ **Audio buses and effects** (reverb, low-pass), spatialisation, streaming music.
  *Progress 2026-10-13:* buses, ducking, reverb and low-pass in recipes, 3D spatialisation.
- [ ] ➕ **Timeline/sequencer and camera rails** as data, for cutscenes.
- [ ] ➕ **Localisation**: string tables, plural rules, per-locale fonts; agents translate and `ui_check` catches
  overflow.
- [ ] ➕ **Accessibility**: UI scale, colour-vision themes, subtitles, remapping, hold/toggle, reduced motion;
  screen reader via Windows UI Automation (later).
- [ ] ➕ **Reusable gameplay modules** as SDK examples: inventory, abilities, dialogue, quests.
- [ ] **Human editor on the protocol**:
  - outliner and inspector
  - click to select
  - **3D gizmos** to drag, rotate and scale
  - **draw tools**: paint vertex colours, sketch paths and splines, sculpt terrain
  - journal timeline ("which agent built this")
  - claim map overlay, review queue, cost and progress dashboard

  *Why:* devs who want to edit by hand can; human edits are journaled like agent edits.

---

## M6: ship

- [ ] **`specula export`**: a standalone game executable plus assets, with the agent and editor layers stripped out.
- [ ] **Shipping basics**: settings menu, structured crash reports for agents to triage, `perf.toml`
  budgets checked on the reference hardware before release.
- [ ] ➕ **Asset packing**: compressed, patch-friendly packs; licence metadata on every asset, and export refuses
  unlicensed ones.
- [ ] ➕ **Steam integration** (if the owner approves): runtime-loaded `steam_api64.dll` for achievements, cloud saves
  and Steam Input.
- [ ] ➕ **Steam Deck Verified pass**: controller-only playthrough by agents, 800p legibility, at least 30 fps.
- [ ] ➕ **Soak tests**: long headless runs for leaks and crashes.
- [ ] **Ship the owner's original game.**

---

## Platform track (after M2, in parallel)

- [ ] **Linux**: X11, then the Wayland wire protocol; Vulkan surface.
  *Why after M2:* the render-hardware interface needs to settle first.
- [ ] **macOS**: Cocoa via the objc runtime, plus a native Metal backend. No MoltenVK, to stay zero-dep.
- [ ] **Handheld-class targets** (for example Steam Deck, Linux + Vulkan) as part of F3's low-end story.
- [ ] **Mobile (after v1.0)**: Android first (Vulkan already, so the render backend carries over), then iOS on the
  Metal backend. Prepared by F6; the zero-dep linking and signing story is researched first.

## Fidelity track: from greybox to Unreal-class realism (grows with M2, M4 and beyond)

A standing challenge, separate from any one game: the same benchmark assets are rebuilt at each fidelity tier, so the
engine's visual progress is measurable. The owner asked for it on 2026-10-13: "single assets to challenge ourselves as
we move up to completion stage", from simple up to Unreal Engine's realism.

**Benchmark set:**
- a carved runestone (stone, engraving, moss)
- a banded wooden barrel (wood, metal)
- an iron lantern (metal, glass, emissive flame, point light)
- a gnarled tree (foliage, alpha, wind, LOD)
- a skeleton warrior (rigged, animated)
- optional final boss: a realistic human head (skin, hair)

Each tier is rendered in the M4 stage world from fixed cameras, and every milestone adds one contact sheet to
`docs/fidelity/`.

| Tier | Look | Unlocked by |
|---|---|---|
| T0 | Greybox: coloured primitives | M0 (now) |
| T1 | Stylised flat: low-poly, vertex colour, bevels | M2 materials + procedural tier 1 |
| T2 | Indie polish: shadows, sky, fog, tonemapping, AA | rest of M2 |
| T3 | Detailed stylised: SDF sculpting, normal maps, baked GI, decals, particles | M4 |
| T4 | Realistic, Unreal-class | the items below |

- [x] **First benchmark: the lava lamp** (owner's choice, 2026-10-13), built at three levels by
  `scripts/lava_lamps.py`:
  - L1: opaque lathes with emissive colour.
  - L2: metal, glass over a tinted liquid, wax spheres and a point light.
  - L3: the ray-marched `lava_lamp` material with refraction, an absorbing and scattering liquid, metaball wax with
    deep cores and glowing edges, a halo, and light spill.

  The captures are reproducible with `time_s` and match on NVIDIA and Intel within 3/255 (integer hashes, not
  `sin` hashes, in shaders).
- [ ] **Benchmark recipes** for the other subjects at T0, with the contact sheet scripted.
- [ ] **Every rung has an integrated-GPU budget** (F3). T4 is reached through quality presets that scale down to the
  iGPU and the Deck, not through Nanite/Lumen-style brute force. *Why:* "realistic within budget" is itself a way to
  stand out from Unreal.
- [ ] ➕ **Image-based lighting:** prefiltered environment maps, BRDF lookup table, reflection probes.
- [ ] ➕ **Normal and parallax-occlusion maps**; detail maps.
- [ ] ➕ **Cascaded shadow maps with soft filtering**, plus contact shadows.
- [ ] ➕ **TAA** (with an upscaling path), **GTAO**, screen-space reflections.
- [ ] ➕ **Subsurface and translucency:** leaves first, skin later.
- [ ] ➕ **Procedural texture graphs** (Substance Designer-style, as text recipes): agents author PBR texture sets
  without scans or third-party tools.
- [ ] ➕ **Hair and fur cards**, for the optional head.
- [ ] **Reference comparison:** a T4 sheet next to reference photos or renders, critiqued with the taste loop.

*Why:* lighting carries most of Unreal's realism, so this track is mainly a renderer track. The benchmark keeps the
renderer honest, and the best rung of each asset doubles as game content.

## Rust track (from M1, long-term)

The aim is to make Rust's costs non-issues and then strengths. Research and measurements:
[research/RUST_AT_SCALE.md](research/RUST_AT_SCALE.md). Baseline on 2026-10-13: clean dev build 8.2 s, incremental
1.2–2.0 s, release 12.7 s, 1.16 MB binary; unsafe code only in `specula-sys`, `-platform` and `-gpu`.

**Cheap and immediate (with M1):**
- [ ] `#![forbid(unsafe_code)]` in every crate without FFI (core, protocol, specula now; every new crate after).
- [ ] `[workspace.lints]`: deny `clippy::undocumented_unsafe_blocks`, `clippy::missing_safety_doc` and
  `unsafe_op_in_unsafe_fn`. SAFETY comments become machine-checked.
- [ ] Move to Rust edition 2024.
- [ ] **Determinism as lints** in simulation crates: `disallowed_types` (default `HashMap`/`HashSet`) and
  `disallowed_methods` (platform trig, wall-clock time).
- [ ] **Compile-time budgets in CI** from `cargo build --timings`: clean dev under 30 s, incremental under 3 s.

**With M3 (module host):**
- [ ] Thin game modules that depend only on `specula-sdk`; edit → reload under 2 s as a budget.
- [ ] `logic_build`: compiler diagnostics as JSON data, with machine-applicable fixes auto-applied.
- [ ] Per-package dev optimisation (engine `opt-level = 2`, modules `0`).

**Long-term:**
- [ ] Build service: the engine queues and merges swarm build requests (the Cargo lock becomes a scheduler).
- [ ] Zero-dep mutation fuzzer for every parser (JSON, HTTP, PNG, inflate, TrueType, glTF, audio, saves), in CI.
- [ ] An unsafe-auditor reviewer role, and the unsafe-site count tracked per crate.
- [ ] Try `rust-lld` on windows-gnu (the toolchain's own linker) and measure link time.
- [ ] Evaluate Cranelift dev builds, the parallel front end and Cargo's shared cache on nightly; adopt each when it
  is stable.

## Long-term

- [ ] Own linker (parked by agreement; the toolchain linker already removes third parties).
- [ ] Own shading language compiled to SPIR-V (removes the dev-time `glslc`).
- [ ] Networking and multiplayer: lockstep or rollback, which F2 makes possible.
- [ ] Animation retargeting and parametric character generation.
- [ ] Publish crates to crates.io (owner to decide).

---

## Design research

These are researched designs, written before implementation so the foundations hold.

- [x] [design/DETERMINISM.md](design/DETERMINISM.md): written 2026-10-13. [ ] Reviewed with the owner.
- [x] [design/PHYSICS.md](design/PHYSICS.md): written 2026-10-13. [ ] Reviewed with the owner.
- [x] [design/ANIMATION.md](design/ANIMATION.md): written 2026-10-13. [ ] Reviewed with the owner.
- [x] [design/PROCEDURAL_MODELLING.md](design/PROCEDURAL_MODELLING.md): written 2026-10-13. [ ] Reviewed with the owner.
- [x] [design/UI.md](design/UI.md): UI, text, input focus and accessibility; written 2026-10-13. [ ] Reviewed with the owner.
- [x] [research/ENGINE_SURVEY.md](research/ENGINE_SURVEY.md): Unreal, Unity and Godot vs. Specula; gaps; Unreal's
  efficiency problems; hard truths. Written 2026-10-13. [ ] Reviewed with the owner.
- [x] [research/RUST_AT_SCALE.md](research/RUST_AT_SCALE.md): compile times, unsafe and the language as strengths;
  measured baseline. Written 2026-10-13. [ ] Reviewed with the owner.
- [ ] Still to research when their lane starts: audio (WASAPI, mixing, Vorbis), navigation (navmesh generation),
  global-illumination baking, particle systems, asset streaming, Linux/Wayland and macOS/Metal backends.

---

## How each milestone runs

This is what worked for M0:
1. Plan the milestone in this file and agree it with the owner.
2. Build in parallel with subagents, one crate or area each, in git worktrees. Claude integrates the results.
3. Read the whole result back against the checklist **and the foundations F1–F5**.
4. Run adversarial review rounds with an independent reviewer subagent until a round finds nothing new.
5. Run the live checks (`scripts/window_check.py`) on every GPU, plus the exit proof with real Claude Code.
6. Tick the boxes here, add a line to ENGINE_PLAN.md's log, and push when the owner asks.
