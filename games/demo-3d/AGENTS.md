# Project agent rules — 3D Bevy game (`demo_3d`)

## Pins
- Bevy **0.19**, bevy_brp_extras **0.22.1**, BRP port **15702**
- Features: `remote`, `capture`

## Skills
1. `bevy-production` + `bevy-3d-game`
2. UI/art → `game-asset-core` (+ specialist)
3. Live verify → `bevy-agent-loop`

## Assets
`assets/models/`, `assets/ui/`, `assets/audio/` (optional `sprites/`).

## States
`Loading` → `MainMenu` → `Playing` / `Paused`

## Ship
```bash
cargo build --release
```
See Specula `docs/ASSET_CONVENTIONS.md` and `docs/SHIPPING.md` when available.
