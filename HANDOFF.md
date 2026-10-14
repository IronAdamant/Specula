# Handoff: read this first

These are notes from Claude (Opus 5.5) to whoever picks up Specula next, most likely a new Claude conversation with no
memory of the earlier ones. Last updated **2026-10-13**.

- **START HERE next session:** the owner asked (2026-10-13) for subagent swarms to advance M1 and M2 **while
  building Sepulcra to completion as dogfood** (Windows first), keep improving the lava lamps as features land,
  and keep performance baselines. Work runs as **lanes**: one subagent per crate/area in a git worktree, each
  attacked by a `lane-reviewer` before merge (`.claude/agents/`). Check `git log` and the roadmap ticks for
  where things stand. Game docs: [DESIGN.md](games/sepulcra/DESIGN.md) (sim rules, numbers, owner questions),
  [STYLE.md](games/sepulcra/STYLE.md) (Claude's style bible draft; owner questions at the end).
  How the game was chosen: [docs/GAME_SELECTION.md](docs/GAME_SELECTION.md).
- **Paused 2026-10-13 (owner: "I'll look into M2 tomorrow").** State at the pause:
  - **M1 done** (exit proof with four Claude subagents; only the msvc replay check is left to CI).
  - **Merged:** capture speed, GPU stats/instancing, world + persistence, dmath, journal/claims/identity
    (resume tokens), render (shadows, bloom, FXAA, sky, fog, overlay), text, input, procedural modelling,
    animation, audio, UI (GPU overlay), engine effects in `world_env`, Sepulcra sim + app (playable greybox).
  - **Unmerged lane: Engine-for-games** (`model` entities with shared GPU meshes and LOD, visibility, transient
    entities, UI layer, game hooks). Branch `worktree-agent-aebd4fb01086d5366` at 688ecb6, worktree in
    `.claude/worktrees/agent-aebd4fb01086d5366` (merges cleanly with main). Round 1's 4 majors are fixed; round 2
    says **do not merge** until these are fixed (all in the new model build workers, `crates/specula/src/`):
    1. `lib.rs` `wait_for_models` (from sight_frame): the ≤10 s wait blocks the engine thread, freezing all
       agents, the game and the window (other agents' calls time out). Park the capture as a waiter (like
       `BuildWaiter`), counting only visible in-frame models, and keep serving requests.
    2. `models.rs` `collect_garbage`: queued builds for deleted entities aren't cancelled (handles keep their
       `want`), starving live agents' builds. Clear `want` for handles not in `live` on each GC.
    3. `models.rs` `finish`: a cancelled job's result is applied to a respawned entity waiting on the same key
       (permanently `failed`). Ignore results whose job id isn't `in_flight[key]`; give cancellation its own
       result kind (not `recipe_invalid`/`Limit`); prune `cancelled` ids; test the `catch_unwind` path.
    4. (minor) restoring a broken recipe's exact old text keeps the stale error in `models.library.broken`.
    5. (minor) a recipe folder removed and restored is never polled again.
    The lane's agent context is likely gone next session: start a fresh lane from the branch with this list.
  - **Next, in order:** (1) the Sepulcra integration lane: real procedural models, `specula-anim` creatures,
    the `specula-ui` HUD through `GpuOverlay`, `specula-audio` sound, `--seed` via `Game::configure`;
    (2) the M2 exit proof; (3) M3 pieces the game needs: save games, module host (hot reload), desync detector.
  - **Owner questions waiting:** STYLE.md's three questions, DESIGN.md (proposal) lines, shipping fonts,
    `style.json` proposals (extra palette entries, invented budgets), Steamworks, launch languages.
- **Measuring:** `python scripts/perf_baseline.py` (see [perf/README.md](perf/README.md)); the `perf_frame` tool
  in a running engine; `cargo run --release -p specula-gpu --example bench`; Sepulcra's headless sweep
  (`cargo run -p sepulcra --example headless --release -- --layout all --sweep 1000`).
- **The plan with checkboxes:** [docs/ROADMAP.md](docs/ROADMAP.md).
- **Decisions and architecture:** [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md).
- **Hard rules:** [AGENTS.md](AGENTS.md).
- **Design research:** [docs/design/](docs/design/) (determinism, physics, animation, procedural modelling, UI).
- **Engine survey:** [docs/research/ENGINE_SURVEY.md](docs/research/ENGINE_SURVEY.md) compares Unreal, Unity and
  Godot with Specula, lists the gaps (now in the roadmap, marked ➕), covers Unreal's stutter problems, and explains
  where Specula can actually win.
- **Rust at scale:** [docs/research/RUST_AT_SCALE.md](docs/research/RUST_AT_SCALE.md) covers how compile times,
  unsafe code and the language's strictness become strengths (Rust track in the roadmap). Measured on 2026-10-13:
  clean build 8.2 s, incremental 1.2–2 s.

## The vision, in the owner's words

