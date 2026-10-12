---
name: bevy-agent-loop
description: >
  Live Bevy iteration with Specula MCP and BRP: doctor, launch, query, mutate,
  viewport capture, and fix loops. Use when debugging a running Bevy app, taking
  screenshots, using BRP, MCP tools, bevy_capture_viewport, or /bevy-agent-loop.
metadata:
  short-description: "MCP/BRP live verify loop for Bevy"
---

# Bevy agent loop (MCP + BRP)

You control a **running** Bevy app. Prefer evidence from **agent eyesight** (pixels you open and judge) over guessing.  
This is **not a Bevy editor** — eyesight is a sensory channel for the agent brain.

Plan: `docs/AGENT_EYESIGHT_PLAN.md` (schema `specula.sight/v1`).  
2D+3D sight: `docs/AGENT_SIGHT_2D3D_PLAN.md` + findings `docs/AGENT_SIGHT_2D3D_FINDINGS_2026-07-22.md`.  
Hardening (shipped): `docs/AGENT_SIGHT_HARDENING_PLAN.md`.  
Fidelity (shipped): `docs/AGENT_SIGHT_FIDELITY_PLAN.md` + findings `docs/AGENT_SIGHT_FIDELITY_FINDINGS_2026-07-23.md` — complex craft, transparent 2D BG, IF heightfield.  
Still soft (active residual): `docs/AGENT_SIGHT_STILL_SOFT_PLAN.md` — hierarchy, multi-view, craft-in-fovea, CD env pack, non-silent CI; closeout **mode=full** only.  
Workflow (project): `.grok/workflows/agent-sight-dogfood.rhai` — `/agent-sight-dogfood` or `/workflow agent-sight-dogfood` (**default mode=full**; gates + sequential CD/IF dogfood + packet–pixel skeptic).

## Pins

- BRP default port **15702**  
- App must enable features **`remote,capture`** and add `BrpExtrasPlugin` when `remote` is on  
- Prefer full **`bevy_brp_mcp`** when installed for hierarchy/watches/input  

## Eyesight rules (hard — baseline + 20/20 + 2D/3D)

Plan: `docs/AGENT_EYESIGHT_PLAN.md` + `docs/AGENT_EYESIGHT_20_20_PLAN.md` + `docs/AGENT_SIGHT_2D3D_PLAN.md`.

