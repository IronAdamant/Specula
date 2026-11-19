# Specula — agent rules

Specula is a **zero-dependency, agent-native game engine and editor in Rust**. Plan: [docs/ROADMAP.md](docs/ROADMAP.md)
(checkboxes + reasoning) and [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md) (decisions, architecture). New to the project?
Read [HANDOFF.md](HANDOFF.md) first.

## Hard rules

1. **Zero third-party crates.** Only Rust `std` and workspace path dependencies. `Cargo.lock` must list workspace
   packages only (`crates/specula/tests/zero_dep.rs` enforces it). Not even dev-dependencies.
2. **OS/driver imports via raw-dylib.** Declare foreign functions with
   `#[link(name = "user32", kind = "raw-dylib")] extern "system" { … }` — never rely on import libraries.
   Drivers that may be missing (e.g. `vulkan-1`) are loaded **at runtime** via `specula_sys::dynlib::Library`:
   `Library::open` takes a bare file name and loads **from System32 only**, so a DLL planted beside the executable is
   never loaded. A DLL that ships with the game (`steam_api64.dll`) uses `open_beside_exe`, and a file the engine wrote
   itself (a module copy) uses `open_path`. No crate declares `LoadLibraryW` (docs/design/SECURITY.md).
   **Each DLL is declared in exactly one crate** — on windows-gnu a second crate's raw-dylib imports from the same
   DLL are silently dropped (crash at call time). `kernel32` → `specula-sys`; `user32`, `gdi32`, `winmm` and `imm32` →
   `specula-platform`; `ole32` → `specula-audio` (`crates/specula/tests/dll_owners.rs` checks this list against the
   code). Runtime-loaded through `dynlib` (any crate may load them): `vulkan-1`,
   `xinput1_4`/`xinput9_1_0` and `shcore` (specula-platform: gamepads; a monitor's physical size), `avrt` (specula-audio),
   `iphlpapi` (specula-sys: which ports have listeners), `bcrypt` (specula-sys `digest`: SHA-256 from the OS, for
   what the engine vouches for, such as recorded review evidence; never hand-rolled, rule 12).
   Tests/examples must not re-declare imports of a DLL their dependencies already declare.
3. **Linker = the Rust toolchain's own** (`.cargo/config.toml`: `-C link-self-contained=yes` on windows-gnu).
   Do not set other linkers or add build scripts that call external tools.
   **Gotcha:** an environment `RUSTFLAGS` *replaces* config rustflags — to deny warnings locally use
   `RUSTFLAGS="-Dwarnings -C link-self-contained=yes"`.
4. **Every crate compiles on non-Windows.** Windows-only code lives behind `#[cfg(windows)]`; other platforms return
   a clear "unsupported on this platform" error at runtime.
5. **Structured errors.** Error types carry machine-readable fields (codes, offsets, VkResult) — agents debug from data.
6. **Conventions** (see `specula-core::math` docs): right-handed, +Y up, metres; column-major matrices;
   Vulkan clip (y down, depth 0..1); screen pixels origin top-left; colours given to APIs are sRGB.
   The engine renders with **reversed Z** (`perspective_vk_reverse_z`, `FrameDesc::reverse_z = true`: depth 1 at
   the 0.05 m near plane, 0 at infinity) so huge and distant worlds keep depth precision.
7. **Tests.** Pure code gets unit tests. Tests needing a GPU or a window must **skip cleanly** (print why, pass) when
   Vulkan / a desktop session is unavailable, so CI without a GPU stays green.
8. **`unsafe`** only at FFI boundaries, each block with a `// SAFETY:` comment.
9. **No warnings**: CI builds with `-Dwarnings`.
10. **Foundations F1–F6** in [docs/ROADMAP.md](docs/ROADMAP.md) apply to every change from M1 on: swarm-first tools,
    deterministic simulation ([docs/design/DETERMINISM.md](docs/design/DETERMINISM.md)), low-end performance (Intel
    iGPU is the reference), physics/animation designed in, humans optional but never locked out, portable by design
    (mobile comes later; nothing may assume desktop, x86 or a keyboard).
    **Determinism lints:** every crate inherits `[lints] workspace = true` and is classified in
    `crates/specula/tests/determinism_lints.rs`; simulation crates carry an identical copy of
    `crates/specula-physics/clippy.toml` (no `HashMap`/`HashSet`, platform float maths or rounding — use
    `specula_core::dmath` — wall-clock time or threads). Allow an exception only with a reason. Clippy caches don't
    notice a changed `clippy.toml`: `cargo clean -p <crate>` first.
11. **No session or share links in commit messages** (claude.ai sessions and the like): the repo is public. The CI job
    `commit-messages` rejects them in new commits; don't rewrite older history.
12. **Security first** (owner, 2026-10-22). Before you run Specula anywhere new (another OS, a server, a cloud
    swarm), check its security first.
    - The engine binds to loopback only, until per-agent auth tokens **and** TLS exist.
    - TLS always comes from the operating system: SChannel on Windows, Network.framework on macOS. On Linux, which
      has no OS TLS, use a tunnel (SSH, WireGuard or Tailscale).
    - **Never hand-roll cryptography.**
    - Specula sets a baseline. Operators, and the organisations they work for, secure their own environments.
13. **One write policy** (owner, 2026-10-28). Every change to the live world goes through `write_policy`
    ([docs/design/WRITE_POLICY.md](docs/design/WRITE_POLICY.md)).
    - Writers get `&mut dyn WorldWrite`, never `&mut World`. The live journal is written only through the policy.
    - A new write path picks an entry point and a named `Rule` or `Exempt`. An exemption is a named variant with
      its reason, never a missing check.
    - Never add a `World` field, a `&mut World` accessor or a hatch call outside its owner.
    - Add a row to the regression table (`every_write_path_holds_the_policy`). `crates/specula/tests/write_policy_guard.rs`
      enforces the rest.
14. **Authority and jobs** (2026-10-29). Who may do what comes from `authority::authority` (or
    `authority::of_session` on caller threads), never from a role string: owner, lead and reviewer come only from
    operator grants (`--owner`, `--lead`, `--reviewer`). Every tool declares its access in `authority::access` (a
    test fails without it), and the authority drill must hold. A deferred answer is a `jobs::Reply` (owners, busy
    refusals that name the owner, owner-only cancel). One tool call makes at most one commit
    ([docs/design/AUTHORITY.md](docs/design/AUTHORITY.md)).
15. **Bounds, workers and traces** (2026-10-30). Every tool argument has a limit in its schema and every answer a
    ceiling ([docs/design/TOOLS.md](docs/design/TOOLS.md)). Blocking work (disk, encodes, GPU waits, walks of the
    world) never runs on the engine thread: use `Engine::offload` or `Engine::step_job`
    ([docs/design/WORKERS.md](docs/design/WORKERS.md)); `engine_thread_guard`, `disk_guard` and `budget_check.py`
    enforce it. New output is a `trace_event!`, never a print (`print_guard`); agents debug from traces
    ([docs/design/TRACE.md](docs/design/TRACE.md)). Every new tool needs a family and a one-line catalog entry
    (`toolscope.rs`). Facts about what was drawn come from the frame record, and agent-reported values never show as
    engine results.
16. **Rust only, and one Rust version** (owner, 2026-11-16: "The only programming language used here should be
    Rust").
    - **No new Python, shell or PowerShell.** A new check or tool is Rust, in `tools/examen`. The scripts in
      `scripts/` are being ported there and go when their ports are proved (ROADMAP M6, "The repository's own
      tooling in Rust"). Data (JSON, TOML, CI's workflow files) is not a programming language.
    - **The Rust version is pinned** in `rust-toolchain.toml`. It is the declared minimum (`rust-version`), CI
      installs it by number, and a test holds the three together. rustup reads the file in this checkout, so
      run cargo here with no `RUSTUP_TOOLCHAIN` and no `+toolchain`; **never change a machine's default
      toolchain for it.** The pin moves on purpose, as a change of its own, through `gate --all`: a newer
      Rust changes what clippy asks for and can change what Cargo does (HANDOFF, "Gotchas").

## Layout

| Crate | Role |
|---|---|
| `specula-sys` | the only `kernel32` imports; runtime library loader (`dynlib`); Ctrl+C / console-close handling (`interrupt`) |
| `specula-core` | JSON, math, base64, checksums, DEFLATE/zlib, PNG; structured tracing (`trace`) |
| `specula-platform` | native windows + input (Win32 first); another process's window as a live check meets it (`probe`) |
| `specula-gpu` | Vulkan (runtime-loaded) + render-hardware interface |
| `specula-protocol` | HTTP/1.1 + MCP server for agents |
| `specula` (bin) | world server: world, rendering, agent tools |
| `specula-input` | actions, not keys: keyboard, mouse, gamepad (XInput), touch |
| `specula-audio` | output (WASAPI), mixing, synthesis, analysis |
| `specula-text` | TrueType fonts → laid-out text: a glyph atlas (CPU) for interfaces, curves drawn on the GPU for text in the world; bundled faces, system CJK faces, editing |
| `specula-ui` | HUD and menu toolkit for agents and humans |
| `specula-anim` | skeletal animation (A1–A3): clips, graphs, IK, motion generators, motion checks |
| `specula-lex` | Lex: gameplay rules as typed text data (validate, compile, simulate deterministically) |
| `specula-balance` | balance lab: targets as data, deterministic sweeps and tuning over Lex parameters |
| `specula-ludus` | the game file as data: phases, win and loss conditions, score, settings, a button's words, and the run's state machine the project shell follows |
| `specula-corpus` | the human body as numbers: 114 dimensions by sex (93 measured on 6,068 people, their age, 20 links) and how each follows stature, weight and fourteen named traits; a frame of joints; what each part weighs; the checks of proportion and balance ([docs/design/CORPUS.md](docs/design/CORPUS.md)) |
| `specula-formae` | the shape library: a table of named shapes an agent starts from by name and changes by numbers (plane figures, curves and solids; prisms, pyramids and solids of revolution of any one-ring figure), each with parameters, units and ranges, aliases, closed-form measures, anchors and an exact point count before building |
| `specula-memoria` | what a project remembers: notes with a kind and paths, superseding, finding, the brief an agent is given at its first call |
| `specula-physics` | collision queries, triggers, character controller, rigid bodies (stages P1–P3) |
| `specula-via` | navigation: tiled navmeshes, paths, off-mesh links, crowds |
| `specula-mens` | AI as data: behaviour trees and utility AI, validated, with a deterministic runner |
| `specula-locus` | placement by relations (`world_place`'s solver) |
| `specula-model` | procedural modelling: recipes → meshes, LODs, colliders; CSG, SDF (with bone chains and columns of sections), terrain, scatter, texture graphs; a person's surface from a measured body (`figure`: [docs/design/FIGURE.md](docs/design/FIGURE.md)) |
| `specula-studio` | the asset studio's measures: `style.toml` rules, asset facts, taste checks, critiques |
| `specula-scintilla` | particles/VFX and decals as data: effects, a closed-form evaluator, budgets, photosensitivity checks |
| `specula-gustus` | taste direction: gallery spaces and spreads, pick-driven tuning, the preference store, board measures, style presets |
| `specula-lux` | baked GI: a BVH ray caster and deterministic irradiance probe volumes (LUX1 files) |
| `specula-gltf` | glTF 2.0 / GLB import and export |
| `specula-sdk` | the one crate game-logic modules depend on: deterministic `Cx` API, versioned C ABI |
| `specula-logic` | module host internals: library loading, the build service, diagnostics as data |
| `specula-lingua` | localisation as data: string tables, locale chains, CLDR plural rules, number formats, a pseudo-locale, checks |
| `specula-tempus` | timelines, sequencer tracks and camera rails as data |
| `games/sepulcra`, `games/sepulcra-app` | the first game: headless rules, and the app hosted by the engine |
| `games/ambulo` | example module: a walking, jumping character (the module host's proof) |
| `tools/examen` (`specula-examen`) | the repository's own tooling, in Rust: the gate, the live checks' runner, what a change needs verified, the document guards, and the tables computed from public data (`examen corpus tables`: the body's) |
| `games/inventarium`, `facultas`, `colloquium`, `quaestio`, `exemplum` | gameplay modules as SDK examples: inventory, abilities, dialogue, quests, and a playable demo of the four |

## Tooling on this machine

- Toolchain `stable-x86_64-pc-windows-gnu`. Vulkan SDK: `C:\VulkanSDK\1.4.363.0` (`Bin\glslc.exe`,
  `Bin\spirv-val.exe`; validation layer registered system-wide). `VULKAN_SDK` may not be in the shell environment —
  use the full path.
- Shaders: GLSL sources in `crates/specula-gpu/shaders/`, compiled to `.spv` with `glslc` **by hand** and committed;
  the crate embeds the `.spv` with `include_bytes!` (no build scripts).

## Agent tools (the owner's MCP servers)

Seven of the owner's std-only Rust tools are registered as user-scope MCP servers on this machine:
- Chisel: test impact, risk, and automatic issue logging.
- Stele: code index and symbol search.
- wikifier: dependency map and change journal.
- Trammel: plans and verification.
- Relatum: notes between agents.
- CoordinationHub: source-file leases, the task board and messages.
- Probatio: mutation testing.

Each server lists only the tool groups named for it below. Edit the block when the work changes; each server rereads it on the next request and tells Claude Code its tool list changed, so no restart is needed. Their state lives in gitignored folders (`.chisel/`, `.stele-context/`, `.wikifier_staging/`, `.trammel/`, `.swarmnotes/`, `.coordinationhub/`, `.probatio/`).

These tools coordinate **source files**. The engine's own `claim_*`, `task_*` and `note_*` tools coordinate the **live world**; keep using those for world work.

The live test of all seven on this repository, with timings, the bugs found (all fixed and re-verified the same day)
and a how-to, is in [docs/agent-tools/LIVE_TEST_2026-11-05.md](docs/agent-tools/LIVE_TEST_2026-11-05.md); its
"Re-verification" section has the current guidance.

```agent-tools
# Source-file coordination: CoordinationHub owns leases, tasks and messages; Relatum owns notes.
coordinationhub: tools core, locks, tasks, messages, agents
trammel: disable locks, notes, maintenance
chisel: disable locks
# Stele is used for search and symbols; agents here don't supply embeddings or KV state.
stele: disable locks, notes, messages, kv, embeddings
wikifier: disable map, status
```

## The gate

`cargo run -p specula-examen -- gate` is what a change passes before it is reported or merged.

**It is sized to what changed** (owner, 2026-11-16; `tools/examen/src/changed.rs` puts every changed path
in a tier, and the change's tier is the highest). The change is measured from the last commit the gate
passed at in this checkout, and from `origin/main` when there is none. **Commit, then gate:** a pass is
recorded only when nothing is uncommitted, so that while work is not pushed a note added to a document
does not run everything again; fix on top if it fails.

| What changed | What runs |
|---|---|
| Markdown documents, and pictures under `docs/`, that no test reads | the document guards and the tooling's own tests (seconds) |
| Tests and examples of a crate, the tooling, scripts, `evals/`, `perf/`, and documents or data under `docs/` that tests read | the workspace's tests once; clippy |
| The engine, a game, the workflows, or any path `changed.rs` does not know | everything: also the engine's library tests five more times at a high thread count, once more with every worker thread starting late (`SPECULA_TEST_WORKER_DELAY_MS`), the tests that hold a speed budget one at a time, and the debug and release builds |

- Its first lines say what was chosen and what was left out, and the summary keeps them: a sized pass is not
  a full one. **`--all` runs everything**: at the end of a milestone, and when the Rust pin moves.
- A path nobody listed is the engine's, so sizing only leaves a step out for a path known not to need it. A
  file is known by what it is, not by its folder: `docs/` also holds assets that tests read. What a test
  reads under `docs/`, or as a document, is listed in `changed.rs`, and a test there holds the list against
  every quoted path in the sources (a path built another way is not seen by it: list it by hand).
- **A game has no tier of its own**: the engine's library tests read Sepulcra's assets, so a change under
  `games/` gets what the engine gets. What decides the size (the workflows, `changed.rs`) is never sized by
  itself.
- It first says which Rust it runs with, and fails when that is not the version `rust-toolchain.toml` pins
  (rule 16).
- It then reports which GPU adapters the tests will see, and fails without one unless told `--no-gpu` (GPU
  tests skip themselves cleanly, so a machine whose driver stopped loading would otherwise pass having
  tested nothing).
- It reads whole logs and keeps them (`<target>/gate/`); a run in which no test ran, or one past its time
  limit, fails. A failed gate ends with what each failed test said, so a failure on CI, where the logs go
  with the machine, can be read.
- **CI** runs the document guards on every push; the builds and tests run when anything but documents
  changed since the last commit of `main` whose builds and tests all passed (not since the push before:
  documents pushed over a failed push still run everything). It keeps its build folder between runs (`examen kept`: saved by pushes to `main`, and cleared
  whenever a manifest, the lock file, a `clippy.toml`, the toolchain file or a workflow changed, since
  cargo does not tell those by a source's time), and installs the pinned Rust by number. A weekly job
  (`newest.yml`) tries the newest stable from nothing, and blocks nothing.

- **A gate must not pass by luck** (owner, 2026-11-12). A test that fails in some runs and passes in others is
  reported as FLAKY and fails the gate. A failure seen once is a bug until its cause is found: "it passes when run
  alone" is not an answer.
- **No result may depend on when a worker thread runs.** The engine thread has the result or does the work itself.
  The slow-worker run turns such a dependence into a failure every time.
- **Tests assert what the design guarantees**, not how fast this machine was. A count of passes or an elapsed time
  is a timing property; say why a pass ended instead (`capture::flight::PassEnd`), or what the code did (a counter
  that didn't move, an order, an output).
- **A speed is asserted as a budget:** `assert!(took < specula_core::speed_budget(Duration::from_millis(20)))`. The
  bound is tight when the test runs alone (the gate's `speed` step) and at least 5 s in every other run, where
  sixteen test threads share the machine. A bare bound under 5 s on a time fails
  `crates/specula/tests/clock_guard.rs` (its header lists the forms it reads, and the two it can't). GPU time is a
  time too: it is measured on a queue other processes use.

## Live checks (GPU + desktop; CI can't run these)

`cargo run -p specula-examen -- live` runs every live check, one after the other, each on its own ports, after
building the engine and the game in this checkout (debug and release). Run it at the end of a milestone and
after anything wide; `--only NAME,NAME` runs a few. A check that fails there is a finding: find its cause
(M5's audit found thirteen checks that had gone stale because they only ran when their own lane was
touched, and an engine defect only a check that had not been run for two milestones could show). A script
in `scripts/` that is not a check is named in the runner's `NOT_CHECKS` with the reason; a check that names
its ports another way than `--port` has its flags in `RUNS`. A check leaves the checkout as it found it:
the runner asks git after each one, and a file left changed or new fails that check. On Windows give it a
short `--out`: the checks build modules below it, and the runner refuses a folder too deep for the linker
before it starts (HANDOFF, "Gotchas that cost time").

**A new check is written in Rust** (rule 16), in `tools/examen/src/native.rs`: a function listed in `CHECKS`,
which the runner runs after the scripts, on ports of its own. It drives the engine as an agent with a shell
does (`specula new`, an engine on a project, `specula call TOOL '{json}'`, `specula stop`), keeps everything
it makes below its own output folder, and stops every engine it started by its own process id. The first is
`new_project_check` (a project made by `specula new` builds, plays, remembers and follows its game file);
`launcher_check` is the launcher's (`native/launch.rs`: it also meets the engine's own window, through
`specula_platform::probe`: found by its process, pictured as it rendered itself, sent keys as posted
messages). **A script that needs any change is ported there first, never edited** (owner, 2026-11-17:
"I prefer all to be Rust rather than having Python"); its port is run beside it, shown to fail on a
seeded fault, and then the script goes.

After changing `specula-gpu`, `specula-platform` or the engine's window loop, run
`cargo build -p specula && python scripts/window_check.py`. On every adapter it builds a scene over MCP, screenshots
the real window, resizes / minimizes / restores it, and requires zero validation messages and a clean shutdown.
Open the PNGs in `target/window-check/` and look at them before claiming the window path works.

## MCP

The running engine serves MCP at `http://127.0.0.1:7471/mcp` (see `.mcp.json`). Tool names use
`^[a-zA-Z0-9_-]{1,64}$` (`world_spawn`, `sight_frame`).

From a shell, with no MCP client ([docs/design/LAUNCH.md](docs/design/LAUNCH.md)): `specula list` finds the
engines running on this machine, `specula tools [FILTER]` and `specula call TOOL '{json}'` (or `TOOL @args.json`) use one, and
`specula projects` / `open --last` / `open NAME` / `new NAME` / `stop` manage projects and engines, and
`specula run FOLDER --ticks N` plays a project's game with no window and prints how it stands, and
`specula check FOLDER` plays every run its `checks/runs.json` lists and says which did not hold. The
executable with no argument opens the start screen (the project list). The list of a person's projects is
never an MCP tool, and scripts' engines (`--settings off`) are never listed.
