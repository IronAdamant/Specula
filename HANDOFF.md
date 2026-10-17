# Handoff: read this first

These are notes from Claude (Opus 5.5) to whoever picks up Specula next, most likely a new Claude conversation with no
memory of the earlier ones. This page is **current state and what's next** only; history is in
[CHANGELOG.md](CHANGELOG.md). Last updated **2026-10-17**.

- **Where things stand:** M0, M1 and M2 are done (M2 on 2026-10-16; `python scripts/m2_exit.py --adapter both`
  re-checks its exit criteria after `cargo build --release -p specula`). The first game, *Sepulcra* (tower defence),
  is playable as a vertical slice and is the engine's dogfood. **M3 has not started.**
- **Next:** M3 needs the owner's go-ahead. Don't start it unasked. Its plan (owner direction 2026-10-17: games
  built through data and tools, not hand-written code; taste engineered in; agent-native) is in ROADMAP's M3/M4,
  from [docs/research/AGENT_GAMEMAKING.md](docs/research/AGENT_GAMEMAKING.md): Lex (gameplay as data) first, then
  Locus (placement by relations), animation A3-A4 with agent-authored motion and motion checks, physics P1-P4,
  balance lab, agent evals, save games, module host, desync detector. The owner said to commit and **push before
  starting M3**.
- **Pre-M3 cleanup done (2026-10-17):** an outside review (Grok) listed gaps and the owner approved fixing them
  before M3. All merged, each reviewed:
  - current docs; a CI guard against session links in commit messages; a pinned MCP revision test; a Markdown link
    check and one owner-decisions list (`crates/specula/tests/docs_links.rs`); Linux clippy in CI.
  - with `include_ui`, opaque HUD panels (alpha >= 0.9) hide what's under them in sight packets
    (`id_buffer.ui_occluders`; textured interface quads never hide anything).
  - the sky eases into the horizon (no crisp line).
  - the near-white slab under a selected brazier was **the selection mark itself** (pale `RUNE_PALE` times a glow,
    washed out by ACES, bloom and the grade), not the fire light: highlights now use saturated `RUNE_GLOW`
    (`looks::highlight`), and the range-ring dots are cyan too.
  - agent calls are served **between presents** (`crates/specula/src/schedule.rs`): one agent with the window
    open went from about 110-230 to 500-1,270 calls/s; under a 64-agent swarm the window keeps 60 fps (frames used
    to stall for 2-3 s) at about 20% less total throughput. `perf_frame.serve` shows the schedule. Numbers in
    `perf/drills/`.
  - Deferred: CI rendering checks (need Vulkan loading on Linux first; Platform track), per-agent auth tokens
    (needed before any non-localhost bind), outlier-aware auto-framing.
- **How work runs:** as **lanes**: one subagent per crate/area in a git worktree, each attacked by a
  `lane-reviewer` before merge (`.claude/agents/`), and visual claims checked by `sight-skeptic`. Check `git log`
  and the roadmap ticks for where things stand.
- **Lessons:** reviews keep finding real bugs (determinism leaks, unbounded work, device loss, stale caches), so
  keep the rounds; after round 3-4 the integrator verifies narrow final fixes. Measure with the game running, not
  only static worlds (the first draw cache was slower in play). Several sessions share the GPU and ports 7460-7499;
  stop engines by PID only. Usage limits interrupt lanes: resume each with SendMessage.
