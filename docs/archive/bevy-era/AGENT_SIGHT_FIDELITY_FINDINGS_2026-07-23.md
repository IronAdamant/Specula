# Agent Sight Fidelity — findings (2026-07-23)

## Status

**F0–F6 complete.** Residual multi-view orbit + Name stems; CD `CargoPod` + full transparent sprite inventory; IF `OreCrusher` + multi-part machine polish + stronger heightfield; live verify green on both dogfood trees.

## F0 platform

- Stems: `CargoPod`, `RepairDrone`, `OreCrusher`, `SignalRelay` (score >0 + unit tests)
- `side_orbit_camera_translation`: high strategy Y drops altitude and widens lateral offset
- Tall prefixes include OreCrusher / SignalRelay / LoadingBay
- `cargo test -p specula -p specula-brp` PASS; MCP `cargo install --path crates/specula --force`
- Surface plan pointer → `AGENT_SIGHT_FIDELITY_PLAN.md`

## F1 Crystal Drift

| Item | Detail |
|------|--------|
| New feature | `CargoPod` + `sprites/cargo_pod.png` (collect for score) |
| Sprites | Full inventory regenerated; complex silhouettes; **transparent BG**; true-magenta audit exit 0 |
| Env | Higher-alpha nebulas; warp gate / station / debris readable on full frame |
| Live | primary=`Player`; `CargoPod` in subjects; full nonblack≈0.79 after env refresh; magenta 0 |

## F2 Iron Feud

| Item | Detail |
|------|--------|
| New feature | `OreCrusher` multi-part (chassis/hopper/jaws/flywheel/chute) + `crusher_tint.png` |
| Machines | Drills/belts already multi-part; chests feet; accumulator terminals; assembler arm/inlet; solar stand; multi-part rock |
| Env props | Multi-part (not single cuboid): cliff face/ledge/buttress/cap; supply crate body/lid/band/feet; ash base/berm/mound; terrain saddle deck/abutments/crown; field scrap base/shards/pipe |
| Terrain | Stronger multi-octave `heightfield_y`; variance + +Y winding tests PASS |
| Test | `env_props_are_multipart_in_spawn_source` — every F2 env spawn uses `add_children` |
| Live | Playing; primary=`StrategyCamera`; `OreCrusher` + env parents present; child Names (e.g. ScrapPipe, SaddleAbut*) in packets; landscape game≠alt; nonblack≈1.0 |

## Assessments

1. Transparent code-built sprites remain more reliable than Imagine+magenta key.  
2. High-alpha nebulas are required for “env shows through” on black space.  
3. Side-orbit high-Y drop improves multi-view difference under strategy cam.  
4. Heightfield must stay primary ground; landmark slabs stay small.  
5. Sibling git keeps CD/IF craft durable outside monorepo.

## Packet / evidence paths (scratch)

- `{SCRATCH}/gbr-tests.log`, `mcp-surface.log`
- `{SCRATCH}/crystal-drift-build.log`, `sprite-audit.log`, `dogfood-see-2d.log`, `eyesight/cd/`
- `{SCRATCH}/iron-feud-build.log`, `iron-feud-tests.log`, `dogfood-see-3d.log`, `eyesight/if/`

## F2.1 subject ranking fix (same day)

- Expanded `is_child_mesh_part` for F2 multiparts; protect top-level dogfood Names.
- `gameplay_subject_score`: exact `DOGFOOD_NAME_STEMS` +120 so OreCrusher survives max_subjects=24.
- Unit test `filter_keeps_ore_crusher_when_multiparts_crowd` PASS.
- Live re-verify: **OreCrusher in subjects**; `see entity --name OreCrusher` crops OK; proof `{SCRATCH}/orecrusher-subject-proof.txt`, `dogfood-see-3d-oreprove2.log`.

## Residual (optional later)

- Strategy multi-view still limited by camera FOV; dedicated spawn camera remains optional.
