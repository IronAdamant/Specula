# demo_3d (3D)

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

## Monorepo dogfood

From the Specula workspace root:

```bash
cargo run -p demo_3d --features remote,capture
```

Package:

```bash
./scripts/package-demo.sh demo_3d games/demo-3d
```