1. **Pixels are primary.** Query without opening captures is not eyesight.  
2. **Open every PNG** returned (`abs_path` if chat truncates image bytes).  
3. **Observation claims must cite capture paths** — what you *saw*, not taste lectures.  
4. **Taste/design stay human-owned** — build to their requirements; do not claim AI art director.  
5. **State gate (A0):** Iron Feud env claims need Playing (`IRON_FEUD_AUTO_PLAY=1`). Use `require_playing` / `wait_for_subjects`.  
6. **True fovea (A1):** `bevy_see_entity` with projection (`ortho2d` / `topdown3d`) — not center guess when Name+Transform exist.  
7. **Multi-view (A2):** env → `bevy_see_pack landscape|water` (game + alt view). 2D also `hud` / `env_2d`.  
8. **Motion (A3):** physics/feel → `bevy_see_motion` + stimulus note; stills-only strips (no livestream).  
9. **Clean subjects (A4):** default `subject_filter=gameplay_prefer` (new Names need score >0; demote OreCrystal / child mesh parts).  
10. **Diff (A5):** `save_baseline` / `compare_baseline` or `bevy_see_diff` after visual changes.  
11. **Black-frame warning:** empty window — dark space with sprites is not empty.  
12. **Default one-shot:** `bevy_see_verify` / `specula see verify --profile crystal-drift|iron-feud` first.  
13. **3D height:** landscape pack notes height_bands when TerrainFlat/Hill/Peak present; IF dogfood uses multi-band ground.  
14. **No shortcuts:** rebuild MCP (`cargo install --path crates/specula --force`) before treating dogfood captures as pass evidence.  
15. **Name onboarding:** every new dogfood Name must score `gameplay_subject_score > 0` (add stems to `GAMEPLAY_NAME_HINTS` / `DOGFOOD_NAME_STEMS` + unit test).  
16. **Baseline after visual change:** `save_baseline` then `compare_baseline` once per game when iterating assets/env.  
17. **Sequential BRP:** one game on port 15702 at a time.  
18. **Transparent 2D BG law:** CD sprites must be opaque subject + fully transparent background — **no purple/magenta square plates**; run `scripts/check_sprite_transparency.py` (true-magenta = 0).  
19. **Complex 3D craft:** IF drills/belts/machines multi-part silhouettes (not block soup); terrain randomized heightfield with placeable start pocket.  
20. **Entity fovea profile:** always pass `--profile iron-feud` (or `crystal-drift`) to `see entity` / `bevy_see_entity`. Without profile, IF uses ortho2d XY and aims at the wrong pixels after packs/restore.  
21. **Pack camera restore (T2):** landscape/water packs restore StrategyCamera after alt/side; notes may include `[restored]`.  
22. **Structural demotion (T1):** multiparts demoted via parent_entity / co-located heuristics + name fallback; protected top-level Names (OreCrusher, SolarFlareBuoy, …) stay.  
23. **ChildOf merge (S1):** `query_all_subjects` merges `bevy_ecs::hierarchy::ChildOf` from a second BRP query (AND multi-query returns empty).  
24. **fovea_dark (S3/SS3):** unreadable entity crop → warning (craft luma + mean; thr30-only is insufficient); do not claim craft from dark fovea.  
25. **env_2d (S4):** wait env subjects + one dark-frame retry; `env_2d_dark` if still black.  
26. **Live bar:** full `/agent-sight-dogfood` `mode=full` only (see `docs/AGENT_SIGHT_CI_MATRIX.md`); GH unit CI ≠ sight green forever.

## Standard loop (hard requirements)

1. **`bevy_env_check`** (or CLI `specula doctor`) if the host is unknown.  
2. **Launch — never block MCP on cold Bevy compile**  
   - **Cold first compile (no `target/debug/<package>`):** shell/background  
     `cargo run --features remote,capture` (or `cargo build` then run).  
   - **Warm binary exists:** MCP `bevy_launch_app` with `manifest_path` +  
     `features: "remote,capture"` and **`wait_secs: 0`** (default; returns immediately; may spawn the debug binary).  
   - **Always** next: step 3. Do **not** set high `wait_secs` on launch (host tool timeouts ~120s).  
3. **Wait** with MCP **`bevy_wait_brp`** (`timeout_secs` **180** cold / **30** warm) or CLI `specula brp wait --timeout-secs 180`.  
4. **`bevy_see_scene`** (or CLI `specula see scene --out-dir .`) — open packet captures + subjects.  
5. **Fovea / motion as needed:** `bevy_see_entity`, `bevy_see_region`, `bevy_see_motion`, `bevy_see_pack`.  
6. **Mutate** if needed (`bevy_brp_mutate`) then **re-see** (`bevy_see_diff` with baseline).  
7. **Fix code or assets** → rebuild/restart if needed → eyesight again.  
8. Stop when the glance test would pass for a non-author human.

### Dual launch / one BRP port

Default BRP port is **15702**. Only **one** app should bind that port. Dual `cargo run` is fine for “start A, stop A, start B” smoke tests — not two simultaneous instances on the same port. For two live apps, register distinct ports/targets (`bevy_register_target`) and change `BrpExtrasPlugin` / env port on the second.

## Tool map (specula MCP)

