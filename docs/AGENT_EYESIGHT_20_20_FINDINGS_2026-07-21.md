# Agent Eyesight 20/20 — findings (2026-07-21)

## Status

**A0–A8 implemented** in Specula. Observation-grade acuity (`acuity: "20/20-candidate"`) on live dogfood.

**Honest claim:** agents can **see** at high fidelity (composition, true fovea, multi-view, motion, diffs, filtered subjects). **Taste and design remain human-owned.**

## What shipped

| Phase | Delivered |
|-------|-----------|
| A0 | `wait_for_subjects`, `require_playing`, inferred `app_state`, menu-only warnings |
| A1 | World→screen fovea (`ortho2d` / `topdown3d`), zoom ladder, `screen_xy` / `screen_aabb` |
| A2 | Pack multi-view: game + camera-nudge alt (`views: game, alt`) |
| A3 | Default 8-frame motion; static_scene warning when frames identical |
| A4 | `subject_filter=gameplay_prefer`, max 48, Stars deprioritized |
| A5 | `save_baseline` / `compare_baseline` on `see scene` |
| A6 | `pack=diagnostic` + bounds outline on fovea crops |
| A7 | README honesty, skill acuity rules, this findings file |
| A8 | Unit tests for filter/projection/black-frame; multi-target docs in skill |

## MCP / CLI (updated)

- Tools: `bevy_see_scene|entity|region|motion|diff|pack` with acuity params  
- CLI: `specula see scene|entity|region|motion|diff|pack`  
- Pack: `entity_craft|landscape|water|physics_jump|lighting|diagnostic`  
- Initialize instructions mention 20/20 acuity + human-owned taste  

## Dogfood

### Crystal Drift (Playing)

- Scene: `app_state=Playing`, `acuity=20/20-candidate`, Player/Nebula/Asteroid/Crystal; no Star spam dominance  
- Entity Player: fovea + zoom ladder  
- Baseline save + compare after mutate → `diff` role  
- Motion strip after player move  

### Iron Feud (`IRON_FEUD_AUTO_PLAY=1`, Playing)

- Scene: WaterBody, Ground, RockOutcrop_*, FieldScrap_*, StrategyCamera; **not** MenuCamera  
- Entity WaterBody: **world→screen projection** `@ (2016,864)` (not center fallback)  
- Water/landscape packs: `views: [game, alt]` via StrategyCamera nudge  
- Diagnostic pack: bounds overlay PNG  

## Tests

`cargo test -p specula -p specula-brp` — pass (incl. filter, projection, black-frame dark-space).

## For human MCP users

Same tools as agents. Use eyes to **verify** what you asked for. You still own art direction and “is this good?”

## Follow-ups (optional)

- Tighter 3D projection using full camera matrix  
- Prefer WaterBody over OreCrystal* in primary_subject ranking  
- Dual-port simultaneous dogfood documentation sample  
