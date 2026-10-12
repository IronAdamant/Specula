# demo_2d (2D)

**Short 2D Bevy demo** (Specula GAME_DOD) — not a movement sandbox.

## Objective

**Collect 3 gold orbs** while **avoiding the red hazard**.  
Collect all orbs → **Victory**. Touch the hazard → **Game Over**.

## Controls

| Input | Action |
|-------|--------|
| Enter / Space | Start from menu; from end screens → menu |
| WASD / arrows | Move |
| Esc | Pause / resume (also resume with Enter) |
| M | From pause → main menu |

## Run (dev + agent capture)

```bash
cargo run --features remote,capture
```

BRP port **15702**. Package for sharing: see repo `scripts/package-demo.sh` and `docs/PACKAGING.md`.

## Features

| Feature | Enables |
|---------|---------|
| `remote` | BRP HTTP on port **15702** |
| `capture` | Screenshots / diagnostics (implies remote) |

## Assets

| Path | Use |
|------|-----|
| `assets/sprites/player.png` | Player sprite (disk) |
| `assets/ui/`, `assets/audio/` | Ready for chrome/SFX |

## Ship / package

```bash
cargo build --release
# then copy binary + assets/ into a dist folder (see docs/PACKAGING.md)
```

## Monorepo dogfood

From the Specula workspace root:

```bash
cargo run -p demo_2d --features remote,capture
```

Package:

```bash
./scripts/package-demo.sh demo_2d games/demo-2d
```
