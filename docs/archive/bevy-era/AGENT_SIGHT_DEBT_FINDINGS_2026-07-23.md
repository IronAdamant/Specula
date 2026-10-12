# Agent Sight Debt — findings & assessments (2026-07-23)

## Status

**R0–R4 complete.** Residual filter/multi-view/fovea debts closed in Specula; Crystal Drift and Iron Feud full asset+env improve pass with one new feature and one new env each; live `see verify` green on rebuilt MCP.

## R0 — Specula residual sight

| Item | Result |
|------|--------|
| **Name onboarding** | `DOGFOOD_NAME_STEMS` + unit test `dogfood_stems_all_score_positive_including_r1_r2` |
| **R1/R2 stems** | SolarFlareBuoy, WarpGateRing, RadarDome, TerrainSaddle score >0 and survive OreCrystal spam |
| **3D alt nudge** | `alt_camera_nudge_translation` — side XZ + lift for topdown3d; landscape pack uses it |
| **Tall fovea** | `crop_half_for_entity` inflates WatchPost/OreSilo/RadarDome/TerrainPeak/… |
| **iron-feud half** | visible_half 22×22 (was 20) |
| **Baseline** | MCP/skill: save_baseline after asset/env change |
| **Tests** | `cargo test -p specula -p specula-brp` PASS; install `--force` before dogfood |

### Exclusions not implemented

Livestream, editor, unlit suite, auto taste.

## Crystal Drift (R1)

**Path:** `/Users/aron/Documents/coding_projects/Crystal Drift`

| Quota | Item |
|-------|------|
| New feature | `SolarFlareBuoy` + `sprites/solar_flare_buoy.png` — score + shield charge |
| New env | `WarpGateRing` + `sprites/warp_gate_ring.png` — large landmark |
| Improve all sprites | All inventory paths regenerated (Imagine + keying + variants): player, asteroids L/M/S, boost, crystal, fuel, enemy, scrap, shield, nebula, station, beacon, rescue, debris, comet, signal |
| Env pass | 6 nebulas higher alpha; station/debris/props retuned; on-screen start props |

**Live:** `primary_subject=Player`; Names include SolarFlareBuoy, WarpGateRing; PNG non-empty; no black_frame.

## Iron Feud (R2)

**Path:** `/Users/aron/Documents/coding_projects/Iron Feud` · `IRON_FEUD_AUTO_PLAY=1`

| Quota | Item |
|-------|------|
| New feature | `RadarDome` + `models/radar_tint.png` |
| New env | `TerrainSaddle` + `models/saddle_tint.png` (mid-band; in height samples) |
| Improve all tints | ground, rock, tree, scrap, cliff, water, relay, crate, ash, watch, silo, hill, peak → 64×64 richer noise/wave/stripe |
| Env pass | Brighter sun/fill; larger ground; taller cliff; height bands retained |

**Live:** Playing; primary=StrategyCamera; WaterBody + RadarDome + TerrainSaddle + TerrainPeak_* present; OreCrystal filtered; height_bands warning on landscape.

## Packet / primary paths

| Game | Primary | Key artifacts |
|------|---------|---------------|
| CD | Player | `captures/eyesight/verify_packet.json`, `scene_full.png` |
| IF | StrategyCamera | same under Iron Feud; landscape pack height_bands note |

Scratch: `{SCRATCH}/dogfood-see-2d.log`, `dogfood-see-3d.log`, `eyesight/cd|if/`, build logs, `mcp-surface.log`.

## Assessments

1. **DOGFOOD_NAME_STEMS table + unit test** makes Name onboarding enforceable, not folklore.  
2. **Side XZ nudge** alone is not enough for strategy-camera multi-view honesty — **perceptual `views_similar`** (`captures_look_similar`: hash **or** mean abs &lt; 0.02) is required; live IF landscape now warns correctly (mean_abs ~1.7e-5).  
3. **Full inventory asset replace** must use **opaque craft on transparent BG** (no magenta plates). Imagine+key failed dogfood; clean PIL silhouettes + in-frame env positions fixed full-frame readability (CD scene_full ~1.3MB, 81% non-black, WarpGate/station/nebulas painted).  
4. **Env must be inside ortho frame** — far-off landmarks Named/on_screen via projection but still black in pixels if outside practical composition.  
5. **TerrainSaddle as height sample** ties new env to band unit tests without breaking start placement.  
6. Taste remains human-owned.

### Residual gaps

- 3D landscape alt remains nearly identical at high strategy height even with large XZ nudge — warning is honest; true multi-angle needs a dedicated side camera later.  
- Dual BRP still sequential on 15702 (documented only).

## Tests / rebuild

- specula-brp unit tests including dogfood stems, alt nudge, tall crop half  
- CD/IF `cargo build --features remote,capture` PASS  
- IF `height_terrain_has_three_distinct_bands` PASS  
