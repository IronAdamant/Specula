# Agent Sight Trust — findings (2026-07-25)

## Status

**T0–T6 complete** (platform). Structural demotion, pack camera restore, dedicated multi-view path, regression tests; live workflow proof after MCP install.

## T0 — Fovea profile baseline

- `topdown3d_projects_ore_crusher_off_center`, `match_subjects_prefers_exact_name` PASS.
- CLI/MCP `--profile iron-feud|crystal-drift` on `see entity` (prior ship).

## T1 — Structural demotion

- `EyesightSubject.parent_entity` + `extract_parent_entity` (ChildOf/Parent BRP).
- `is_structural_child_of_named_root` / `is_noise_subject` / `is_protected_top_level_name`.
- Filter uses structural demotion with string `is_child_mesh_part` fallback.
- Tests: `structural_child_via_parent_entity_demotes`, `solar_flare_buoy_not_demoted_as_solar_prefix`, multipart flood.

## T2 — Pack camera restore

- `capture_with_camera_nudge`: capture in closure; **always** restore after (even if capture errors).
- Notes: `[restored]` / `[restore_failed]`; warnings on restore fail.
- `pack_camera_restore_value` pure helper + unit test; source contract test.

## T3 — Stronger multi-view

- `dedicated_side_camera_translation` after alt + side-orbit still similar.
- Pack path may emit `dedicated_side` view; honest `views_similar` if all match.
- Unit: dedicated more aggressive than side-orbit.

## T4 — Regression

- 53+ unit tests on `specula-brp`.
- Workflow aligned: entity requires `--profile iron-feud`; restore is code-level.

## T5 — Live / workflow

- **agent-sight-dogfood-3** full run: **overall `passed: true`**, skeptic **green**.
- Gates: tests PASS (53+ brp); `mcp_rebuilt=false` (skip_install).
- CD: primary Player; CargoPod present; magenta 0.
- IF: StrategyCamera; OreCrusher @ (1949,801) topdown3d XZ; packs `[mutated][restored]`; camera [3,28,10] post-pack; multi-view game≠alt.
- Evidence: `{SCRATCH}/gbr-tests.log`, `mcp-surface.log`, `workflow-full-report.md`, `workflow-full.log`.

## Residual

- BRP may not always reflect ChildOf; string fallback remains.
- Dedicated-side path still strategy-cam nudge (not ECS camera spawn).
