# Specula — agent rules

Specula is a **zero-dependency, agent-native game engine and editor in Rust**. Plan: [docs/ROADMAP.md](docs/ROADMAP.md)
(checkboxes + reasoning) and [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md) (decisions, architecture). New to the project?
Read [HANDOFF.md](HANDOFF.md) first.

## Hard rules

1. **Zero third-party crates.** Only Rust `std` and workspace path dependencies. `Cargo.lock` must list workspace
   packages only (`crates/specula/tests/zero_dep.rs` enforces it). Not even dev-dependencies.
2. **OS/driver imports via raw-dylib.** Declare foreign functions with
   `#[link(name = "user32", kind = "raw-dylib")] extern "system" { … }` — never rely on import libraries.
   Drivers that may be missing (e.g. `vulkan-1`) are loaded **at runtime** via `specula_sys::dynlib::Library`.
   **Each DLL is declared in exactly one crate** — on windows-gnu a second crate's raw-dylib imports from the same
   DLL are silently dropped (crash at call time). `kernel32` → `specula-sys`; `user32`, `winmm` and `imm32` →
   `specula-platform`; `ole32` → `specula-audio` (`crates/specula/tests/dll_owners.rs` checks this list against the
   code). Runtime-loaded through `dynlib` (any crate may load them): `vulkan-1`,
   `xinput1_4`/`xinput9_1_0` (specula-platform), `avrt` (specula-audio),
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

## Layout

| Crate | Role |
|---|---|
| `specula-sys` | the only `kernel32` imports; runtime library loader (`dynlib`); Ctrl+C / console-close handling (`interrupt`) |
| `specula-core` | JSON, math, base64, checksums, DEFLATE/zlib, PNG; structured tracing (`trace`) |
| `specula-platform` | native windows + input (Win32 first) |
| `specula-gpu` | Vulkan (runtime-loaded) + render-hardware interface |
| `specula-protocol` | HTTP/1.1 + MCP server for agents |
| `specula` (bin) | world server: world, rendering, agent tools |
| `specula-input` | actions, not keys: keyboard, mouse, gamepad (XInput), touch |
| `specula-audio` | output (WASAPI), mixing, synthesis, analysis |
| `specula-text` | TrueType fonts → glyph atlas → laid-out quads (CPU) |
| `specula-ui` | HUD and menu toolkit for agents and humans |
| `specula-anim` | skeletal animation (A1–A3): clips, graphs, IK, motion generators, motion checks |
| `specula-lex` | Lex: gameplay rules as typed text data (validate, compile, simulate deterministically) |
| `specula-balance` | balance lab: targets as data, deterministic sweeps and tuning over Lex parameters |
| `specula-physics` | collision queries, triggers, character controller, rigid bodies (stages P1–P3) |
| `specula-via` | navigation: tiled navmeshes, paths, off-mesh links, crowds |
| `specula-mens` | AI as data: behaviour trees and utility AI, validated, with a deterministic runner |
| `specula-locus` | placement by relations (`world_place`'s solver) |
| `specula-model` | procedural modelling: recipes → meshes, LODs, colliders; CSG, SDF, terrain, scatter, texture graphs |
| `specula-studio` | the asset studio's measures: `style.toml` rules, asset facts, taste checks, critiques |
| `specula-scintilla` | particles/VFX and decals as data: effects, a closed-form evaluator, budgets, photosensitivity checks |
| `specula-gustus` | taste direction: gallery spaces and spreads, pick-driven tuning, the preference store, board measures, style presets |
| `specula-lux` | baked GI: a BVH ray caster and deterministic irradiance probe volumes (LUX1 files) |
| `specula-gltf` | glTF 2.0 / GLB import and export |
| `specula-sdk` | the one crate game-logic modules depend on: deterministic `Cx` API, versioned C ABI |
| `specula-logic` | module host internals: library loading, the build service, diagnostics as data |
| `games/sepulcra`, `games/sepulcra-app` | the first game: headless rules, and the app hosted by the engine |
| `games/ambulo` | example module: a walking, jumping character (the module host's proof) |

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

## Live checks (GPU + desktop; CI can't run these)

After changing `specula-gpu`, `specula-platform` or the engine's window loop, run
`cargo build -p specula && python scripts/window_check.py`. On every adapter it builds a scene over MCP, screenshots
the real window, resizes / minimizes / restores it, and requires zero validation messages and a clean shutdown.
Open the PNGs in `target/window-check/` and look at them before claiming the window path works.

## MCP

The running engine serves MCP at `http://127.0.0.1:7471/mcp` (see `.mcp.json`). Tool names use
`^[a-zA-Z0-9_-]{1,64}$` (`world_spawn`, `sight_frame`).
