# Agent Sight Hardening — findings (2026-07-23)

## Status

**H0–H6 complete.** Pixel/magenta gates + side-orbit multi-view path; CD/IF versioned as sibling git repos; CD PulseMine + transparent sprite inventory; IF LoadingBay + multi-part drill/belt meshes + continuous heightfield; live verify green.

## H0 platform

- `png_nonblack_fraction`, `png_true_magenta_pixel_count` (+ path helpers)
- Landscape: alt nudge then **side-orbit** second path when similar; `views_similar` if still match
- `side_orbit_camera_translation`; DOGFOOD stems PulseMine/LoadingBay/…
- `scripts/check_sprite_transparency.py`
- Tests green; MCP install --force

## H1 versioning

- Crystal Drift + Iron Feud: local `.git` with craft commits
- Doc: `docs/DOGFOOD_REPOS.md`

## H2 Crystal Drift

| Item | Detail |
|------|--------|
| New feature | `PulseMine` + `sprites/pulse_mine.png` (contact hazard + score) |
| Sprites | Full inventory regenerated; transparent BG; true-magenta audit exit 0 |
| Live | primary=Player; PulseMine in subjects; full nonblack≈0.85; magenta 0 |

## H3 Iron Feud

| Item | Detail |
|------|--------|
| New feature | `LoadingBay` multi-part dock mesh + `bay_tint.png` |
| Machines | Drill treads/cab/boom/bit; belt rails/rollers/chevrons |
| Terrain | `build_heightfield_mesh` + `heightfield_y` seeded; start pocket flat; unit `heightfield_randomized_with_flat_start_pocket` |
| Live | Playing; LoadingBay present; side_orbit view path; height_bands warning |

## Assessments

1. Pixel gates close Names≠pixels class of failures.  
2. Side-orbit still often similar under high strategy cam — warning is correct.  
3. Transparent PIL craft beats Imagine+magenta key for dogfood reliability.  
4. Heightfield + named peaks give continuous relief and packet subjects.  
5. Sibling git makes craft durable.

### Residual

- Strategy multi-view still weak at high Y; dedicated camera entity optional later.  
- Not every MachineKind re-sculpted equally — drills/belts prioritized; others already multi-part from prior work.


## H5 skeptic fixes (same day)

| Issue | Fix |
|-------|-----|
| Heightfield faces -Y (black void) | Restore upward winding `[i0,i2,i1,…]` + unit test `upward_winding_has_positive_y_normal` |
| Landscape identical multi-view | BRP mutate needs `[x,y,z]` sequence not `{x,y,z}`; `translation_value_for_brp` + surface mutate failures |
| Slab-dominated terrain | Named hill/peak samples shrunk to small landmarks; continuous heightfield is primary Ground |
| Post-fix evidence | Fresh `{SCRATCH}/eyesight/if/` after winding+mutate fixes; landscape game≠alt; nonblack≈0.92 |
