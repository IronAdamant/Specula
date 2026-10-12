# Agent Sight 2D + 3D — findings & assessments (2026-07-22)

## Status

**D0–D5 complete.** Dimension-specific sight polish shipped; Crystal Drift and Iron Feud dogfooded with new Named features + assets; IF multi-band height terrain; MCP rebuilt before pass evidence; live `see verify` + packs reviewed with open PNG observations.

## Features (Specula D0)

| Area | Change |
|------|--------|
| **Profiles** | `crystal-drift` / `2d` / `ortho2d`; `iron-feud` / `3d` / `topdown3d` (projection, half-extents, wait_for, require_playing) |
| **GAMEPLAY_NAME_HINTS** | D1/D2 stems: Comet, Fragment, Signal, Sat, Watch, Post, Silo, Terrain, Hill, Peak, Height, … so new Names survive `gameplay_prefer` |
| **PRIMARY_PREFIXES** | CometFragment, SignalSat, WatchPost, OreSilo, TerrainFlat/Hill/Peak, HeightTerrain |
| **Region presets** | Pure `RegionPreset` / `region_preset_rect` — `hud_top_left`, `hud_top_bar`, `horizon_band`, `ground_band`, `center_half` |
| **Packs** | **`hud`**, **`env_2d`** added; landscape notes height readability + `height_bands` warning when Terrain* Names present; larger 3D camera nudge |
| **Filter/rank** | Demote OreCrystal* and child mesh parts; boost PRIMARY_EXACT; do not treat `WaterBody` as noise (`…Body` compound only) |
| **`bevy_see_verify`** | Remains default one-shot; stills-only; multi-view `views_similar` warning kept |
| **Unit tests** | Profile apply, region rects, D1/D2 name scores, height_band notes, known packs, filter prefers WaterBody/terrain over ore/children |

### Exclusions (not implemented)

- Livestream / 60 FPS continuous video  
- Human editor / hierarchy UI / gizmos  
- Full unlit material suite  
- Auto taste / beauty scoring  

## Crystal Drift (2D) D1

**Path:** `/Users/aron/Documents/coding_projects/Crystal Drift`

| Quota | Item |
|-------|------|
| Feature 1 | `CometFragment` + `assets/sprites/comet_fragment.png` — collectible hazard (score + fuel drain) |
| Feature 2 | `SignalSat` + `assets/sprites/signal_sat.png` — scan collectible / landmark |
| Env improve | Nebula field re-layered (5 clouds, stronger tints); DerelictStation larger/warmer; DebrisRing larger + retinted/repositioned |

**Live verify:** `primary_subject=Player`; subjects include `CometFragment`, `SignalSat`; PNG bytes > 0; no false black_frame.  
**Packs:** `env_2d`, `landscape` with non-empty captures.

### Observation (opened images)

- **Full:** dark space, Player center, satellite + icy comet fragment on-screen, HUD top-left, asteroids, soft nebulas.  
- **Player fovea:** craft readable (cyan engines).  
- Env improve visible as wider nebula placement and station/debris framing (not rename-only).

## Iron Feud (3D) D2

**Path:** `/Users/aron/Documents/coding_projects/Iron Feud`  
**Playing:** `IRON_FEUD_AUTO_PLAY=1`

| Quota | Item |
|-------|------|
| Feature 1 | `WatchPost` + `assets/models/watch_tint.png` — elevated lookout mesh |
| Feature 2 | `OreSilo` + `assets/models/silo_tint.png` — industrial silo mesh |
| Height terrain | `TerrainFlat`, `TerrainHill_N/E`, `TerrainPeak_N/W` via `height_terrain_samples()` — three distinct top_y bands (flat ~0.15, hill ~2.2–2.5, peak ~5.4–5.8) |
| Env improve | Taller `CliffRidge_West`; rocks near hills; clearer water/ground tints |

**Unit proof:** `height_terrain_has_three_distinct_bands` + `height_bands_are_distinct()`.  
**Start cells:** hills/peaks placed outside factory pocket (~0..10,0..10).

**Live re-verify (after D4 ranking fix):**  
`app_state=Playing`; `primary_subject=StrategyCamera` (camera/env tier OK); subjects include WaterBody, WatchPost, OreSilo, all Terrain* bands; OreCrystal* filtered out; landscape pack emits `height_bands present…` warning.

### Observation (opened images)

- **Full / landscape game:** flat start green; mid-height blocks (hills); taller brown/grey volumes (peaks/cliff); blue water; WatchPost tower + OreSilo; not a single flat plane.  
- **Landscape alt:** still multi-height blocks (alt ≈ game from high strategy camera; do not over-claim multi-angle if `views_similar`).

## Packet / primary paths (live)

| Game | Primary | Key packets under game `captures/eyesight/` |
|------|---------|-----------------------------------------------|
| CD | Player | `verify_packet.json`, `scene_full.png`, `entity_Player_crop.png`, `pack_env_2d_*`, `pack_landscape_*` |
| IF | StrategyCamera | `verify_packet.json`, `scene_full.png`, `pack_landscape_*`, `pack_water_*`, `entity_WatchPost_*`, `entity_WaterBody_*` |

Scratch copies: goal implementer `{SCRATCH}/eyesight/cd|if`, logs `dogfood-see-2d.log`, `dogfood-see-3d.log`, `d0-test-build.log`, `mcp-surface.log`, build logs.

## Assessments

1. **2D packs (`hud`, `env_2d`) + region presets** make HUD/parallax composition inspectable without inventing pixel rects each session.  
2. **3D height bands need both mesh relief and Named stems** — code-measurable tops + landscape height_bands warning close the “flat ground lie.”  
3. **Filter scoring is load-bearing** — OreCrystal scoring via “Ore”+“Crystal” and child `…Body` parts crowded out WaterBody until D4 demotions (and careful WaterBody ≠ child Body).  
4. **`see verify` first** remains the right habit; packs second.  
5. **Profiles must match dimension** — wrong half-extents ruin fovea.  
6. **Stale MCP binary still falsifies dogfood** — install `--force` before pass evidence.  
7. **Taste remains human-owned** — agent sees and builds to requirements only.

### Residual gaps

- Strategy camera alt vs game often hash-similar; larger nudge helps but true multi-angle 3D may need deliberate side camera later.  
- OreCrystal still exists in world (good for mining) — filter hides them from packets when scored correctly.  
- CD nebulas remain subtle on full frame (readability tradeoff with dark space).  
- Dual simultaneous BRP still sequential-only on 15702.

## Tests / rebuild

- `cargo test -p specula -p specula-brp` PASS  
- `cargo build -p specula` PASS  
- `cargo install --path crates/specula --force` before final IF re-verify  
- CD/IF `cargo build --features remote,capture` PASS; IF height unit test PASS  

## Docs / skills

- Plan: [AGENT_SIGHT_2D3D_PLAN.md](AGENT_SIGHT_2D3D_PLAN.md)  
- Skill: `.grok/skills/bevy-agent-loop` (+ eyesight-packs reference)  
- Prior: [AGENT_SIGHT_NEXT_FINDINGS_2026-07-21.md](AGENT_SIGHT_NEXT_FINDINGS_2026-07-21.md)  
