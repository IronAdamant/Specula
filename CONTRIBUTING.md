# Contributing to Specula

Thanks for helping agents drive Bevy at runtime.

## Principles

1. **Prefer composition over reinvention.** Extend or integrate [`bevy_brp_mcp`](https://github.com/natepiano/bevy_brp) / [`bevy_brp_extras`](https://crates.io/crates/bevy_brp_extras) rather than duplicating BRP or MCP surfaces.
2. **Keep v0 focused.** High-quality readiness checks, a reliable MCP path, capture, templates, and docs beat feature sprawl.
3. **Test the shipped path.** Unit tests should call real modules (inject I/O at boundaries). Avoid mocks that reimplement production logic.

## Workspace layout

| Path | Role |
|------|------|
| `crates/specula-env` | OS / Rust / Bevy readiness detection |
| `crates/specula-brp` | BRP HTTP client + image capture helpers |
| `crates/specula` | CLI + MCP server |
| `templates/sample-app` | Feature-gated RemotePlugin / capture sample |

## Development setup

```bash
# Toolchain
rustup default stable

# Library + CLI tests (fast)
cargo test -p specula-env -p specula-brp -p specula

# Doctor
cargo run -p specula -- doctor

# Sample with BRP (needs GPU/window for full scene + screenshots)
cargo run -p specula_sample --features remote,capture
```

## Extending

- **New OS detection signals** → `specula-env` (`CommandRunner` stays injectable).
- **New BRP helpers** → `specula-brp` client methods; wire MCP tools in `crates/specula/src/mcp.rs`.
- **Richer agent tools** → prefer upstream `bevy_brp_mcp` PRs; wrap here only for Grok-specific UX (env, image return shaping, docs).

## Pull requests

1. Run `cargo test -p specula-env -p specula-brp -p specula` and `cargo build -p specula`.
2. Update `CHANGELOG.md` under `[Unreleased]` or the next version section.
3. Keep docs (README compatibility matrix) in sync when bumping Bevy / BRP crate versions.
4. Dual-license all new source as MIT OR Apache-2.0.

## Code style

- Rust 2021, `rustfmt` defaults.
- Prefer small public APIs with `serde` types for CLI/MCP boundaries.
- Logging on MCP servers must use **stderr** only (stdout is the protocol).

## License

By contributing, you agree that your contributions are dual-licensed under MIT OR Apache-2.0, the same as Bevy.
