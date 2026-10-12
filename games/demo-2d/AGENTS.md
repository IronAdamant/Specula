# Project agent rules — 2D Bevy game (`demo_2d`)

## Pins
- Bevy **0.19**, bevy_brp_extras **0.22.1**, BRP port **15702**
- Features: `remote`, `capture`

## Skills
1. `bevy-production` + `bevy-2d-game`
2. Art → `game-asset-core` (+ specialist)
3. Live verify → `bevy-agent-loop`

## Assets
`assets/sprites/`, `assets/ui/`, `assets/audio/` (paths relative to `assets/` for AssetServer).

## States
`Loading` → `MainMenu` → `Playing` / `Paused`

## Ship
```bash
cargo build --release
```
See Specula `docs/ASSET_CONVENTIONS.md` and `docs/SHIPPING.md` when available.
