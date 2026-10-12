# Agent Sight Next — findings & assessments (2026-07-21)

## Status

**S0–S4 complete.** MCP/CLI rebuilt with profiles, ranking, collapse, `bevy_see_verify`. Dogfood Crystal Drift + Iron Feud Playing.

## Features (Specula S0)

- **`rank_primary_subject`** — Player / WaterBody / cameras over Crystal / OreCrystal / Stars  
- **`collapse_duplicate_names`** — `duplicate_count` on repeated Names  
- **Profiles** — `crystal-drift`, `iron-feud` (projection, wait, require_playing)  
- **`bevy_see_verify` / `specula see verify`** — full + primary fovea (+zoom)  
- Baseline save/compare + `auto_baseline` path  
- Motion optional `mutate_entity` / `mutate_translation` stimulus  
- Pack **views_similar** warning when alt hash == game  
- Diagnostic primary from **ranker/allowlist** (not Player-only)  
- GAMEPLAY_NAME_HINTS expanded so dogfood Names survive filter  

## Dogfood inventory

### Crystal Drift (`/Users/aron/Documents/coding_projects/Crystal Drift`)

| Quota | Item |
|-------|------|
| Addition 1 | `BeaconBuoy` + `sprites/beacon_buoy.png` |
| Addition 2 | `RescuePod` + `sprites/rescue_pod.png` (score + fuel) |
| Improvement | Player sprite scale/tint for fovea readability |
| Env mod | Nebula retint/reposition; DerelictStation moved/scaled |
| Env add | `DebrisRing` + `sprites/debris_ring.png` |

Live packet: `primary_subject=Player`; Names include BeaconBuoy, RescuePod, DebrisRing, Nebula*, DerelictStation.

### Iron Feud (`/Users/aron/Documents/coding_projects/Iron Feud`)

| Quota | Item |
|-------|------|
| Addition 1 | `RelayTower` + relay tint mesh |
| Addition 2 | `SupplyCrate` + crate tint mesh |
| Improvement | Water material clearer (tint + emissive) for fovea |
| Env mod | CliffRidge_West repositioned |
| Env add | `AshPlateau` + ash tint |

Live packet: `app_state=Playing`, `primary_subject=WaterBody`; Names include RelayTower, SupplyCrate, AshPlateau, WaterBody, Ground. `IRON_FEUD_AUTO_PLAY=1`.

## Tests / logs

- `cargo test -p specula -p specula-brp` PASS (ranking/collapse/profile unit tests)  
- Scratch: `{SCRATCH}/specula-tests.log`, `dogfood-see-2d.log`, `dogfood-see-3d.log`, build logs  

## Assessments

1. **Ranking fixed the biggest dogfood lie** — Player beats Crystal; WaterBody beats OreCrystal noise.  
2. **Filter must score new Names** — without GAMEPLAY_NAME_HINTS entries, additions vanished from packets even when spawned.  
3. **Profiles make sight operable** — iron-feud defaults remove hand-tuned projection/wait for agents.  
4. **`see verify` is the right one-shot** for current-gen agents (full + fovea).  
5. **Still not taste** — sight supports building to human needs; design remains human-owned.  
6. **No excluded features** — no livestream, editor, unlit suite, or beauty scoring.  

### Residual gaps (next optional, not this goal)

- OreCrystal still appears when scored as “Ore”; collapse helps but ranking already skips them for primary.  
- 3D fovea still approximate (good enough for WaterBody at non-center).  
- Dual simultaneous games need two BRP ports (sequential dogfood is fine).  
