---
name: bevy-production
description: >
  Build production Bevy games (modular plugins, AppState, assets, ship checklist)
  for Bevy 0.19 — not one-file demos. Use when the user wants a real Bevy game,
  production architecture, shippable structure, "build a game", or /bevy-production.
  Chain with bevy-demo-game for short demos, bevy-2d-game or bevy-3d-game; for art
  load game-asset-core; for live verify load bevy-agent-loop.
metadata:
  short-description: "Production Bevy architecture (not demos)"
---

# Bevy production games

You are shipping a **maintainable game**, not a BRP cube demo.

Pins: **Bevy 0.19**, **bevy_brp_extras 0.22.1**, BRP port **15702**, features `remote` / `capture`.

**Short demos / “make it a real game”:** also load **`bevy-demo-game`** and obey repo **`docs/GAME_DOD.md`** (objective, challenge, win/lose—not only movement).

## Load order

1. This skill (always for real games).
2. **`bevy-demo-game`** when building a complete short demo (recommended default for demos).
3. **`bevy-2d-game`** or **`bevy-3d-game`** for the dimensional kit.
4. **`game-asset-core`** (+ specialist) whenever generating art.
5. **`bevy-agent-loop`** when running, capturing, or debugging live.

## Unprompted defaults

| Situation | Do this without being asked |
|-----------|----------------------------|
| New game | Modular layout below; states Loading → MainMenu → Playing → Paused → Victory/GameOver as needed |
| Short demo | Meet docs/GAME_DOD.md before calling done |
| Entry point | Thin `main.rs`; logic in `lib` + plugins/systems |
| Assets | `assets/sprites` or `models`, `ui`, `audio` — never only forever-procedural for a product |
| Remote control | Feature-gate `BrpExtrasPlugin`; Name important entities |
| Dev Cargo | `opt-level = 1` for the app, `3` for deps; release profile for ship |
| Demo sample | Use only for BRP smoke — never as the final game structure |

## Layout contract

```text
src/main.rs          # App::new + GamePlugin only
src/lib.rs           # pub struct GamePlugin
src/plugins/         # core (window/defaults), remote (cfg feature)
src/states.rs        # Loading | MainMenu | Playing | Paused | Victory | GameOver (as needed)
src/systems/         # loading, menu, gameplay, pause, end screens
src/components.rs
src/resources.rs
assets/{sprites|models,ui,audio}/
```

See `references/layout.md` for file responsibilities.

## Architecture rules

1. **One concern per module.** Gameplay systems do not configure windows; remote plugin stays behind `#[cfg(feature = "remote")]`.
2. **States own screens.** Menu UI despawns on exit; gameplay only runs in `Playing`.
3. **Resources for shared data** (score, settings); components for entity data.
4. **Name entities** (`Name::new(...)`) so BRP queries are readable.
5. **Reflect** any component you expect to mutate over BRP in advanced flows.
6. Prefer **Bevy built-ins** first; add crates only with a clear need and **0.19-compatible** pins:

| Need | Pin (Bevy 0.19) |
|------|-----------------|
| Physics 2D | `avian2d` **0.7** |
| Physics 3D | `avian3d` **0.7** |
| Tilemaps | maintained 0.19-compatible crate when required |

See `docs/PHYSICS.md`. Default kits stay transform-based (no physics dep).

7. **Query disjointness (B0001):** overlapping `Query<&mut T>` systems panic at runtime. Use marker components, `Without`, `ParamSet`, or split systems when growing the scaffold.

## Anti-demo (fail criteria)

Stop and restructure if the “game” is only:

- A single Startup system spawning a static mesh/sprite  
- No `Update` input or game logic  
- No state machine  
- No `assets/` directory for a product build  

## Ship checklist (before calling it done)

- [ ] **GAME_DOD** satisfied (objective, challenge, win and/or lose—not movement only)  
- [ ] Menu → play → pause (or quit) works  
- [ ] `cargo run --features remote,capture` works for agent iteration  
- [ ] `cargo build --release` succeeds  
- [ ] Assets load from disk paths used in code  
- [ ] Window title and package name match the game  
- [ ] README: how to run, features, controls, **objective**  
- [ ] Live capture reviewed when MCP is available (menu + play + end)  

## Art integration

1. Load **`game-asset-core`** (+ specialist).  
2. Write engine-ready files under the layout asset roots.  
3. Load with `AssetServer` (`"sprites/player.png"`, etc. relative to `assets/`).  
4. Capture viewport to verify scale, pivot, and background keying.

## References

- `references/layout.md` — module responsibilities  
- `references/states-snippet.md` — AppState wiring sketch (Bevy 0.19)  
