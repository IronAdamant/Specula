# Troubleshooting

## Environment / `specula doctor`

### `rustc` or `cargo` missing

Install via [rustup](https://rustup.rs):

| OS | Notes |
|----|--------|
| **Windows** | rustup + Visual Studio Build Tools (“Desktop development with C++”) |
| **Linux** | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` then distro `build-essential` / `gcc` |
| **macOS** | rustup + `xcode-select --install` |

Open a **new** shell after installing so `PATH` includes `~/.cargo/bin`.

### Compile probe fails

`doctor --compile-probe` creates a temporary Bevy dependency and runs `cargo build`. Failures are often:

- Missing C linker / MSVC tools  
- Linux: missing X11/Wayland/ALSA/udev headers (see doctor’s install guidance)  
- Network blocked from crates.io  

You can still use Specula without the probe if `rustc` and `cargo` report ready.

### Bevy is not a global binary

Specula correctly does **not** search for a `bevy` executable. Bevy is pulled as a Cargo crate per project.

## BRP connection

### Connection refused on port 15702

1. Run the app with BRP enabled:
   ```bash
   cargo run -p specula_sample --features remote,capture
   ```
2. Confirm nothing else is bound: change port with `BrpExtrasPlugin::with_port(…)` or `BRP_EXTRAS_PORT`.
3. Wait for first compile + window startup:
   ```bash
   specula brp wait --port 15702 --timeout-secs 180
   ```

### Query returns empty / type not found

**Short aliases** (MCP `bevy_brp_query` / `bevy_brp_mutate` expand these):

| Alias | Fully-qualified Reflect path (Bevy 0.19) |
|-------|------------------------------------------|
| `Name` | `bevy_ecs::name::Name` |
| `Transform` | `bevy_transform::components::transform::Transform` |
| `GlobalTransform` | `bevy_transform::components::global_transform::GlobalTransform` |

Example MCP query components: `["Name", "Transform"]` (default when omitted).

Or pass FQNs directly (any string containing `::` is left unchanged):

```text
bevy_transform::components::transform::Transform
```

Discover methods with `specula brp discover` or MCP `bevy_brp_discover`. For schema help, prefer `bevy_brp_mcp`’s type guide tools.

### Optional movement smoke (`send_keys`)

After BRP is ready:

1. `bevy_brp_query` with `Name` + `Transform` — note Player translation.  
2. Prefer live `rpc.discover` / `bevy_brp_discover` for `brp_extras/send_keys` param schema (extras versions differ).  
3. `bevy_brp_call` with `method: "brp_extras/send_keys"` and discover-backed params (e.g. press/hold movement keys briefly).  
4. Re-query Transform — Player should move if input is wired.  
5. **Fallback:** `bevy_brp_mutate` on Player `Transform` `translation` if send_keys is missing or flaky.  

Rich keyboard/mouse injection: install optional `bevy_brp_mcp`.

### Mutate fails

- Entity IDs are Bevy `u64` canonical IDs from query results.  
- `path` uses Bevy `GetPath` syntax (e.g. `translation.x`).  
- Component must be `Reflect` and registered.

## Common BRP method names

Use **exact** method strings (do not invent prefixes like `bevy_brp_extras/…`). Discover live with `bevy_brp_discover` / `specula brp discover`.

| Method | Purpose |
|--------|---------|
| `rpc.discover` | List registered methods |
| `world.query` | Read entities/components |
| `world.mutate_components` | Write a component field path |
| `brp_extras/screenshot` | Write a PNG to a filesystem path |
| `brp_extras/get_diagnostics` | Diagnostics (when extras enabled) |
| `brp_extras/send_keys` | Keyboard injection (if registered; prefer `bevy_brp_mcp` for rich input) |

## Screenshots / visual capture

### `brp_extras/screenshot` method not found

The running app needs `bevy_brp_extras` (`capture` / `remote` features on the sample). Plain `RemotePlugin` alone does not register screenshot.

Also enable Bevy’s `png` feature.

### MCP image truncated in chat UI

`bevy_capture_viewport` returns image **and** text with `bytes=…` and **`abs_path=…`**. If the client drops large base64 images, open the absolute path on disk.

### Black or empty PNG

Known limitation: minimized, hidden, or fully occluded primary windows may produce black frames on some platforms. Success means the PNG is fully written — not that pixels are nonuniform.

Mitigations:

- Keep the window visible and focused  
- Capture a specific camera entity when supported  
- Use headless/offscreen targets in advanced apps  

### MCP image not showing in Grok Build

`bevy_capture_viewport` returns MCP `image` content (base64 PNG). If the client truncates large payloads, lower resolution or use the path returned in the text block and `read` the file.

## MCP server

### Grok Build cannot start `specula`

- Use an absolute `command` path in `config.toml`  
- Ensure the binary is built: `cargo build -p specula --release`  
- Logs belong on stderr; do not redirect stdout  

### Want the full Bevy agent tool list

```bash
cargo install bevy_brp_mcp --locked
grok mcp add bevy-brp -- bevy_brp_mcp
```

Or: `specula mcp --delegate-brp-mcp` (requires `bevy_brp_mcp` on `PATH`).

## Launch / agent loop

### MCP `bevy_launch_app` used to hang ~120s

`bevy_launch_app` is **non-blocking by default** (`wait_secs=0`). It returns `status=spawned` + log path immediately. When `target/debug/<package>` exists it prefers that **warm binary**; otherwise it runs cold `cargo run` and labels `mode=cold_cargo_run`.

1. Call **`bevy_wait_brp`** with `timeout_secs` **180** (cold) or **30** (warm).  
2. Then `bevy_brp_discover` / query / `bevy_capture_viewport`.  
3. For **cold first compiles**, prefer shell/background `cargo run --features remote,capture` so the host MCP `tool_timeout_sec` cannot kill a long compile.  
4. Optional: `wait_secs` on launch is capped at **60** (warm restarts only). Do **not** use long wait on launch for cold builds.

CLI equivalent: `specula brp wait --port 15702 --timeout-secs 180`.

### Two apps / dual launch on port 15702

Only one process can listen on the default BRP port. Dual launch for dogfood means sequential starts (or different ports). Use `bevy_register_target` for multi-instance with distinct ports.

### Scaffold into `.` or monorepo root fails

Scaffold **refuses** `--path .` (even with `--force`) so it never wipes the current directory. Use a subdirectory:

```bash
specula scaffold --kind 2d --path ./my-game --name my-game
```

### Stuck on Loading / empty Name queries

Usually a missing asset root (binary run from `target/debug` without template `AssetPlugin`).

- Templates pin debug assets to `CARGO_MANIFEST_DIR/assets`.  
- After ~12s they fail-forward to MainMenu with an error log.  
- Confirm `assets/` exists and paths match `AssetServer::load("sprites/…")` etc.  
- Override with `BEVY_ASSET_ROOT` if needed.

### Bevy B0001 query conflict panic

Two systems with overlapping `Query<&mut T>` panic at runtime. Fix with marker components, `Without`, `ParamSet`, or split systems. Scaffolded `AGENTS.md` documents this.

### Scaffold: templates not found

**G6:** `specula` embeds `templates/` in the binary. Scaffold resolves:

1. `SPECULA_TEMPLATE_ROOT` (optional override)  
2. Monorepo `templates/` (dev / `cargo install --path`)  
3. Embedded extract to `~/.cache/specula/templates/<version>/`  

You do **not** need a monorepo checkout for scaffold after a normal `cargo install --path` (or any install that compiled with templates present). Optional override:

```toml
[mcp_servers.specula.env]
SPECULA_TEMPLATE_ROOT = "/path/to/custom/templates"
```

`specula mcp-config` prints how templates were resolved.

## Slow compiles

Bevy debug builds are heavy. The sample uses elevated `opt-level` for dependencies. For day-to-day work consider:

- `cargo run --release` when iterating on BRP only  
- Dynamic linking setups from Bevy docs (advanced)  
- Shell `cargo run` for the first compile; MCP launch after `target/` is warm  

## Version skew

Keep Bevy, `bevy_brp_extras`, and `bevy_brp_mcp` on the same track (see README matrix). Mixing 0.18 apps with 0.22 MCP crates will fail in subtle ways.

Optional physics: **avian2d / avian3d 0.7** for Bevy **0.19** — see [PHYSICS.md](PHYSICS.md).
