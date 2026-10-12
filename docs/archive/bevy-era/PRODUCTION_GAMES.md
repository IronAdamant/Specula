# Production games with Specula

Specula is an **agent-native Bevy game companion**: skills teach how to build, scaffold defines where code and assets live, and MCP verifies what is running.

> **Not the production path:** `templates/sample-app` is a BRP **integration fixture** (static 3D cube for launch / query / capture smoke tests). Prefer production skills and (when available) `scaffold --kind 2d|3d`.

## Product loop

```
Skills (HOW)  →  Scaffold / layout (WHERE)  →  MCP capture (WHAT)
```

1. Load **`bevy-production`** plus **`bevy-2d-game`** or **`bevy-3d-game`**.
2. Use a production layout (plugins, states, `assets/`) — not a single-file demo.
3. Implement a vertical slice: **Loading → MainMenu → Playing** (+ **Paused**).
4. Run with `--features remote,capture`.
5. Follow **`bevy-agent-loop`**: launch (non-blocking) → **`bevy_wait_brp`** → query/mutate → **capture viewport** → fix → repeat. Cold first compile: prefer shell `cargo run`.
6. For art, load Grok **`game-asset-core`** (+ specialist) and drop files under `assets/`.
7. Physics is optional: pins in [PHYSICS.md](PHYSICS.md) (`avian2d` / `avian3d` **0.7**). Default kits stay transform-based.

## Skill map (this repo)

| Skill | When to load |
|-------|----------------|
| [`bevy-production`](../skills/bevy-production/SKILL.md) | Any real Bevy game: architecture, assets, ship checklist |
| [`bevy-2d-game`](../skills/bevy-2d-game/SKILL.md) | Sprites, tilemaps, orthographic camera, 2D input |
| [`bevy-3d-game`](../skills/bevy-3d-game/SKILL.md) | glTF, lighting, 3D camera, meshes |
| [`bevy-agent-loop`](../skills/bevy-agent-loop/SKILL.md) | Live BRP / MCP / screenshot iteration |

### External art skills (Grok bundled)

| Skill | Use for |
|-------|---------|
| `game-asset-core` | Always, before any game art |
| `game-animation-frames` | Walk cycles, attacks, FX |
| `game-character-consistency` | Same hero across views/variants |
| `game-tilesets` | Seamless tiles / transitions |
| `game-ui-icons` | Buttons, HUD, icon sets |

## 2D vs 3D

| Choose 2D when… | Choose 3D when… |
|-----------------|-----------------|
| Sprite / tile / UI-heavy | Meshes, spatial camera, glTF |
| Fast art loop with Imagine + game-asset skills | Lighting, shadows, 3D props |
| Platformer, top-down, puzzle, visual novel UI | Third-person, FPS, diorama |

Both share the same production contracts (states, asset roots, remote features). Only camera and gameplay modules differ.

## Layout contract

```text
<game>/
  Cargo.toml              # Bevy 0.19, features remote + capture
  AGENTS.md               # skills + pins for this game
  assets/
    sprites/ | models/
    ui/
    audio/
  src/
    main.rs               # thin App entry
    lib.rs                # GamePlugin
    plugins/              # core, remote, …
    states.rs             # Loading | MainMenu | Playing | Paused
    systems/              # loading, menu, gameplay
    components.rs
    resources.rs
```

## Anti-demo rules

Treat the task as **failed** if the deliverable is only:

- A static mesh/sprite with no `Update` systems  
- No app states / no menu → play transition  
- No `assets/` (everything procedural forever)  
- Architecture invented ad hoc with no modules  

A **demo/fixture** is fine for BRP smoke tests. A **game** must be playable and extensible.

## Stable contracts

| Contract | Value |
|----------|--------|
| Bevy | **0.19** |
| bevy_brp_mcp / bevy_brp_extras | **0.22.1** |
| BRP port | **15702** |
| Features | `remote`, `capture` (`capture` ⇒ `remote`) |
| App states | `Loading`, `MainMenu`, `Playing`, `Paused` |
| Asset roots | `assets/sprites`, `assets/models`, `assets/ui`, `assets/audio` |

## MCP tools (verify)

See README for the full tool table. Minimum live loop:

1. `bevy_env_check`  
2. `bevy_launch_app` (or manual `cargo run --features remote,capture`)  
3. `bevy_brp_query` / `bevy_brp_mutate`  
4. `bevy_capture_viewport`  

For hierarchy, watches, and input injection, also register **`bevy_brp_mcp`**.

## Scaffold

```bash
specula scaffold --kind 2d --path ./my-2d-game
specula scaffold --kind 3d --path ./my-3d-game
specula scaffold --kind demo --path ./brp-fixture   # integration fixture only
```

## Phase-4 depth

- [Asset path conventions](ASSET_CONVENTIONS.md)  
- [Shipping / release notes](SHIPPING.md)  

## Roadmap

- **v0.2 (done):** skill pack, 2d/3d templates, scaffold kinds, MCP prompts/workflow  
- **v0.3 (in progress):** short demos meeting [GAME_DOD.md](GAME_DOD.md); see [ROADMAP.md](ROADMAP.md) (G1–G6)

Track status in [PROGRESS.md](../PROGRESS.md).