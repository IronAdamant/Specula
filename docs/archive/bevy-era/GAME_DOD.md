# Demo game — definition of done (DoD)

**Product maturity:** alpha.  
**Purpose:** Stop agents from treating “WASD on a plane” as a finished game.

A **short demo** is complete only when a player (or agent reviewing captures) can experience a full loop: menu → play → outcome → back or quit.

This applies to:

- Scaffolded games (`specula scaffold --kind 2d|3d`)  
- In-repo dogfood demos (`games/demo-2d`, `games/demo-3d` when present)  
- Any “build a Bevy demo” agent task in this ecosystem  

Art may be placeholders. **Systems may not be.**

---

## Hard requirements (2D and 3D)

| # | Requirement | Capture / verify |
|---|-------------|------------------|
| 1 | **Main menu** with start action (key and/or click) | Capture shows menu text/UI |
| 2 | **Playing** state with player control | Capture after input / BRP move |
| 3 | **Clear objective** (collect N, reach zone, survive T, etc.) | HUD or menu states the goal |
| 4 | **Win and/or lose end state** (dedicated state or screen) | Capture of Victory and/or GameOver |
| 5 | **Pause** (or equivalent) that freezes gameplay | Esc (or documented key) |
| 6 | **Return to menu or quit** from pause/end | Documented control works |
| 7 | **At least one challenge** (hazard, enemy, timer, or obstacle—not empty sandbox) | Visible in play capture |
| 8 | **Disk assets** under `assets/` (not only forever-procedural) | `AssetServer` path used |
| 9 | **Named entities** for BRP (`Player`, cameras, key props) | `bevy_brp_query` readable |
| 10 | **Features** `remote` + `capture` for agent loop | App runs with those features |
| 11 | **README** lists controls + objective | Human can play without source |

## Soft requirements (demo quality)

- Score or progress display during play  
- Audio path ready (`assets/audio/` + optional load; silent OK if documented)  
- Consistent window title / package name  
- Release note: `cargo build --release` + run with `assets/` beside binary (see packaging docs)

## Explicit non-done

Fail the task if the deliverable is only:

- Movement with no objective or end state  
- Static mesh/sprite showcase  
- BRP cube fixture (`scaffold --kind demo`) presented as the product  
- Menu that never reaches a win/lose outcome  

---

## 2D-oriented examples (any one pattern is enough)

- Collect **N** pickups, avoid hazard or time limit → Victory / GameOver  
- Reach an **exit** zone; spikes or chaser cause GameOver  
- Survive until timer ends; one enemy type  

## 3D-oriented examples

- Reach a **goal volume** on a plane; falling off or timer → GameOver  
- Collect **N** props; simple moving hazard  
- Stand in zone for T seconds while dodging  

---

## Agent verification loop

1. Load `bevy-demo-game` + dimensional skill + `bevy-agent-loop`.  
2. Run with `--features remote,capture`.  
3. Captures: **menu**, **mid-play (objective visible)**, **end state**.  
4. Only then mark complete or package.

Workflow goals: `complete_demo_2d`, `complete_demo_3d` (MCP `bevy_workflow`).

---

## Related

- [PRODUCTION_GAMES.md](PRODUCTION_GAMES.md) — architecture & skills  
- [SHIPPING.md](SHIPPING.md) — release basics  
- [ROADMAP.md](ROADMAP.md) — G1–G6 game factory plan  
