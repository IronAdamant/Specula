# Research: what production engines provide, and what Specula is missing

Written 2026-10-13. This compares Unreal Engine 5, Unity 6 and Godot 4.5 (with notes on Bevy and Fyrox) against
Specula's roadmap. **Goal:** find every gap between "agent-native engine" and "engine you can ship a commercial game
on", decide what to copy and what to avoid, and confirm where Specula can actually win.

## 1. The landscape in 2026: agents are coming to every engine

- **Unity** ships an official MCP server for Unity 6+ (scene hierarchy, components, build settings, console).
  Community servers exist with 100+ tools.
- **Unreal** has an Epic-provided MCP bridge (reported with UE 5.8), and Unreal MCP was announced for UEFN in
  August 2026 (beta). Open-source Unreal MCP servers also exist.
- **Godot** has several community MCP plugins, including ones aimed at Claude Code.

These reports come mostly from vendor blogs and directories, so the details are unverified.

**What this means.** "An agent can drive the editor" is no longer unique. It will be a checkbox feature in every
engine. Those integrations are **bolted on**: the agent operates a tool designed for humans, one at a time, through an
API surface that was never built for concurrency, verification or replay. Specula has to win on what bolt-ons can't
easily retrofit:

| Specula advantage | Why big engines struggle to copy it |
|---|---|
| **Swarm concurrency**: claims, versions, agent-attributed journal, surgical undo | Their editors assume one human per level (Unreal's One File Per Actor helps with source control, not live multi-agent edits) |
| **Verification as data**: sight packets, ID-buffer visibility, measured checks, `perf_frame` | Their renderers aren't built to report ground truth per entity to a caller |
| **Deterministic headless simulation at scale**: replay hashes, parallel playtest swarms | Their physics and gameplay frameworks aren't deterministic by default; retrofitting is a rewrite |
| **Whole engine readable by agents**: zero-dep, one repo, small | Unreal is millions of lines; agents can't hold it or safely change it |
| **Efficient by default**: low-end budgets as errors | Their defaults target high-end hardware (see §3) |
| **Recipes as source**: procedural assets agents can read and edit | Their asset pipelines are binary and DCC-tool-based |

If Specula only matched features, it would lose to engines with decades of head start. The plan has to keep these six
advantages central.

## 2. Feature inventory: production engines vs. Specula's plan

✅ = in the roadmap · ➕ = **gap, added to the roadmap by this survey** · ⏸ = deliberately deferred

| Area | Unreal 5 | Unity 6 | Godot 4.5 | Specula |
|---|---|---|---|---|
| **Rendering core** | Deferred/forward, Nanite virtual geometry, Lumen dynamic GI, TSR upscaling, Virtual Shadow Maps | URP/HDRP, GPU Resident Drawer | Forward+, Mobile, Compatibility renderers; SDFGI/VoxelGI | ✅ render graph, PBR-lite + stylised, shadows, instancing, culling, LOD (M2) |
| **Post-processing** | Full stack (tonemap, bloom, DoF, motion blur, AA) | Volume-based post stack | Environment effects, glow, SSAO, SSR | ➕ tonemapping, bloom, FXAA/SMAA, colour grading, optional SSAO (M2) |
| **Global illumination** | Lumen (dynamic, expensive) | Baked lightmaps, APV probes | Lightmaps, SDFGI, VoxelGI | ➕ **baked lightmaps / probe volumes, agent-baked** (M4). The low-end-friendly answer to Lumen. |
| **Sky, fog, water** | Sky atmosphere, volumetric clouds and fog, Water plugin | HDRP sky and fog; water system | Sky shaders, volumetric fog | ➕ analytic sky, height fog (M2); stylised water (M4) |
| **Terrain and foliage** | Landscape, foliage instancing, PCG framework | Terrain, SpeedTree | Community terrain plugins | ✅ procedural tier 4 (M4); ➕ terrain renderer with LOD + splat materials (M4) |
| **Decals** | Deferred decals | URP/HDRP decals | Decal node | ➕ (M4) |
| **VFX / particles** | Niagara (GPU, node-based) | VFX Graph, Shuriken | GPU/CPU particles | ➕ **particle system**: CPU first, GPU later; cosmetic only, so outside determinism; effects as data (M4) |
| **Large worlds / streaming** | World Partition, HLOD, data layers | Addressables, scene streaming | Manual | ➕ **async streaming with per-frame budgets + HLOD** (M3). Avoids traversal stutter. |
| **2D** | Paper2D (minimal) | Full 2D suite | Excellent 2D | ✅ sprites, tilemaps, vector (M2); ➕ 2D physics variant (M3+) |
| **Physics** | Chaos | PhysX / Unity Physics (DOTS, deterministic) | Godot Physics, Jolt option | ✅ P1–P4 (M3–M4) |
| **Animation** | Control Rig, IK Rig, Motion Matching, Sequencer | Mecanim, Animation Rigging, Timeline | AnimationTree, IK | ✅ A1–A4 (M2–M4) |
| **Navigation and AI** | NavMesh (Recast), Behaviour Trees, StateTree, EQS, Mass AI | NavMesh, Behaviour package | NavigationServer (Recast-based) | ➕ **navmesh generation + pathfinding + crowd avoidance; behaviour trees/utility AI as data** (M3). Most games need this. |
| **Gameplay framework** | Gameplay Ability System, Enhanced Input, GameMode/Pawn | Input System, Netcode, DOTS | Scenes, signals, InputMap | ✅ modules + SDK (M3); ➕ **input actions with rebinding** (M3); reusable gameplay modules (inventory, abilities, dialogue) as SDK examples (M5) |
| **Scripting** | C++, Blueprints, Verse (UEFN) | C# | GDScript, C#, GDExtension | ✅ hot-reloaded Rust modules. Agents write Rust well; no visual scripting needed. |
| **UI** | UMG, CommonUI (gamepad focus), Slate (editor) | UI Toolkit, uGUI | Control nodes, themes; screen reader via AccessKit (experimental, 4.5) | ✅ immediate-mode UI (M5). Design in [design/UI.md](../design/UI.md); ➕ gamepad focus navigation, theming, UI as data |
| **Text** | Slate fonts, localisation dashboard | TextMeshPro (SDF) | Font import, MSDF, BiDi/shaping | ➕ details in UI.md: TrueType, Slug-style GPU text, shaping stages |
| **Audio** | MetaSounds, spatial audio, submixes | Audio mixer | Audio buses, effects | ✅ WASAPI, mixer, decoders (M5); ➕ buses, effects (reverb, low-pass), spatialisation, streaming music (M5) |
| **Cinematics** | Sequencer | Timeline, Cinemachine | AnimationPlayer | ➕ **timeline/sequencer + camera rails as data** (M5) |
| **Localisation** | Localisation dashboard, String Tables | Localization package | Translation CSV/PO | ➕ string tables, plural rules, per-locale fonts; agents translate (M5) |
| **Accessibility** | Partial (colour-vision options, UI) | Partial | Screen reader (experimental) | ➕ UI scale, colour-vision modes, subtitles, remapping, reduced motion, hold-to-toggle (M5); screen reader via Windows UI Automation (later) |
| **Save system** | SaveGame objects | Manual / packages | Resources / ConfigFile | ✅ M6. ➕ moved earlier to M3: determinism makes saves a serialisation of state |
| **Networking** | Replication, Iris | Netcode for GameObjects/Entities | High-level multiplayer API | ⏸ long-term. Determinism (F2) keeps lockstep/rollback open. |
| **Profiling** | Unreal Insights | Profiler, Frame Debugger | Profiler, monitors | ✅ `perf_frame` (M1); ➕ CPU zones, frame capture, memory tracker exposed as tools (M2) |
| **Build and packaging** | Cook + package per platform | Build pipeline | Export templates | ✅ `specula export` (M6); ➕ asset packing/compression, patch-friendly packs (M6) |
| **Platform services** | Online Subsystem (Steam, EOS) | Platform packages | Plugins | ➕ optional runtime-loaded `steam_api64.dll` (achievements, cloud saves, Steam Input). This is an **owner decision** (see §4). |
| **Platforms** | PC, consoles, mobile, XR | Everything | PC, mobile, web, consoles via third parties | ✅ Windows → Linux/Steam Deck → macOS; ⏸ consoles (closed SDKs, see §4); ⏸ mobile, web, XR |
| **Editor** | Full DCC-like editor | Full editor | Full editor | ✅ M5 editor on the protocol (gizmos, painting, outliner, journal timeline) |
| **Agent integration** | MCP bridge (2026) | Official MCP (2026) | Community MCP | ✅ **native**: the core of the engine |

## 3. What to learn from Unreal's efficiency problems

The owner's complaint: games made with Unreal often run poorly, especially on modest hardware. The documented causes,
and Specula's design response:

| Problem | Cause | Specula's response |
|---|---|---|
| **Shader-compilation stutter** | On PC (D3D12/Vulkan), pipeline state objects must be compiled for the exact GPU and driver, at runtime. Unreal's material graphs produce huge numbers of shader permutations. Epic's example: Fortnite compiles about 30,000 PSOs per match and uses about 10,000. PSO precaching (UE 5.2+) helps but can't guarantee a PSO is ready in time. | **A small, fixed set of pipelines.** Materials are parameters into a few uber-shaders, not generated shader graphs. All pipelines are built at load (F3), plus a Vulkan pipeline cache on disk. `perf_frame` reports any pipeline created mid-game as an error. |
| **Traversal stutter** | Streaming in new areas does synchronous work (loading, registration, GC) on the game thread. It persisted after shader fixes (Digital Foundry's UE 5.2-era testing as reported by PCGH). | **Async streaming with per-frame time budgets** (➕ M3). No synchronous loads on the simulation thread. The budget is measured and enforced. |
| **High-end defaults** | Lumen, Nanite, Virtual Shadow Maps and TSR are designed for current-generation hardware. Teams ship with them on, and low-end machines suffer. | **Defaults target the integrated GPU.** Baked GI and stylised materials are the default path. Expensive features are opt-in and budgeted. |
| **Engine weight** | Huge engine, long builds, large binaries, a heavy editor. | Zero-dep, small engine. Export strips the agent and editor layers. Binary size and start-up time are tracked. |
| **Optimisation left to the end** | Performance work happens at the end of production, when it's expensive. | **Budgets are errors from M1 onwards.** Performance-auditor agents run continuously on the reference low-end device. |

Specula's aim isn't to out-feature Unreal. It's to make the efficient path the default and to make inefficiency
visible to agents immediately.

## 4. Hard truths (being honest)

- **Consoles.** PlayStation, Xbox and Switch SDKs are closed, under NDA, and need platform-holder approval and devkits.
  Specula can't target them without a publishing relationship. The render-hardware interface keeps it possible, but
  plan for **PC + Steam Deck** first. That's where small studios ship anyway.
- **Steamworks** is a Valve DLL (`steam_api64.dll`). Loading it at runtime fits the existing pattern (like
  `vulkan-1.dll`), and no crates are involved. It's a platform service like the GPU driver, not a library we build
  with. **The owner should decide** whether this fits the zero-dep principle. Recommendation: allow optional
  runtime-loaded platform services, per game.
- **Fonts and assets have licences.** Shipping a game means checking the licence of every font, sound and model. Agents
  can track licences in asset metadata, and an export check can refuse unlicensed assets (➕ M6).
- **Complex scripts** (Arabic, Hebrew, Indic) need full text shaping, which is a big job. Latin, Cyrillic and Greek
  come first; CJK needs large fonts and atlas paging; complex shaping comes later. See UI.md.
- **Being first matters less than being finished.** Big engines will keep adding AI features. Specula's edge is
  architecture, which takes years to copy. It needs to ship a real game to prove that.

## Sources

- Epic Games, ["Game engines and shader stuttering: Unreal Engine's solution to the problem"](https://www.unrealengine.com/en-US/tech-blog/game-engines-and-shader-stuttering-unreal-engines-solution-to-the-problem),
  summarised via [DSOGaming](https://www.dsogaming.com/news/epic-games-adds-new-pso-precaching-mechanism-to-unreal-engine-5-1-to-reduce-shader-compilation-stutters)
  and [Yahoo Tech](https://tech.yahoo.com/gaming/articles/stutters-hitches-unreal-engine-5-112436805.html). The page
  itself returned 403 to our fetcher.
- [PC Games Hardware on UE 5.2 stutter and Digital Foundry's tests](https://www.pcgameshardware.de/Unreal-Engine-Software-239301/News/UE5-2-Weniger-Ruckler-durch-neue-Shader-Kompilierung-1424218/).
- [Unreal Engine 5 overview](https://www.unrealengine.com/UE5) and [Wikipedia](https://en.wikipedia.org/wiki/Unreal_Engine_5).
- MCP landscape (vendor sources, treat with care): [StraySpark 2026 comparison](https://www.strayspark.studio/blog/unity-mcp-server-vs-unreal-godot-blender-2026),
  [MCP ecosystem overview](https://www.strayspark.studio/blog/mcp-ecosystem-game-developers-2026),
  [UEFN Unreal MCP](https://explainx.ai/blog/uefn-unreal-mcp-fortnite-verse-agents-august-2026).
- [Godot 4.5 release notes](https://godotengine.org/releases/4.5/) (AccessKit screen reader, experimental).
- [Steamworks: Steam Deck compatibility](https://partner.steamgames.com/doc/steamhardware/compat).
