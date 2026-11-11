# Handoff: read this first

These are notes from Claude (Opus 5.5 through M5's first wave; Fable 5.2 from 2026-11-12) to whoever picks up Specula
next, most likely a new Claude conversation with no memory of the earlier ones. This page is **current state and
what's next** only; history is in [CHANGELOG.md](CHANGELOG.md). Last updated **2026-11-12**.

- **Owner, 2026-11-12: what the engine is for, and how M5 finishes.**
  - **The engine is leverage, not a gate.** Agents can already build games without an engine; Specula exists so they
    spend fewer tokens for a better result ("making efficient use of your tokens, and reducing token waste while
    maximising the results"). A shipped game uses the runtime but never needs the agent tools or the editor (M6
    `specula export`).
  - **Who works through it:** one agent or a swarm. Humans ask (highlight and ask), edit directly in the editor, or
    choose (galleries, the review screen).
  - **Mods and extensions are M6, and live on the other side.** People's own tooling, mods and external engine
    pieces plug in from their system and never enter this repository; Specula stays zero-dependency and validates
    what plugs in. Whether a game is moddable is its human developers' choice.
  - **One builder.** From this date M5 is finished by a single primary agent, lane by lane, with subagents only for
    independent review (the parallel lanes hit the usage limit twice, conflicted in shared files and made
    performance numbers noisy). The review rule stands: code is reviewed by someone who didn't write it.

- **Where things stand:** M0, M1, M2 and **M3 are done** (M3 on 2026-10-22, after an integration audit; `python
  scripts/m3_exit.py --adapter integrated` re-checks the iGPU budget with a release `sepulcra-app`; M2's check is
  `scripts/m2_exit.py`). The first game, *Sepulcra* (tower defence), is playable and is the engine's dogfood.
  **M4 is done (2026-10-30):** the owner approved the exit proof's 33 props at the window (`scripts/m4_exit.py`), and
  a final integration audit passed before the push (2026-11-05, CI green). **M5 started 2026-11-06.** The owner's
  answers at its start: Sepulcra's props get a **per-prop `blocks_build` flag** as data (big pieces block building,
  small dressing doesn't); Claude's M4 style proposals are **confirmed** (the building, ruin, terrain, set and `prop`
  categories, fire colour on candles and lanterns, the `wood` preset, the `*_t3` budget); and **every M5 lane uses
  the owner's seven agent tools** (AGENTS.md "Agent tools"; the live test is in
  [docs/agent-tools/LIVE_TEST_2026-11-05.md](docs/agent-tools/LIVE_TEST_2026-11-05.md)), reporting what helped and
  what got in the way.
- **Started early:** the owner had planned to restart on 2026-10-21, but upgraded their plan and gave the go-ahead
  the night of 2026-10-17. The pre-M3 work is pushed (`d634afe`, CI green).