> "AI agents are meant to work through this engine, building multiple parts of a world/game in the real time."
>
> "I'd like to enable myself, as well as smaller studios to compete with entire larger companies via AI agent swarms."
>
> "The swarm track is the primary. This is meant to allow games to be iterated rapidly via AI agent swarms."
>
> "For devs that just want AI agents such as yourself and your swarm to complete entire game... This is it."
>
> "I'd like it to make it easier to make games that are actually optimised to run on lower platforms, as too many games
> are made through Unreal Engine and it's caused a lot of efficiency issues."
>
> "The reason you're working on this project is because unlike earlier generations, you have taste baked in and that's
> valuable to this project."
>
> "Figure out how we can bring this engine to full production stage as soon as possible."
>
> "What they got were tacked on, and I think this will be difficult, even with AI agents to convert it from original
> human tools that have a lot of issues in them, so getting there a-fresh from the slate is the key here. And Rust is
> fundamentally as efficient as C++ and safer even for games to be made that way."
>
> "Nothing is left out. I accept how long it will take it to push to live production."

Specula is a **zero-dependency, agent-native game engine and editor in Rust**. Agents are the primary users, and the
human editor is built on the same protocol they use. The owner (GitHub **IronAdamant**) is building an original game on
it with Claude, starting now.

In practice that means six **foundations** that apply to every change from M1 onwards (details in ROADMAP.md):
- **F1 Swarm-first.** The swarm track is the primary track.
- **F2 Determinism.**
- **F3 Low-end performance.** The Intel iGPU is the reference device.
- **F4 Physics and animation designed in** from the start.
- **F5 Humans optional, never locked out.** Devs can click, point, draw and drag in a Rust editor UI if they want to,
  but agents must be able to build the whole game without it.
