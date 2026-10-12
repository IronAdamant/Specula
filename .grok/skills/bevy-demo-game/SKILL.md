---
name: bevy-demo-game
description: >
  Turn a Bevy vertical slice into a complete short demo game that meets
  GAME_DOD (menu, objective, challenge, win/lose, pause, assets, remote/capture).
  Use when building a Bevy demo, finishing a scaffolded game, dogfood demo-2d/demo-3d,
  "make it a real game", or /bevy-demo-game. Chain with bevy-production and
  bevy-2d-game or bevy-3d-game; verify with bevy-agent-loop.
metadata:
  short-description: "Short Bevy demo DoD — not just movement"
---

# Bevy short demo game

You are finishing a **playable short demo**, not a movement sandbox.

**Authority:** repo `docs/GAME_DOD.md` (read it; do not invent a weaker bar).  
**Pins:** Bevy **0.19**, features `remote`/`capture`, BRP port **15702**.

## Load order

1. **`bevy-demo-game`** (this skill)  
2. **`bevy-production`** (layout, plugins, anti-demo)  
3. **`bevy-2d-game`** or **`bevy-3d-game`**  
4. Art → **`game-asset-core`** (+ specialist)  
5. Live verify → **`bevy-agent-loop`**  
6. Package later → packaging skill / workflow `package_demo` when asked  

## Where to work

| User intent | Location |
|-------------|----------|
| “The demo” / monorepo dogfood | `games/demo-2d` or `games/demo-3d` if present; else templates |
| New external game | `specula scaffold --kind 2d\|3d --path <dir>` then implement DoD |
| BRP smoke only | `scaffold --kind demo` — **never** call this the product game |

## Required loop (implement all)

1. **MainMenu** → start → **Playing**  
2. **Objective** (collect N / reach zone / survive T / …) shown in HUD or copy  
3. **Challenge** (hazard, enemy, timer, obstacle)  
4. **Victory** and/or **GameOver** states with clear screen text  
5. **Paused** freezes gameplay; resume or return to menu  
6. Disk **assets/** + `AssetServer`  
7. `Name` on Player and key entities  
8. README: controls + objective  

## Reject as incomplete

- Only WASD / movement  
- No end state  
- Procedural-only forever with no `assets/`  
- Cube BRP fixture sold as the demo  

## Portfolio / one-shot slices (optional path)

If the user wants a **portfolio vertical slice** that skips MainMenu (direct Loading → Playing):

- Allowed only when they **explicitly** ask for one-shot / no-menu.  
- Still require: objective, challenge, at least GameOver (or Victory), `remote,capture`, disk assets, named Player.  
- Document the choice in the game README.  
- Default for “short demo” / monorepo dogfood remains **full GAME_DOD** (menu + pause + end).

## Capture acceptance (agent)

Before claiming done, capture (or obtain captures of):

1. Main menu (skip only for explicit one-shot portfolio)  
2. Mid-play with objective visible  
3. Win or lose screen  

Use MCP: shell or `bevy_launch_app` (`wait_secs=0`) → **`bevy_wait_brp`** → `bevy_capture_viewport` (skill `bevy-agent-loop`).

## Workflow MCP

- `bevy_workflow` goal `complete_demo_2d` or `complete_demo_3d`  
- Prompts: `build_demo_2d`, `build_demo_3d` when available  

## After systems work

- Content/art pass with user briefs + `game-asset-*`  
- `cargo build --release`; document assets-beside-binary  
- Packaging phase (G4) when user asks to distribute  

## Related docs

- `docs/GAME_DOD.md`  
- `docs/ROADMAP.md`  
- `docs/PRODUCTION_GAMES.md`  
