# Agent Eyesight — findings (2026-07-21)

Concise report for Obsidian. Product: **agent eyesight** (not a Bevy editor).

## Features added (copy for Obsidian)

- **`specula.sight/v1` packet schema** — captures + subjects + stimulus + style_intent + warnings + agent_must
- **`bevy_see_scene` / CLI `specula see scene`** — E0 full frame + Name/Transform subjects + abs_path
- **`bevy_see_entity` / `see entity`** — E1 fovea crop around screen point for a named entity
- **`bevy_see_region` / `see region`** — E1 pixel-rect crop (landscape / water / HUD patches)
- **`bevy_see_motion` / `see motion`** — E2 multi-frame capture + horizontal montage strip (+ optional keys)
- **`bevy_see_diff` / `see diff`** — E3 baseline vs after + abs-diff PNG + mean score
- **`bevy_see_pack` / `see pack`** — multi-view presets: entity_craft, landscape, water, physics_jump, lighting
- **Black-frame warning** — empty-window heuristic (mean + max luminance; dark space scenes not false-positive as empty)
- **MCP + CLI + skill contract** — tools in Specula MCP; `bevy-agent-loop` eyesight rules; `verify_scene` routes to see_scene
- **Unit tests** — crop, montage, black-frame, packet schema, subject parse, diff score (shipped `specula-brp` paths)

### Crystal Drift (2D dogfood)

- **New assets (7):** `crystal.png`, `fuel_canister.png`, `enemy_scout.png`, `scrap.png`, `shield_orb.png`, `nebula.png`, `station.png`
- **Environment:** nebula field (4 parallax clouds) + `DerelictStation` landmark
- **Gameplay:** scrap collect bonus score; shield orbs (absorb hits); sprites for crystals/fuel/enemies; scrap/shield drops from rocks

### Iron Feud (3D dogfood)

- **New textures (5):** `rock_tint.png`, `tree_tint.png`, `scrap_tint.png`, `cliff_tint.png`, `water_tint.png`
- **Environment:** rock outcrops, tree scrubs, cliff ridge, landscape plateau naming, water body (existing + eyesight packs)
- **Gameplay:** field scrap piles + **G** salvage → `ScrapSalvage` counter on HUD

## What was seen (live BRP)

| Target | Evidence |
|--------|----------|
| Crystal Drift | Playing: Player, NebulaCloud_*, DerelictStation, Crystal, Asteroids; **`black_frame_warning` absent** after heuristic fix; non-empty PNGs under `captures/eyesight/` |
| Iron Feud | **Playing** via `IRON_FEUD_AUTO_PLAY=1`: RockOutcrop_*, TreeScrub_*, CliffRidge_West, FieldScrap_*, WaterBody, Ground; 11 unique MD5 PNGs (scene/water/landscape/motion); water uses `water_tint` texture |

Motion strip sizes grew across frames on Crystal Drift (entities moving) — temporal channel works.

## Tests

- `cargo test -p specula -p specula-brp` — **PASS**
- Crystal Drift / Iron Feud `cargo build --features remote,capture` — **PASS**
- Live CLI eyesight against both games on port 15702 — **PASS** (real PNGs, schema packets)

## Suggestions (next)

1. **Auto world→screen for entity crops** using camera + projection when available (better fovea than center default).  
2. **Filter subjects** (cap Stars; prefer gameplay Names) to keep packets small.  
3. Prefer `IRON_FEUD_AUTO_PLAY=1` (or Enter) for agent dogfood so Playing-world packs show water/scrap by default.  
4. **Higher-fidelity art** for new Crystal Drift sprites (current are geometric placeholders).  
5. **Optional short video** export from motion frames for physics feel reviews.  
6. **Unlit diagnostic** needs a small game-side feature flag (documented as V4/V5 optional).

## Dead code

- No large dead-code purge required; unused helpers in dogfood games pre-existed (warnings only). Left alone where still useful for future systems.

## Paths

- Plan: `docs/AGENT_EYESIGHT_PLAN.md`  
- Code: `crates/specula-brp/src/eyesight.rs`, MCP tools in `crates/specula/src/mcp.rs`, CLI `see`  
- Skill: `.grok/skills/bevy-agent-loop/` + `references/eyesight-packs.md`  