- **Game docs:** [DESIGN.md](games/sepulcra/DESIGN.md) (sim rules, numbers), [STYLE.md](games/sepulcra/STYLE.md)
  (Claude's style bible draft), in-game test plan [TESTPLAN.md](games/sepulcra/TESTPLAN.md). Capture:
  `docs/fidelity/sepulcra_slice_hud.png`. How the game was chosen: [docs/GAME_SELECTION.md](docs/GAME_SELECTION.md).
- **Measuring:** `python scripts/perf_baseline.py` (see [perf/README.md](perf/README.md)); the `perf_frame` tool
  in a running engine; `cargo run --release -p specula-gpu --example bench`; Sepulcra's headless sweep
  (`cargo run -p sepulcra --example headless --release -- --layout all --sweep 1000`); `scripts/swarm_drill.py`
  for agent throughput.
- **The plan with checkboxes:** [docs/ROADMAP.md](docs/ROADMAP.md).
- **Decisions and architecture:** [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md).
- **Hard rules:** [AGENTS.md](AGENTS.md).
- **Open owner decisions:** the one list is [below](#open-owner-decisions).
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

## What works today

- **Run it:** `cargo run -p specula` opens a window and serves MCP at `http://127.0.0.1:7471/mcp`; Claude Code opened
  in this folder connects through `.mcp.json`. Sepulcra: `cargo run -p sepulcra-app --release -- --port 7486 --
  --seed 7` (see its TESTPLAN.md).
- **Flags:** `--port N`, `--headless`, `--adapter discrete|integrated|<index>`, `--size WxH`, `--captures DIR`,
  `--worlds DIR`, `--world NAME` (autosaves to `worlds/NAME/`), `--cache DIR`, `--assets DIR`, `--style FILE`,
  `--perf FILE`, and `-- GAME ARGS` for a hosted game.
- **Tools** (each one's schema is in `tools/list`):
  - World: `world_spawn`, `world_set`, `world_delete`, `world_list`, `world_query` (paged filters), `world_batch`
    (atomic), `world_env`, `world_save` / `world_load` (text world files), `world_hash` (per-tick simulation hash).
  - Swarm: `agent_hello` (identity and role), `claim_take` / `claim_renew` / `claim_share` / `claim_release` /
    `claim_list` (leases on subtrees and regions), `world_changes` (change feed), `world_history` and `world_undo`
    (the agent-attributed journal), per-entity versions with `expect_version`.
  - Sight: `sight_frame` (PNG plus a packet: exact screen boxes, `visible_px`, `occluded` from the ID buffer,
    overlays), `sight_orbit` (turntable), `sight_compare` (before/after), `sight_query`, named persistent cameras
    (`camera_set` / `camera_list` / `camera_delete`), `window_view` (point the human's window).
  - Assets: `model_build` / `model_reload` (procedural recipes and glTF/GLB), 2D tilemaps (`tilemap_get` /
    `tilemap_set`).
  - Performance and time: `perf_frame` (GPU ms per pass, render graph, draws, memory, pacing, budget errors from
    `perf.toml`), `perf_capture`, `perf_zones`, `perf_memory`, `time_control` (pause, step, scale), `engine_info`.
  - A hosted game adds `play_*` tools (Sepulcra: `play_new`, `play_command`, `play_control`, `play_state`,
    `play_map`, `play_ui`, `play_record`, `play_replay`).
- **Rendering:** Vulkan render graph, PBR-lite and stylised materials, directional and point lights, shadow maps,
  post (tone mapping, FXAA, LUTs), sky, glass, instancing, culling, LODs, GPU skinning, 2D sprites, tilemaps and
  vector shapes, a UI overlay. Reversed Z.
- **Simulation:** fixed-tick clock, seeded RNG streams, deterministic maths (`dmath`), ordered containers, a
  journal-replay test that reproduces `world_hash` in CI.
- **Game-side crates:** `specula-anim` (clips, A1-A2), `specula-audio` (WASAPI, procedural 3D audio),
  `specula-input` (keyboard, mouse, XInput gamepads), `specula-text`, `specula-ui`, `specula-model` (procedural
  modelling, tier 1), `specula-gltf`.
- **Size:** about 129k lines of Rust in 13 crates plus the Sepulcra game and app; about 1,100 tests. **Zero
  third-party crates.** The Rust toolchain's own linker; Vulkan, XInput and AVRT are loaded at runtime.

**What it can't do yet** (most of it is M3+):
- **No physics** (P1-P4 are M3), no gameplay-as-data (Lex), no module host or hot-reloaded game logic yet: Sepulcra's
  rules are hand-written Rust in `games/sepulcra`.
- **Animation** stops at stage A2 (clips and state-driven playback); blending, IK and agent-authored motion are M3.
- **No save games** for players (world files are an editor format), no title screen in the app, settings aren't
  saved and there's no rebinding.
- **Windows only** at runtime: other platforms compile but report "unsupported" (Vulkan loading isn't implemented
  off Windows yet). CI has no GPU, so rendering is checked by hand with `scripts/window_check.py`.
- **Localhost only:** no per-agent auth tokens yet, so the server must never bind beyond loopback.
- **Auto-framing** can shrink to a dot when one object is far away from the rest.

## How the owner likes to work

- **Subagents:** they asked for subagents on milestone builds, with **adversarial review before advancing** a
  milestone. They want nothing left open.
- **Pushing:** push only when asked. They usually say "push please".
- **Honesty:** they value directness and corrections. When they believed Bevy was "the first editor that works with
  Rust", a gentle correction was welcome: Bevy is code-first, and Fyrox and Godot-rs exist.
- **Their own builds:** they asked for things built by us rather than borrowed ("full control"), for example their own
  linker. The agreed compromise: the toolchain linker plus raw-dylib already removes third parties, so the custom
  linker is parked as a long-term option.
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

This is the **one list** of decisions waiting for the owner (other docs link here instead of keeping their own;
`crates/specula/tests/docs_links.rs` checks it isn't duplicated). Remove an item once it's decided and record the
answer where it applies.

- **M3 go-ahead and scope:** approve or trim the M3/M4 plan above.
- **Sepulcra scope:** art style, size and launch languages (genre is decided: tower defence; v1.0 platforms are
  decided: Windows + Steam Deck, mobile later).
- **Steamworks:** may `steam_api64.dll` be loaded at runtime as an optional platform service? It's needed for
  achievements, cloud saves and Steam Input glyphs.
- **Sepulcra questions:** how dark night should be (Claude darkened it; fire and runes carry the scene; STYLE.md
  proposes "readable moonlight"); active pause on a gamepad (P is active pause, Start opens the menu); a title
  screen; saved settings and rebinding; may the fire colour mark "can't afford" (STYLE.md reserves fire for fire).
- **Sepulcra rules** ([DESIGN.md](games/sepulcra/DESIGN.md) has the context): walls only on top-face lanes, or on
  cliff lanes too? Is a boss (colossus) leak meant to be survivable, as now (5 tombstones, about 30 % of good runs), or a
  must-kill? Edge reach of 3 m (two cell rows) across faces: too generous or too strict? Should rune pillars slow
  wraiths (now only the obelisk answers flyers)? Calling waves early pays 0.5 dust per second left: should it be a
  real economy lever? Is 8 m per wall a good weight for breaking walls?
- **STYLE.md's questions:** floating slab over mist, or a dark canyon? Cyan magic vs green dead?
- **Other waiting items:** DESIGN.md's **(proposal)** lines, shipping fonts, `style.json` proposals, and reviewing
  the five `docs/design/` docs (their "reviewed" boxes in ROADMAP are unticked).
- **MCP revision:** when to move to MCP 2026-07-28, which drops `initialize` and sessions (we pin 2025-11-25;
  `version_negotiation` in `crates/specula-protocol/tests/mcp_http.rs` asserts it).
- **crates.io:** whether and when to publish the crates.
- **Window under a swarm:** the window keeps at least 60 fps while agents are backed up (`BUSY_FRAME` in
  `crates/specula/src/schedule.rs`), which costs a 64-agent swarm about 20% throughput. Keep 60, or make it a
  `perf.toml` setting?
- **Selection look:** selected stones now glow saturated cyan (glow 2.0) instead of the old pale mark that
  rendered near-white. Is that the look you want?
- **Remote agents:** are any remote or multi-machine agents planned for M3? If so, per-agent auth tokens come first.

## Claude's opinions and thoughts

**What's strong:**
- **Zero-dep suits agents.** Every line is in this repo, so an agent can read and fix anything, with no version churn
  and no hidden behaviour in dependencies.
- **Sight from the engine's own data.** Agents check images against the engine's packets instead of trusting their
  own perception. That carried over from the Grok-Bevy research and is the project's most distinctive idea.
- **The base is solid.** Validation-clean on two GPU vendors, structured errors throughout, and three milestones
  that each survived several adversarial review rounds.

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
1. **Ask before M3.** If the owner gives the go-ahead, push first (they said so), then start with Lex (gameplay as
   data), since Locus, the balance lab and agent evals all build on it. Run it as lanes with reviews, as in M2.
2. **Bring the open owner decisions** (the list above) to the owner in one short batch, with Claude's recommendation
   for each; several of them (night darkness, title screen, settings) shape Sepulcra's M3 work.
3. **Keep Sepulcra the dogfood.** Each M3 feature should land in the game (rules moved into Lex data, physics on
   creatures, motion checks on its animations), measured on the Intel iGPU with the game running.
4. **Before any non-localhost bind**, build per-agent auth tokens (ROADMAP swarm track). Before CI can check
   rendering, Vulkan loading must work on Linux (Platform track).
5. **Walk the owner through the five design docs** in `docs/design/` and the engine survey when there's a natural
   pause; their "reviewed" boxes are still unticked.
