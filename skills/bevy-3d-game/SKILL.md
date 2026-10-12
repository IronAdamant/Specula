---
name: bevy-3d-game
description: >
  Build production 3D Bevy games (camera, lighting, meshes/glTF path, movement)
  on Bevy 0.19. Use for 3D, glTF, mesh, third-person, FPS, or /bevy-3d-game.
  Always also follow bevy-production; for 2D art overlays use game-asset-core.
metadata:
  short-description: "Production 3D Bevy vertical slice"
---

# Bevy 3D games

Requires **`bevy-production`** layout and states. This skill covers the **3D kit**.  
For a **complete short demo**, also load **`bevy-demo-game`** and meet **`docs/GAME_DOD.md`**.
## Unprompted defaults

| Need | Default |
|------|---------|
| Camera | Perspective `Camera3d`; sensible start transform looking at play area |
| Light | At least one key light (shadows optional for slice); ambient if scene is too dark |
| Player | Capsule/cube or glTF; `Name::new("Player")`; ground plane |
| Input | WASD on XZ plane (or genre-appropriate); document controls |
| Assets | Prefer path to `assets/models/` for glTF even if first slice uses primitives |
| Materials | Simple `StandardMaterial` colors until art lands |
| Names | Player, Ground, MainCamera, KeyLight |

## Vertical slice (minimum playable)

1. **Loading** — ready scene assets (or skip delay if only primitives + one texture).  
2. **MainMenu** → **Playing**.  
3. **Playing** — move player on plane; Esc → pause/menu.  
4. Ground + player + camera + light always present in Playing.  
5. When art exists: load at least one texture or glTF from disk.

## Camera and control patterns

Pick one and document it:

- **Top-down-ish 3D** — camera above, look at origin; move on XZ.  
- **Third-person** — camera behind player (can start fixed offset).  
- **FPS** — later; needs pointer lock — do not start here unless requested.

Avoid a floating camera with no reference ground (capture looks empty/black).

## Lighting for capture

Agents screenshot the window. Ensure:

- Exposure/light intensity high enough that the subject is visible  
- Player not the same color as the ground  
- Window not minimized (see troubleshooting)

## Art pipeline

- UI/HUD still use **`game-asset-core`** + **`game-ui-icons`**.  
- Character concept sheets can use **`game-character-consistency`** then a 3D authoring path outside Imagine if needed.  
- glTF: place under `assets/models/…` and load with `SceneRoot` / scene spawner patterns for Bevy 0.19.  
- Textures: `assets/models/` or `assets/sprites/` as appropriate.

## Physics

Start **without** a physics crate (transform XZ movement + distance checks + optional fall-off). Enough for short GAME_DOD slices.

When gravity, solid ground colliders, or rigid bodies are needed:

| Pin | Version | Feature |
|-----|---------|---------|
| `avian3d` | **0.7** (Bevy **0.19**) | `physics` on scaffolded kits |

```bash
cargo run --features remote,capture,physics
```

```toml
avian3d = { version = "0.7", optional = true }
physics = ["dep:avian3d"]
```

- Ground plane collider + dynamic capsule/cuboid player is a good first pattern.  
- Strategy / factory camera games usually should **not** enable full physics by default.  
- Own Transform either via physics or manual movement; avoid double writes (**B0001**).  
- Full notes: repo `docs/PHYSICS.md`.

## Anti-patterns

- Static cube with no input (that is the **demo fixture**, not a game)  
- Zero lights + dark clear color (black captures)  
- Unbounded fall (no ground)  
- Giant monorepo of unorganized systems in `main.rs`  

## References

- `references/3d-slice.md` — light + camera + movement sketch  

