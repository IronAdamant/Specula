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
   DLL are silently dropped (crash at call time). `kernel32` → `specula-sys`; `user32` and `winmm` → `specula-platform`;
   `ole32` → `specula-audio`. Runtime-loaded through `dynlib` (any crate may load them): `vulkan-1`,
   `xinput1_4`/`xinput9_1_0` (specula-platform), `avrt` (specula-audio).
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
11. **No session or share links in commit messages** (claude.ai sessions and the like): the repo is public. The CI job
    `commit-messages` rejects them in new commits; don't rewrite older history.

## Layout

| Crate | Role |
|---|---|
| `specula-sys` | the only `kernel32` imports; runtime library loader (`dynlib`); Ctrl+C / console-close handling (`interrupt`) |
| `specula-core` | JSON, math, base64, checksums, DEFLATE/zlib, PNG |
| `specula-platform` | native windows + input (Win32 first) |
| `specula-gpu` | Vulkan (runtime-loaded) + render-hardware interface |
| `specula-protocol` | HTTP/1.1 + MCP server for agents |
| `specula` (bin) | world server: world, rendering, agent tools |
| `specula-input` | actions, not keys: keyboard, mouse, gamepad (XInput), touch |
| `specula-audio` | output (WASAPI), mixing, synthesis, analysis |
| `specula-text` | TrueType fonts → glyph atlas → laid-out quads (CPU) |
| `specula-ui` | HUD and menu toolkit for agents and humans |
| `specula-anim` | skeletal animation (stages A1–A2) |
| `specula-model` | procedural modelling: recipes → meshes, LODs, colliders |
| `specula-gltf` | glTF 2.0 / GLB import and export |
| `games/sepulcra`, `games/sepulcra-app` | the first game: headless rules, and the app hosted by the engine |

## Tooling on this machine

- Toolchain `stable-x86_64-pc-windows-gnu`. Vulkan SDK: `C:\VulkanSDK\1.4.363.0` (`Bin\glslc.exe`,
  `Bin\spirv-val.exe`; validation layer registered system-wide). `VULKAN_SDK` may not be in the shell environment —
  use the full path.
- Shaders: GLSL sources in `crates/specula-gpu/shaders/`, compiled to `.spv` with `glslc` **by hand** and committed;
  the crate embeds the `.spv` with `include_bytes!` (no build scripts).

## Live checks (GPU + desktop; CI can't run these)

After changing `specula-gpu`, `specula-platform` or the engine's window loop, run
`cargo build -p specula && python scripts/window_check.py`. On every adapter it builds a scene over MCP, screenshots
the real window, resizes / minimizes / restores it, and requires zero validation messages and a clean shutdown.
Open the PNGs in `target/window-check/` and look at them before claiming the window path works.

## MCP

The running engine serves MCP at `http://127.0.0.1:7471/mcp` (see `.mcp.json`). Tool names use
`^[a-zA-Z0-9_-]{1,64}$` (`world_spawn`, `sight_frame`).