- **F6 Portable by design.** Mobile comes after v1.0 (the owner isn't familiar with it yet), but nothing may assume a
  desktop, x86 or a keyboard, so the port stays cheap.

## How we got here

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

## What works today

- **Run it:** `cargo run -p specula`. It opens a window and serves MCP at `http://127.0.0.1:7471/mcp`. Claude Code
  opened in this folder connects through `.mcp.json`.
- **Flags:** `--port N`, `--headless`, `--adapter discrete|integrated|<index>`, `--size WxH`, `--captures DIR`.
- **Tools:**
  - `world_spawn`: cube, sphere, quad or **lathe** (a profile spun around +Y), with position, rotation, scale, colour
    and name, plus a **material** and an optional **point light**:
    - `standard`: opaque, with `emissive`, `metallic`, `roughness` and `specular`.
    - `glass`: a blended thin shell with Fresnel reflections.
    - `blob_shadow`: a soft contact shadow.
    - `lava_lamp`: a ray-marched lamp.
  - `world_set`: change an entity in place, including `rename`, its look, light and lathe profile.
  - `world_list` and `world_delete`.
  - `world_env`: sun, ambient light, exposure, filmic tone mapping and background.
  - `window_view`: point the human's window camera (or `auto`).
  - `sight_frame`: a PNG plus a subject list with screen boxes and `distance_m`, capped by `max_subjects`. `time_s`
    fixes the animation time, so a frame is reproducible. The full packet is saved to `captures/run-*/`.
  - `world_query` (paged filters), `world_batch` (atomic), `world_save` / `world_load` (text world files),
    `agent_hello` (identity and role).
  - `perf_frame` (GPU ms per pass, draws, memory, pacing, budget errors) and `time_control` (pause, step, scale).
  - A hosted game adds its own `play_*` tools.
  - `engine_info`.
- **Size:** about 15k lines of Rust in 6 crates (`specula-sys`, `-core`, `-platform`, `-gpu`, `-protocol`,
  `specula`). 149 tests. **Zero third-party crates.**
- **Linking:** the Rust toolchain's own linker. Vulkan is loaded at runtime.

**What it can't do yet.** Plan game sessions around these limits:
- **Save/load exists** (`--world NAME` autosaves to `worlds/NAME/`), but there's no journal, undo or claims yet
  (the Journal lane is building them).
- **Visuals are early.** Materials, point lights, glass, emission and tone mapping exist (built for the lava-lamp
  benchmark, 2026-10-13). There are no textures, shadow maps, bloom pass, imported models or 2D sprites yet.
- **Not playable yet.** There's no game logic, input, physics, audio, or text/UI, so nothing can be played until M3.
- **Sight limits:** no occlusion, and sphere boxes are about 30% loose.

**So for the game right now:** design documents, plus **greybox blockouts** built live through MCP (layout, scale,
sightlines). Let the game decide which engine features come first.

## How the owner likes to work

- **Subagents:** they asked for subagents on milestone builds, with **adversarial review before advancing** a
  milestone. They want nothing left open.
- **Pushing:** push only when asked. They usually say "push please".
- **Honesty:** they value directness and corrections. When they believed Bevy was "the first editor that works with
  Rust", a gentle correction was welcome: Bevy is code-first, and Fyrox and Godot-rs exist.
- **Their own builds:** they asked for things built by us rather than borrowed ("full control"), for example their own
  linker. The agreed compromise: the toolchain linker plus raw-dylib already removes third parties, so the custom
  linker is parked as a long-term option.
- **crates.io:** publishing is for the owner to decide later.
- **Naming:** Latin-style names.
- **Test steps:** in-game test plans as `- [ ]` checklists (they use Obsidian).
- **Other work:** Fallout 4 mods in `D:\Coding Projects\AgentCreationKit`. A peer Claude session there once committed
  work by mistake, so check `git log` before committing.

## Gotchas that cost time

- **`RUSTFLAGS`:** an environment `RUSTFLAGS` *replaces* the config's flags. Use
  `RUSTFLAGS="-Dwarnings -C link-self-contained=yes"`.
- **One crate per DLL:** on windows-gnu, two crates importing the same DLL via raw-dylib → the second one's imports
  silently vanish.
- **Linker on PATH:** the LLVM-MinGW gcc first on PATH lacks libgcc (the bundled linker avoids it). In Git Bash, `link`
  is coreutils, not MSVC.
- **Shaders:** compile with `--target-env=vulkan1.3`, or you'll chase phantom shader mismatches. The SDK is at
  `C:\VulkanSDK\1.4.363.0`, and `VULKAN_SDK` isn't in the shell environment.
- **Python:** `-I` ignores `PYTHONIOENCODING`, so keep script output ASCII. Scripts that launch the engine must set its
  working directory or pass `--captures`.
- **Live check:** CI has no GPU. After GPU or window changes, run `python scripts/window_check.py` and **open the PNGs**.
- **MCP tool names:** no dots, because Claude Code rejects them.

## Open owner decisions

These are listed in ROADMAP.md under "Path to production":
- **First game:** the genre is chosen (tower defence, *Sepulcra*). Art style and launch languages are still open.
  v1.0 platforms are decided: Windows + Steam Deck, with mobile later.
- **Steamworks:** may `steam_api64.dll` be loaded at runtime as an optional platform service?

## Claude's opinions and thoughts

**What's strong:**
- **Zero-dep suits agents.** Every line is in this repo, so an agent can read and fix anything, with no version churn
  and no hidden behaviour in dependencies.
- **Sight from the engine's own data.** Agents check images against the engine's packets instead of trusting their
  own perception. That carried over from the Grok-Bevy research and is the project's most distinctive idea.
- **The M0 base is solid.** Validation-clean on two GPU vendors, with structured errors throughout.

**What worries me (honestly):**
- **Scope.** Zero-dep means writing a TrueType rasteriser, a Vorbis decoder, physics, skeletal animation, a UI toolkit
  and three platform backends. Agents make that feasible, but it's still years of work. The owner knows and accepts
  this. Keep every feature in the plan, but **let the game decide the order within each milestone**, and never break
  a foundation (F1–F6) to go faster.
- **Coordination is the real bottleneck for swarms, not rendering.** That's why M1 (claims, versions, journal) matters
  more than pretty pixels, and why I added a swarm track to the roadmap.
- **Content is where small studios lose.** The biggest leverage is M4: procedural modelling tools, `style.toml` hard
  checks and contact sheets. These let agents produce on-style assets while the owner approves in batches.
- **The human stays the director.** Swarms multiply output, not taste. The engine should make the owner's judgement
  cheap to apply: style rules, review queues, and a journal showing which agent built what.
- **Competition is real.** Unity and Unreal shipped MCP servers in 2026, so "agents can drive the engine" is no
  longer unique. Specula wins only on what bolt-ons can't retrofit: swarm concurrency, verification as data,
  deterministic headless playtests, an engine small enough for agents to read, and efficiency by default. Guard
  those six advantages (engine survey §1).
- **Tokens are a cost.** Every tool response should stay bounded. `max_subjects` was the first example of this rule.

**Recommendations for the next session:**
1. **Pick the game first** (2026-10-13: the owner paused to think through a few ideas). Ask for the ideas, rough is
   fine. Ask the questions in docs/GAME_SELECTION.md, score each idea (fit, lanes pulled forward, risks, first-slice
   sketch) and recommend one. **The owner decides.** Then write the game design doc and the style bible, and greybox
   the first space through MCP.
2. If the greybox needs to survive restarts, build **M1 persistence (text save/load) first**. It's small and unblocks
   everything.
3. Then do the rest of M1. Its exit proof (4+ agents building separate districts at once) doubles as the first real
   swarm test.
4. Walk the owner through the five design docs in `docs/design/` and the engine survey. They're researched but not yet reviewed with the
   owner, and their "reviewed" boxes in ROADMAP.md are unticked.
5. At the start of the game, write its **style bible** with the owner (see design/PROCEDURAL_MODELLING.md). The owner
   said Claude's taste is a reason Claude is on this project, so put it to work there: shape language, palette and
   proportions.
