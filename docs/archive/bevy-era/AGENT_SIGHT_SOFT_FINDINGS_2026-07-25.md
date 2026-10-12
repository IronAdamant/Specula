# Agent Sight Soft Edges — findings (2026-07-25)

## Status

**E0–E7 complete.** Soft edges S1–S5 investigated; platform fixes landed where BRP allows; CI matrix documented; full `agent-sight-dogfood` mode=full is the closeout bar.

## S1 Hierarchy

| | |
|--|--|
| **Probe** | Live IF: `ChildOf`-only BRP query returns ~103 entities with bare parent `u64`. `Name+Transform+ChildOf` multi-component query returns **0** (AND filter). |
| **Decision** | **A-partial** — merge parents via second query. |
| **Fix** | `parent_map_from_childof_query` + `merge_childof_parents` in `query_all_subjects`. |
| **Evidence** | `{SCRATCH}/hierarchy-brp.json`; unit `parent_map_from_childof_bare_u64`. |

## S2 Multi-view

| | |
|--|--|
| **Probe** | `BrpClient::spawn_entity` exists; switching **active** render camera via BRP is unreliable for viewport capture. |
| **Decision** | Keep StrategyCamera nudge chain (alt → side-orbit → dedicated-side). |
| **Fix** | `find_dedicated_view_camera` for optional `AgentSightSideCamera` / `SideCamera` Names if games place one later. |
| **Evidence** | Unit `find_dedicated_view_camera_prefers_named`; pack still restores after nudge. |

## S3 Fovea craft readability

| | |
|--|--|
| **Probe** | Aim can be correct (OreCrusher ~1949,801 topdown3d) while crop is deep shadow. Old gate thr=30 + `FOVEA_CROP_NONBLACK_MIN=0.05` **missed** live shadow crops (thr30 nonblack ~0.25–0.28, mean ~31–35) — skeptic correctly rejected “warned” claims without packet proof. |
| **Fix** | `fovea_crop_is_too_dark`: craft nonblack @ `FOVEA_CRAFT_LUMA_MIN=48` with min **0.35**, **or** mean Rec.601 luma &lt; `FOVEA_CROP_MEAN_LUMA_MIN=50`. `see_entity` pushes `fovea_dark: crop craft_nonblack=… mean_luma=…`. |
| **Unit** | `fovea_crop_is_too_dark_on_shadow_fixture` — mixed shadow PNG that would pass old thr30 min=0.05 but **must** trip new gate; bright craft must not. |
| **Live packet proof** | After MCP rebuild: `see entity --name OreCrusher --profile iron-feud` → warnings: `fovea_dark: crop craft_nonblack=0.254 (min 0.35 @ luma>=48) mean_luma=34.6 (min 50) for 'OreCrusher' @ (1949,801) …`. Packet + crop under goal `{SCRATCH}/entity_OreCrusher_packet.json` and `entity_OreCrusher_crop.png`. |

## S4 CD env_2d

| | |
|--|--|
| **Probe** | Prior workflows: black_frame / HUD-only env_2d fulls (timing). |
| **Fix** | Wait for Nebula/WarpGate/Station/Player (up to 4s); if full nonblack &lt; `FULL_FRAME_NONBLACK_MIN`, sleep 400ms and **recapture once**; warn `env_2d_dark` if still dark. |
| **Evidence** | Live workflow env_2d packs after fix. |

## S5 CI / non-silent

| | |
|--|--|
| **Doc** | [AGENT_SIGHT_CI_MATRIX.md](AGENT_SIGHT_CI_MATRIX.md) |
| **Policy** | Headless: always `cargo test -p specula -p specula-brp`. Live bar: **`/agent-sight-dogfood` mode=full**. No fake headless screenshot CI. |

## E6 Full workflow

**agent-sight-dogfood-4** — overall **`passed: true`**, skeptic **green** (~6m) — pre–S3-craft-gate re-proof.

**agent-sight-dogfood-5** (after craft-luma `fovea_dark` + MCP install) — overall **`passed: true`**, skeptic **green** (~5.5m).

| Phase | Result |
|-------|--------|
| Gates | PASS · 103 tests (57+ brp) · mcp_rebuilt=false (skip_install; install done pre-run) |
| CD | Player · CargoPod · magenta 0 · env_2d_dark warned honestly when space-black |
| IF | StrategyCamera · OreCrusher @ (1949,801) topdown3d · packs [mutated][restored] · multi-view differs · **fovea_dark craft_nonblack=0.254 mean~35** |
| Skeptic | green · 18 claims · failures [] · confirmed packet `fovea_dark` + no craft identity from crop |

Evidence: `{SCRATCH}/workflow-soft-full-report.md`, `entity_OreCrusher_packet.json` (warnings include `fovea_dark`), `entity_OreCrusher_crop.png`, `gbr-tests.log`, `mcp-surface.log`, commit `497c792`.

## Residual

- Games without BRP ChildOf still use string/co-located demotion (merge path no-ops).
- True multi-view still needs game-side secondary active camera for free parallax without strategy-cam nudge.
- Dark fovea under shadow is **warned** (`fovea_dark` with craft luma + mean) — craft identity not claimed from unreadable crops. Improving IF lighting/silhouette is game-side, not a false “craft ok”.
- CD env_2d may still warn env_2d_dark on pure-black space (gate vs soft nebulas); composition not overclaimed.
- GH unit CI ≠ live sight alone — full `/agent-sight-dogfood` `mode=full` remains the live bar ([AGENT_SIGHT_CI_MATRIX.md](AGENT_SIGHT_CI_MATRIX.md)).

Optional longer-horizon re-review register (not required for soft E0–E7 closeout): [AGENT_SIGHT_STILL_SOFT_PLAN.md](AGENT_SIGHT_STILL_SOFT_PLAN.md).