| Tool | Use |
|------|-----|
| `bevy_env_check` | Host ready? |
| `bevy_launch_app` | Spawn game (`wait_secs=0` default; non-blocking) |
| `bevy_wait_brp` | Poll until BRP ready (use after launch) |
| `bevy_register_target` / `bevy_list_targets` | Multi-instance |
| `bevy_brp_discover` | Method list (`rpc.discover`) |
| `bevy_brp_query` | Read components |
| `bevy_brp_mutate` | Write field |
| `bevy_brp_call` | Arbitrary BRP / `brp_extras/*` |
| `bevy_capture_viewport` | Raw PNG as MCP image (`brp_extras/screenshot`) |
| `bevy_see_scene` | E0 eyesight packet (full + subjects + abs_path) |
| `bevy_see_entity` | E1 fovea crop for named entity |
| `bevy_see_region` | E1 pixel-rect crop (landscape / water / HUD) |
| `bevy_see_motion` | E2 temporal strip (+ optional keys) |
| `bevy_see_diff` | E3 baseline vs after (+ abs-diff) |
| `bevy_see_verify` | One-shot full + ranked primary fovea (+zoom) — prefer first |
| `bevy_see_pack` | Multi-view: entity_craft / landscape / water / physics_jump / lighting / diagnostic / **hud** / **env_2d** |
| `bevy_brp_mcp_status` | Upstream MCP install help |

CLI mirrors: `specula see scene|verify|entity|region|motion|diff|pack`.  
Profiles: `--profile crystal-drift` (2D) | `iron-feud` (3D topdown, require_playing).

## Exact BRP method names (do not invent)

| Method | Use |
|--------|-----|
| `rpc.discover` | List methods |
| `world.query` | Query |
| `world.mutate_components` | Mutate |
| `brp_extras/screenshot` | Screenshot path (**not** `bevy_brp_extras/…`) |

## Capture quality rules

- Window must be **visible** (not minimized); black frames are often occlusion, not “empty scene”.  
- Ensure lights + non-black clear color in 3D.  
- After art changes, confirm texture path and that the handle finished loading (Loading state).  
- Compare before/after captures when iterating.

## Demo DoD captures

When finishing a short demo (`docs/GAME_DOD.md` / skill `bevy-demo-game`), capture at least:

1. **Main menu**  
2. **Mid-play** with objective/HUD visible  
3. **Victory or GameOver** end screen  

Movement-only scenes are not complete demos.
## BRP hygiene

- **Aliases** (preferred): `Name`, `Transform`, `GlobalTransform` — MCP expands to Bevy 0.19 FQNs.  
- Or pass FQNs with `::` (e.g. `bevy_ecs::name::Name`). Default query is Name + Transform.  
- Prefer entities with **`Name`** for human-readable queries.  
- Mutate small fields first (e.g. `component: "Transform"`, `path: "translation"`) to prove the pipe.  
- Custom components need Reflect + BRP registration to be mutable remotely.

### Optional: movement via keys (after BRP ready)

1. Query Player Transform (alias `Transform`).  
2. `bevy_brp_discover` / check `brp_extras/send_keys` params.  
3. `bevy_brp_call` method `brp_extras/send_keys` (discover-first params).  
4. Re-query Transform.  
5. If send_keys fails: `bevy_brp_mutate` translation instead (portfolio closed-loop still works).

## When to use bevy_brp_mcp

Install/register when you need:

- Entity hierarchy browsing  
- Watches / richer world tools  
- Keyboard/mouse injection  
- Advanced launch discovery  

Same port; complementary to specula’s focused tools.

## Failure playbook

| Symptom | Check |
|---------|--------|
| Connection refused | App not running / missing `remote` / wrong port; call `bevy_wait_brp` |
| Launch MCP timeout | `wait_secs=0` + `bevy_wait_brp`; cold = shell `cargo run`; warm = debug binary |
| Black screenshot | Minimized window; no lights; camera wrong way |
| Empty query / still Loading | Wrong type path; asset root (see ASSET_CONVENTIONS); wait for fail-forward timeout |
| Mutate no-op | Path wrong; type not Reflect; wrong entity id |
| B0001 panic | Overlapping mut queries — ParamSet / Without / split systems |
| Compile forever | First Bevy build; shell cargo run; doctor guidance; dev opt profiles |

## Chain with other skills

- Structure: **`bevy-production`**  
- Slice: **`bevy-2d-game`** / **`bevy-3d-game`**  
- Art defects in capture: regenerate with **`game-asset-core`** (keyable bg, scale)

## References

- `references/loop-checklist.md`  
- `references/eyesight-packs.md` (2D/3D packs + profiles)  
