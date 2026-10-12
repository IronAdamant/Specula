# Shipping a Bevy game (Specula)

Engineering-oriented release checklist for games scaffolded or structured with Specula. This is **not** store certification.

## Build

```bash
# From the game project root
cargo build --release
```

Binary location (Unix):

```text
target/release/<package_name>
```

Windows: `target/release/<package_name>.exe`.

Production templates set:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
```

## Assets next to the binary

Specula **production templates** (2d/3d) set `AssetPlugin` as:

- **Debug:** crate-root `assets/` via `CARGO_MANIFEST_DIR` (agent-friendly when running `target/debug/*`)
- **Release:** relative `"assets"` — ship the folder **beside the executable**
- **Override:** env `BEVY_ASSET_ROOT`

When distributing a release:

1. Ship the `assets/` folder beside the executable (matches release `AssetPlugin`), **or**
2. Set `BEVY_ASSET_ROOT` for custom layouts, **or**
3. Document that the game must be launched from a directory that contains `assets/`.

Never assume `cargo run` paths when double-clicking a binary from another directory.

## Pre-ship checklist

- [ ] Menu → play → pause (or quit) works  
- [ ] `cargo run --features remote,capture` works for agent iteration during development  
- [ ] `cargo build --release` succeeds  
- [ ] Disk assets under `assets/` load (no pink/missing textures in a release run)  
- [ ] Window title and package name match the product  
- [ ] README documents controls and feature flags  
- [ ] Optional: live capture review via Specula MCP (`bevy_capture_viewport`)  

## Platforms

| Platform | Notes |
|----------|--------|
| macOS | May need app bundle / notarization for distribution outside cargo; GPU drivers via system |
| Windows | MSVC toolchain for Bevy; ship runtime DLLs only if you link non-static deps |
| Linux | Install distro graphics/audio packages; see `specula doctor` guidance |

## Agent skills

Before calling a game “shippable”, load **`bevy-production`** ship checklist and verify with **`bevy-agent-loop`** when MCP is available.

## Related

- [ASSET_CONVENTIONS.md](ASSET_CONVENTIONS.md)
- [PRODUCTION_GAMES.md](PRODUCTION_GAMES.md)
