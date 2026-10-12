# Asset path conventions

Bevy loads files **relative to the project `assets/` directory**. Specula production templates and skills use these roots:

| Directory | Purpose | Example `AssetServer` path |
|-----------|---------|----------------------------|
| `assets/sprites/` | 2D characters, props, tiles, VFX sheets | `"sprites/player.png"` |
| `assets/models/` | 3D meshes, glTF, material textures | `"models/player.glb"`, `"models/ground_tint.png"` |
| `assets/ui/` | HUD chrome, buttons, panels, icons | `"ui/panel.png"` |
| `assets/audio/` | SFX and music | `"audio/jump.ogg"` |

## Rules for agents

1. **Never invent absolute paths** for runtime loads — always paths under `assets/` for `AssetServer::load` (e.g. `"sprites/player.png"`).
2. **Put art from `game-asset-*` skills** into the matching root; keep filenames stable when re-exporting.
3. **Prefer engine-ready assets**: keyable backgrounds for sprites (see `game-asset-core`); no baked UI text when the game localizes.
4. **Name entities** after important visual assets (`Name::new("Player")`) so BRP queries match captures.
5. **Loading state**: wait until handles are loaded (`AssetServer::is_loaded`) before transitioning MainMenu → Playing when art is required. Templates fail-forward to MainMenu after ~12s if an asset never loads.

## Where Bevy finds `assets/` (critical)

Default Bevy resolution is **cwd-relative** `assets/`. Running `./target/debug/<bin>` from the wrong directory used to stick games on Loading forever.

Production templates set `AssetPlugin`:

| Build | Root |
|-------|------|
| **Debug** | `{CARGO_MANIFEST_DIR}/assets` (works from any cwd) |
| **Release** | relative `"assets"` (ship folder next to the binary) |
| **Override** | env `BEVY_ASSET_ROOT` |

Do not remove this when extending the scaffold unless you replace it with an equivalent packaging strategy.

## Scaffolded layout

`specula scaffold --kind 2d|3d` creates these directories (and ships at least one placeholder disk asset for the vertical slice).

## Related

- [SHIPPING.md](SHIPPING.md) — release builds and packaging
- [PRODUCTION_GAMES.md](PRODUCTION_GAMES.md) — overall production loop
- [PHYSICS.md](PHYSICS.md) — optional Avian pins (not required for assets)
