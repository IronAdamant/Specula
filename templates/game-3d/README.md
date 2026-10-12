# __WINDOW_TITLE__

**Short 3D Bevy demo** (Specula GAME_DOD).

## Objective

**Collect 3 cyan cubes** on the platform.  
**Avoid the red hazard.** **Falling off** the platform ends the run.

- All cubes → **Victory**  
- Hazard or fall → **Game Over**

## Controls

| Input | Action |
|-------|--------|
| Enter / Space | Start; from end → menu |
| WASD / arrows | Move on XZ |
| Esc | Pause / resume (Enter also resumes) |
| M | Pause → main menu |

## Run

```bash
cargo run --features remote,capture
```

BRP port **15702**. Packaging: repo `scripts/package-demo.sh`, `docs/PACKAGING.md`.

## Assets

`assets/models/ground_tint.png` (disk). `assets/ui/`, `assets/audio/` ready for more content.
