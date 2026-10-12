# Specula — agent rules

Specula is a **zero-dependency, agent-native game engine and editor in Rust**. Plan: [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md).

## Hard rules

1. **Zero third-party crates.** Only Rust `std` and workspace path dependencies. `Cargo.lock` must list workspace
   packages only (`crates/specula/tests/zero_dep.rs` enforces it). Not even dev-dependencies.
2. **OS/driver imports via raw-dylib.** Declare foreign functions with
   `#[link(name = "user32", kind = "raw-dylib")] extern "system" { … }` — never rely on import libraries.
   Drivers that may be missing (e.g. `vulkan-1`) are loaded **at runtime** (`LoadLibraryW`/`GetProcAddress`).
3. **Linker = the Rust toolchain's own** (`.cargo/config.toml`: `-C link-self-contained=yes` on windows-gnu).
   Do not set other linkers or add build scripts that call external tools.
4. **Every crate compiles on non-Windows.** Windows-only code lives behind `#[cfg(windows)]`; other platforms return
   a clear "unsupported on this platform" error at runtime.
5. **Structured errors.** Error types carry machine-readable fields (codes, offsets, VkResult) — agents debug from data.
6. **Conventions** (see `specula-core::math` docs): right-handed, +Y up, metres; column-major matrices;
   Vulkan clip (y down, depth 0..1); screen pixels origin top-left; colours given to APIs are sRGB.
7. **Tests.** Pure code gets unit tests. Tests needing a GPU or a window must **skip cleanly** (print why, pass) when
   Vulkan / a desktop session is unavailable, so CI without a GPU stays green.
8. **`unsafe`** only at FFI boundaries, each block with a `// SAFETY:` comment.
9. **No warnings**: CI builds with `-Dwarnings`.

## Layout

| Crate | Role |
|---|---|
| `specula-core` | JSON, math, base64, checksums, DEFLATE/zlib, PNG |
| `specula-platform` | native windows + input (Win32 first) |
| `specula-gpu` | Vulkan (runtime-loaded) + render-hardware interface |
| `specula-protocol` | HTTP/1.1 + MCP server for agents |
| `specula` (bin) | world server: world, rendering, agent tools |

## Tooling on this machine

- Toolchain `stable-x86_64-pc-windows-gnu`. Vulkan SDK: `C:\VulkanSDK\1.4.363.0` (`Bin\glslc.exe`,
  `Bin\spirv-val.exe`; validation layer registered system-wide). `VULKAN_SDK` may not be in the shell environment —
  use the full path.
- Shaders: GLSL sources in `crates/specula-gpu/shaders/`, compiled to `.spv` with `glslc` **by hand** and committed;
  the crate embeds the `.spv` with `include_bytes!` (no build scripts).

## MCP

The running engine serves MCP at `http://127.0.0.1:7471/mcp` (see `.mcp.json`). Tool names use
`^[a-zA-Z0-9_-]{1,64}$` (`world_spawn`, `sight_frame`).
