---
name: bevy-2d-game
description: >
  Build production 2D Bevy games (orthographic camera, sprites/atlases, tilemaps,
  movement, HUD) on Bevy 0.19. Use for 2D, platformer, top-down, sprite, tilemap,
  or /bevy-2d-game. Always also follow bevy-production; art via game-asset-core.
metadata:
  short-description: "Production 2D Bevy vertical slice"
---

# Bevy 2D games

Requires **`bevy-production`** layout and states. This skill covers the **2D kit**.  
For a **complete short demo**, also load **`bevy-demo-game`** and meet **`docs/GAME_DOD.md`** (objective + win/lose + challenge—not movement alone).
## Unprompted defaults

| Need | Default |
|------|---------|
| Camera | Orthographic 2D camera; clear color set intentionally |
| Player | Sprite (texture from `assets/sprites/` when available) + movement in `Playing` |
| Input | WASD + arrow keys; document controls in README |
| Bounds | Keep player on-screen or on a simple ground/collider box |
| UI | Menu and optional HUD; **no baked text in art assets** (localize in engine) |
| Pixels | Prefer integer/scale-friendly settings if pixel art; otherwise HD sprites with consistent PPU |
| Names | `Name::new("Player")`, `MainCamera`, etc. |

## Vertical slice (minimum playable)

1. **Loading** — load sprite handles (or immediate if single small PNG).  
2. **MainMenu** — press Enter/Space/click → **Playing**.  
3. **Playing** — move player; Esc → **Paused** or menu.  
4. At least one **disk texture** under `assets/sprites/` once art exists (colored quad only as temporary stand-in).

## Systems

- `player_movement`: read keyboard; write `Transform` (or velocity if you add physics later).  
- Keep Y-up Bevy convention; choose top-down vs side-view **once** and stick to it for art (side-view = side sprites from `game-character-consistency`).  
- Z for draw order: background < entities < UI.

## Art pipeline

1. `game-asset-core` + (`game-character-consistency` | `game-animation-frames` | `game-tilesets` | `game-ui-icons`).  
2. Save under `assets/sprites/…` or `assets/ui/…`.  
3. Load: `asset_server.load("sprites/player.png")` (paths relative to `assets/`).  
4. Verify with **`bevy-agent-loop`** capture after run.

## Tilemaps

Prefer a maintained Bevy 0.19-compatible tilemap crate when the design needs a grid; otherwise start with a few static sprites. Seamless tiles: follow **`game-tilesets`** (2×2 composite check).

## Physics

Start **without** a physics crate (transform movement + simple AABB). That is enough for most GAME_DOD top-down demos.

When jumps/solid colliders/rigid bodies need it:

| Pin | Version | Feature |
|-----|---------|---------|
| `avian2d` | **0.7** (Bevy **0.19**) | `physics` on scaffolded kits |

```bash
cargo run --features remote,capture,physics
```

```toml
avian2d = { version = "0.7", optional = true }
physics = ["dep:avian2d"]
```

- Prefer Avian for Bevy 0.19; re-check the [Avian version table](https://github.com/avianphysics/avian) on Bevy bumps.  
- Own movement via physics **or** manual Transform — not both.  
- Keep `Name` on bodies for BRP; watch **B0001** if multiple systems mutate transforms.  
- Skip physics for strategy/factory placement games unless you truly need a solver.  
- Full notes: repo `docs/PHYSICS.md`.

## Anti-patterns

- Perspective/3D camera for a pure 2D game  
- Mixing pixel-art and HD without a scale plan  
- Huge textures without atlases when many frames exist  
- Gameplay in `Startup` only  

## References

- `references/2d-slice.md` — camera + sprite movement sketch  