- **Owner answers, 2026-10-17** (diary):
  - **Window frame floor under a swarm: make it a setting**, chosen by the controller (lead) AI before it deploys
    a swarm, from how strong the human operator's machine is. Low- and mid-spec laptops can't spare frames (a mid-spec
    one might for 2D, depending on its hardware); high-spec laptops, desktops and cloud hardware can. Today it's the
    constant `BUSY_FRAME` (60 fps) in `crates/specula/src/schedule.rs`; the ROADMAP swarm track has the item.
  - **The stronger cyan selection mark: yes**, keep it (STYLE.md's highlight rule stands).
- **M3 and M4 approved (owner, 2026-10-17 late: "I'd like you to finish M3 and M4 please").** Work runs in waves of
  lanes: wave 1 (Lex, physics P1-P3, animation A3 and agent-authored motion, swarm communication, module host and
  SDK); wave 2 (Locus and navigation/AI, Sepulcra in Lex with save games and the balance lab, P4 and A4 ragdolls,
  playtest swarms and tool ergonomics, streaming and HLOD, then the M3 exit proof and agent evals); wave 3 (M4: stage
  world, contact sheets and taste checks, galleries and preferences, review queue, procedural modelling tiers 2-4,
  baked GI, VFX, terrain, then the M4 exit proof). Check `git log` and ROADMAP ticks for progress.
  **Owner, 2026-10-18: shaders get a language of our own,** compiled to SPIR-V, MSL and later the console APIs, built
  first on the Platform track (ROADMAP), so agents write a shader once for every platform.
  **Owner, 2026-10-22, the release plan:** Windows first, all the way through a Windows-focused M6, then the
  announcement. After that, macOS and Linux, tested on the owner's Windows, macOS and Arch Linux machines with
  Sepulcra and the Steam MVP. Those ports are **native** (Metal, and the Linux window systems directly), never
  translation layers such as MoltenVK or Wine/Proton, because those add overhead and dependencies. "Many people are
  on Windows OS anyway."
  **Decided by the owner on 2026-10-22** (Claude's recommendations, answered by the owner; full notes in the owner's diary):
  - **Steamworks: yes, as an optional extra.** `steam_api64.dll` is loaded at runtime, and games run and build
    without it. Docs must tell human devs and agents this plainly.
  - **Remote and cloud agents: local only until M6.** Lay the foundation notes for cloud swarms at the end of M5.
    In M6, build per-agent auth tokens and test cloud swarms thoroughly (Claude, Grok Build, OpenAI and other
    vendors' swarms). Then run an adversarial review **before any marketing**.
  - **MCP 2026-07-28: wait** until Claude Code supports it, then switch in one small lane. Testing needs restarts:
    handle them with subagents, or ask the owner up front.
  - **crates.io: after M6.** First verify on macOS and Linux with small games; if it holds up, publish to
    crates.io, then market. M6 is effectively proto-v1.0. The Specula engine is **open source**.
  - **A private held-out eval set: yes, at M6.** Claude reminds the owner at M6, before any benchmark claims.
  - **Sepulcra v1.0:**
    - Three maps of about 10 waves, using **every face of the cube, sides and bottom included** (a "cubic
      globe").
    - Map 1 is the dark graveyard. Map 2 is the opposite, a frozen wasteland with unique mobs and frost versions of
      the towers. Map 3 is volcanic, a mix of light and fire.
    - English first; other languages only if the Steam page shows demand.
    - Staged: map 1 is the prototype on all six faces, which proves the camera and the pathing. Maps 2 and 3 reuse
      the same cube base with different paths.
    - Camera: right-click orbits the cube; it never flips upside-down. Paths stay readable across edges.
    - **Sepulcra stays free and open** (MIT/Apache, like the engine), released as a free demo on Steam to show
      what AI agents can build when humans direct them well. The owner's paid games will live in their own
      private repos, using the engine as a dependency.
  - **Security baseline:** OS-provided TLS only, never our own crypto. Operators secure their own environments
    (AGENTS.md rule 12).
  - **The realism range:** quality presets span "simple, gameplay first" to "really good on a strong PC".
    The low preset always holds budget on the iGPU.
  - **Sepulcra look:**
    - readable moonlight at night;
    - the floating slab over mist, made stylish;
    - cyan for magic, green for the dead;
    - moss may exceed the saturation rule on map 1, and real grass blades should replace flat green;
    - an iron fence;
    - fire colour is for fire only (dim or desaturated shows "can't afford");
    - open-licence (SIL OFL) fonts.
  - **Sepulcra rules:**
    - walls on top-face lanes only;
    - a boss leak is survivable but costly;
    - keep the 3 m edge reach and 8 m per wall;
    - rune pillars don't slow wraiths;
    - the early-call bonus is real but small.
  - **Controls:** gamepad pause, title screen, saved settings and rebinding, in M5/M6. Each game chooses keyboard
    and mouse, controller, or both; agents recommend both.
  - **The five design docs are reviewed,** delegated to Claude. The owner's notes on them:
    - **ARM:** test determinism on their M2 MacBook.
    - **Physics:** aim as high as possible later.
    - **Animation:** "good enough" at M6, better after v1.0.
    - **Procedural modelling:** lay the foundations now for realistic, organic content (Unreal-class quality, but
      more efficient) after v1.0, starting stylised.
    - **UI:** stays zero-dependency, screen reader included.
  **Owner, 2026-10-22: M4 approved** ("Finish M3 and M4 now fully, and with full effort please"): finish M3, then run
  wave 3 (M4) straight after, with the same review rounds.
  **Merged so far (2026-10-18):** all of wave 1 (swarm communication, the module host, physics P1-P3, Lex,
  animation A3), and from wave 2 the playtests and tool ergonomics (the 20+ replay exit proof passes), the visual
  history gallery, save games with the balance lab, streaming with HLOD, Sepulcra fully in Lex with live Lex, Locus with navigation and AI, and (2026-10-22) determinism hardening
  (module builds journaled, the FP-environment guard, determinism lints), the streaming upload and pump fixes, and
  agent evals ([docs/design/EVALS.md](docs/design/EVALS.md), `scripts/evals.py`), project data through the
  protocol (`project_*`), and P4 joints with A4 ragdolls. All three waves are merged; M3 and M4 are done.
- **M3 plan:** Its plan (owner direction 2026-10-17: games
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
    (needed before any non-localhost bind). Outlier-aware auto-framing was done at the end of M4.
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
- **Visual history (owner, 2026-10-18, "for historical review"):** [docs/gallery](docs/gallery/README.md). After
  every live run (tests, playtests, assets, animation, physics, navigation), add 2-6 representative captures with
  `python scripts/gallery.py add ...` (`gallery.py subjects` lists the subjects; `dupes` and `drop` keep it lean). Thumbnails
  are tracked; full-size originals go to the main checkout's gitignored `gallery-full/`.
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
  `--perf FILE`, `--paused` (start with the clock stopped, for replays), `--response-format concise|detailed`,
  `--tools-default core|all` (agents list 6 core tools unless they ask; `.mcp.json` and the scripts ask for all),
  `--owner NAME` / `--lead NAME` / `--reviewer NAME` (operator grants: roles are never self-declared),
  `--trace DIR`, `--trace-file on|off`, `--strict-budget` (an engine-thread stall fails the run), `--terrain on|off`,
  and `-- GAME ARGS` for a hosted game. Agent names are plain ASCII.
- **Answers are concise by default** on the heavy tools; pass `response_format: "detailed"` (per call, or once in
  `agent_hello`) for everything. Repo scripts use `scripts/specula_mcp.py` or `specula_mcp.detailed()`.
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
  - Coordination: `task_*` (the task board), `intent_*`, `note_*`, `world_rollup`, `lease_*` (ports, GPU time,
    scratch folders), `swarm_settings` (the frame floor; lead only), `desync_trace` / `desync_compare`.
  - Gameplay as data (Lex): `lex_schema`, `lex_list`, `lex_get`, `lex_edit`, `lex_validate`, `lex_simulate`,
    `lex_import`, `lex_export`, `lex_session` ([docs/design/LEX.md](docs/design/LEX.md)); `lex_live` runs a set live in the
    engine. Sepulcra plays from its Lex rules with `-- --rules lex`.
  - Placement and navigation: `world_place` (relations), `nav_build`, `nav_path`, `nav_point`, `nav_agent`,
    `nav_info`, `ai_check`, `ai_set`, `ai_status`, `model_collider`.
  - Animation: `anim_get`, `anim_set` (clips and graphs as data), `anim_state`, `anim_check` (motion checks),
    `anim_filmstrip` (contact sheets with checks); graph parameters via `world_set {anim: {graph, params}}`.
  - Joints and ragdolls: `physics_joint`, `physics_joints`, `ragdoll_set`, `ragdoll_info`; the `props.ragdoll`
    component; `--physics-threads N` (an opt-in deterministic parallel solver).
  - Physics: `physics_raycast`, `physics_cast`, `physics_overlap`, `physics_contacts`, `physics_events`,
    `physics_info`, `physics_explain`, `physics_check`, `physics_impulse`; `collider` and `body` components;
    `world_env {physics}` (gravity, substeps, sleep, 2D mode).
  - Streaming: `stream_state`, `stream_settings` (lead only), `stream_fly`; distant cells draw as merged HLOD proxies.
  - Saves and balance: `save_write`, `save_read`, `save_list`, `save_delete` (`--saves DIR`); `balance_targets`,
    `balance_sweep`, `balance_tune`.
  - Playtests: `playtest_run` (a spec as data: steps, expectations, replays) and `playtest_report`.
  - Project files: `project_list`, `project_read`, `project_write`, `project_patch`, `project_delete`,
    `project_mount` (recipes, UI, sounds, styles and module sources with claims, versions, journal and undo; outside
    edits come in as `disk`; [docs/design/PROJECT_DATA.md](docs/design/PROJECT_DATA.md)).
  - Asset studio: `studio_stage`, `studio_sheet`, `studio_lineup`, `studio_compare`, `studio_check`, `style_check`
    ([docs/design/ASSET_STUDIO.md](docs/design/ASSET_STUDIO.md)).
  - Procedural modelling: `procgen_texture`, `procgen_terrain`, `procgen_scatter`
    ([docs/design/PROCEDURAL_MODELLING.md](docs/design/PROCEDURAL_MODELLING.md)).
  - Review and direction: `review_submit`, `review_list`, `review_decide`, `review_screen`, `critique_add`,
    `critique_list`, `critique_update`, `merge_suggest`, `task_plan`, `task_progress`
    ([docs/design/REVIEW.md](docs/design/REVIEW.md)); the window's review screen opens with F9.
  - Effects: `vfx_get`, `vfx_set`, `vfx_preview`, `vfx_check`, `vfx_info`; the `vfx` and `decal` components; `--vfx`
    ([docs/design/VFX.md](docs/design/VFX.md)).
  - Galleries and taste: `gallery_create`, `gallery_get`, `gallery_list`, `gallery_open`, `gallery_close`,
    `gallery_wait`, `gallery_decide` (owner), `gallery_round`, `gallery_apply`, `gallery_delete`, `taste_query`,
    `taste_suggest`, `board_measure`, `board_check`, `style_onboard`; the window's gallery screen opens with F8
    ([docs/design/GALLERIES.md](docs/design/GALLERIES.md)).
  - GI, terrain and water: `gi_bake`, `gi_info`, `gi_check`, `gi_settings`; `terrain_info`, `terrain_edit`; the
    `water` component ([docs/design/GI.md](docs/design/GI.md), [docs/design/TERRAIN.md](docs/design/TERRAIN.md)).
  - Disk and binary files: `disk_status`, `disk_prune`, `project_upload`
    ([docs/design/DISK.md](docs/design/DISK.md)).
  - Your tool list: `tools_catalog`, `tools_activate`, `tools_deactivate`, `tools_invoke`, `tools_request`,
    `tools_scope` (lead) ([docs/design/TOOLS.md](docs/design/TOOLS.md)).
  - Tracing and the debugger: `trace_query`, `trace_follow`, `trace_call`, `trace_settings`, `trace_crash`,
    `trace_incidents`, `trace_incident` ([docs/design/TRACE.md](docs/design/TRACE.md),
    [docs/design/DEBUGGER.md](docs/design/DEBUGGER.md)).
  - Game logic: `logic_build` (diagnostics as data), `logic_reload`, `logic_unload`, `logic_status`, `logic_input`,
    `logic_hook` (hot-reloaded Rust modules; see [docs/design/MODULES.md](docs/design/MODULES.md)).
  - Performance and time: `perf_frame` (GPU ms per pass, render graph, draws, memory, pacing, budget errors from
    `perf.toml`), `perf_capture`, `perf_zones`, `perf_memory`, `time_control` (pause, step, scale), `engine_info`.
  - A hosted game adds `play_*` tools (Sepulcra: `play_new`, `play_command`, `play_control`, `play_state`,
    `play_map`, `play_ui`, `play_record`, `play_replay`).
- **Rendering:** Vulkan render graph, PBR-lite and stylised materials, directional and point lights, shadow maps,
  post (tone mapping, FXAA, LUTs), sky, glass, instancing, culling, LODs, GPU skinning, 2D sprites, tilemaps and
  vector shapes, a UI overlay. Reversed Z.
- **Simulation:** fixed-tick clock, seeded RNG streams, deterministic maths (`dmath`), ordered containers, a
  journal-replay test that reproduces `world_hash` in CI.
- **Game-side crates:** `specula-anim` (A1-A3: clips, graphs, IK, motion generators and checks), `specula-audio` (WASAPI, procedural 3D audio),
  `specula-input` (keyboard, mouse, XInput gamepads), `specula-text`, `specula-ui`, `specula-model` (procedural
  modelling, tier 1), `specula-gltf`.
- **Size:** about 317k lines of Rust (2026-10-30): 301k in 25 engine crates and 17k in 3 game crates (Sepulcra's
  rules and app, and the Ambulo example module); about 2,350 tests. **Zero
  third-party crates.** The Rust toolchain's own linker; Vulkan, XInput and AVRT are loaded at runtime.

**What it can't do yet** (most of it is M3+):
- Sepulcra uses physics only for its shamblers' ragdoll deaths (gameplay is still rules, not physics). It runs on its
  Rust rules by default (`--rules lex` plays it from Lex). Modules can't load off Windows yet (no `dlopen`), and a
  module stuck in a loop that makes no requests can't be preempted.
- No title screen in the app, settings aren't saved and there's no rebinding.
- **Windows only** at runtime: other platforms compile but report "unsupported" (Vulkan loading isn't implemented
  off Windows yet). CI has no GPU, so rendering is checked by hand with `scripts/window_check.py`.
- **Localhost only:** no per-agent auth tokens yet, so the server must never bind beyond loopback.
- **The window's own auto camera** (no camera set) still fits every object, so one far-away object shrinks it; `sight_frame` leaves outliers out and names them.

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
- **Disk:** drive D: is nearly full; lane worktrees each grew 8-15 GB of `target/`. Build into C: with
  `CARGO_TARGET_DIR` (lanes use one folder each), and delete a merged worktree's `target/`.
- **Firewall:** never bind `0.0.0.0`, even to test a port: Windows asks about every new binary that listens beyond
  loopback (leases read the OS listener table instead).

## Open owner decisions

This is the **one list** of decisions waiting for the owner (other docs link here instead of keeping their own;
`crates/specula/tests/docs_links.rs` checks it isn't duplicated). Remove an item once it's decided and record the
answer where it applies. The owner decided everything else on 2026-10-22 (see "Decided by the owner on 2026-10-22" above).

- **May reviewers open screens on the window?** (from the input sweep, 2026-11-07.) A granted reviewer, below lead,
  can put the review screen (F9) and a gallery (F8) on the human's window. Until the owner decides, those opens
  raise an alert. Claude recommends keeping it with the alert: reviewing is what the grant is for.
- **A token-cost comparison** (Claude's proposal, 2026-11-12): build the same small game task once through the
  engine and once from scratch, counting tokens and checking the result, so the engine's leverage is a measured
  number. In the M5 exit proof, or in M6's eval set?
- **Sepulcra balance (from the balance lab, 2026-10-18):** a rusher who calls every wave early still loses 2 runs in
  16 on unseen seeds. Claude recommends keeping it risky. The owner will playtest it in M6, on each OS, and decide
  then ([docs/design/BALANCE.md](docs/design/BALANCE.md)).

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

**How M4 ran (2026-10-28 to 2026-10-30), and what to keep for M5:**
- **The process changes held:** the one write policy, an integration check after every 3-4 merges, every reviewer
  probe becoming a permanent test, at least one independent review per lane, and dogfooding in the real game.
- **Recurring bug classes were fixed structurally,** not one at a time. Investigators reproduced each class, then a
  structural lane removed it: authority and jobs (AGENTS rule 14), bounds, workers and traces (rule 15). The
  checklists in the lane brief come from them.
- **The lead picks each agent's tools** (`tools_scope`). Agents start with the 6 core tools.
- **Traces and incidents first:** a debugger agent watches `trace_incidents` beside builders, and files critiques.
- **The final audit still found bugs between lanes** (a reply read without waiting, a call that made two commits, a
  field read after it moved). Keep the final audit.
- **Disk:** delete a lane's target folder when it merges, and build with `CARGO_INCREMENTAL=0` when space is short.

**Recommendations for the next session:**
1. **M5 next** (after the owner's weekly reset). Run lanes with reviews, as in M3 and M4. First remind the owner
   of the open decisions (the list above), and pilot their agent tools on one lane.
2. **The approved exit-proof props live in the approved world,** not yet in a fresh Sepulcra's map 1. Putting them
   in the shipped map is an M5 step, tied to the decoration question (should props block building?).
3. **Keep Sepulcra the dogfood.** Each M5 feature (audio, text, the editor) should land in the game, measured on
   the Intel iGPU with the game running.
4. **Before any non-localhost bind**, build per-agent auth tokens (ROADMAP swarm track). Before CI can check
   rendering, Vulkan loading must work on Linux (Platform track).
5. **Walk the owner through the five design docs** in `docs/design/` and the engine survey when there's a natural
   pause; their "reviewed" boxes are still unticked.
