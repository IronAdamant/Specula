# Fast start — Grok Build controlling a live Bevy app

Goal: in a few minutes, a Grok Build session can query, mutate, and see a Bevy scene.

## Prerequisites

- Rust stable (`rustc`, `cargo`) via [rustup](https://rustup.rs)
- Grok Build CLI configured
- (Recommended) GPU + windowing stack for the full 3D sample and screenshots

## Steps

### 1. Clone and verify the host

```bash
cd Specula
cargo run -p specula -- doctor
```

Fix any `NOT READY` items using the printed OS-specific guidance.

### 2. Install CLI + optional full BRP MCP

```bash
cargo install --path crates/specula
cargo install bevy_brp_mcp --locked   # full tool surface
```

### 3. Register MCP servers

```bash
grok mcp add specula -- specula mcp
grok mcp add bevy-brp -- bevy_brp_mcp
```

Or paste the TOML from `specula mcp-config` into `~/.grok/config.toml`.

Templates are **embedded** in the CLI (G6); monorepo/`SPECULA_TEMPLATE_ROOT` are optional overrides. Launch is non-blocking (`wait_secs=0`); always call MCP `bevy_wait_brp` (or CLI `brp wait`) after start. Cold first compile: shell `cargo run --features remote,capture`. Scaffold into a **subdir**, never `--path .`.

### 4. Start the sample (terminal A)

```bash
cargo run -p specula_sample --features remote,capture
```

Wait until the window appears (first compile can take several minutes).

### 5. Drive it (terminal B or Grok session)

```bash
specula brp wait --port 15702
specula brp query --port 15702 \
  --component bevy_transform::components::transform::Transform
specula brp screenshot --path captures/scene.png --with-image-meta
```

In Grok Build, ask the agent to:

1. Call `bevy_env_check`  
2. Call `bevy_brp_query` on port 15702  
3. Call `bevy_capture_viewport` and describe the image  

### 6. Headless BRP-only path (no GPU)

Useful for CI-like control loops:

```bash
cargo run -p specula_sample --bin brp_headless --features remote
# other terminal:
specula brp wait --port 15702
specula brp query --port 15702 \
  --component specula_sample::AgentCube
```

Note: type paths for binary-local types depend on the package name; prefer reflected `Transform` / resources listed via `rpc.discover` and `world.list_components` when unsure.

## Next

- Scaffold: `specula scaffold --path ./my-game`  
- Read [TROUBLESHOOTING.md](TROUBLESHOOTING.md)  
- Prefer `bevy_brp_mcp` for launch discovery, watches, input injection, and hierarchy tools  
