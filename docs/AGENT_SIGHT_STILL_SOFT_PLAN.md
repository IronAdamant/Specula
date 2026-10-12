# Agent Sight Still Soft Plan — re-review residual soft edges

**Status:** planned (not started)  
**Audience:** implementing agent under `/goal`; human may be away for a long session  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**Focus:** Soft plan E0–E7 **shipped**, but the five soft edges are **not closed forever**. Re-**review** and **investigate** each with fresh BRP/PNG evidence; fix platform-owned gaps (including the S3 skeptic hole on dark fovea); close only with **`/agent-sight-dogfood` `mode=full`**.

### Why this plan exists

Soft findings claimed E0–E7 complete and workflow green, yet residual honesty gaps remain:

| Soft edge | Soft plan left | Still soft (re-open) |
|-----------|----------------|----------------------|
| **Hierarchy** | ChildOf second-query merge | Multi-component AND still empty; games without ChildOf stay string/co-located only — prove merge on live IF again; document ownership |
| **Multi-view** | Nudge chain + optional named side cam | Still StrategyCamera mutates; true second active camera not reliable via BRP — re-probe; no silent multi-angle claim |
| **Craft in fovea** | `fovea_dark` @ thr=30 min=0.05 | **Skeptic gap:** deep-shadow OreCrusher (~mean 31, thr30 frac ~0.28) can **omit** warning; unit test was theater. WIP craft-luma + mean gate may be in tree — must ship with real fixture test + live packet proof |
| **CD env pack** | Wait + one recapture + `env_2d_dark` | Can still be space-black / HUD-only; timing vs composition — re-capture and open PNGs |
| **Not silent CI forever** | CI matrix doc + cargo test in GH | Unit CI ≠ live sight; full dogfood still manual — matrix must stay enforced as **required** bar, not optional folklore |

### Long-session / no-shortcuts mandate

- Take as long as needed; **correctness over speed**.  
- **Investigate before bulk coding:** each soft edge gets written findings (cause, BRP/PNG evidence, options) then a minimal fix **or** explicit defer with reason.  
- **No shortcuts:** no synthetic PNGs as sole pass proof for live claims; no string-only hierarchy pretence; no fake multi-view; no “warned” without a real packet field; no gates-only closeout.  
- Rebuild MCP (`cargo install --path crates/specula --force`) **before** final full-workflow evidence if platform code changed.  
- Taste/design remain **human-owned** (fovea craft readability = **pixel/lighting honesty**, not beauty).  
- Sequential BRP **15702** only.

### Parent / prior work

| Doc / artifact | Role |
|----------------|------|
| [AGENT_SIGHT_SOFT_PLAN.md](AGENT_SIGHT_SOFT_PLAN.md) | E0–E7 first soft pass (shipped) |
| [AGENT_SIGHT_SOFT_FINDINGS_2026-07-25.md](AGENT_SIGHT_SOFT_FINDINGS_2026-07-25.md) | Residuals this plan re-opens |
| [AGENT_SIGHT_TRUST_PLAN.md](AGENT_SIGHT_TRUST_PLAN.md) | Structural demotion, pack restore, multi-view |
| [AGENT_SIGHT_CI_MATRIX.md](AGENT_SIGHT_CI_MATRIX.md) | Headless vs full workflow bar |
| `.grok/workflows/agent-sight-dogfood.rhai` | **Default `mode=full`** — required closeout bar |

### Dogfood trees

| Dimension | Path | Launch notes |
|-----------|------|----------------|
| **2D** | `/Users/aron/Documents/coding_projects/Crystal Drift` | Playing; `remote,capture` |
| **3D** | `/Users/aron/Documents/coding_projects/Iron Feud` | **`IRON_FEUD_AUTO_PLAY=1`**; `remote,capture` |

Pins: Bevy **0.19**, BRP **15702**.  
**Workflow closeout (mandatory):**

```text
/agent-sight-dogfood
# args — mode MUST be full (not gates-only). Workflow default is already "full".
{
  "mode": "full",
  "skip_install": true,   // false if SS1–SS5 just installed MCP
  "require_cd_name": "CargoPod",
  "require_if_name": "OreCrusher",
  "cd_path": "/Users/aron/Documents/coding_projects/Crystal Drift",
  "if_path": "/Users/aron/Documents/coding_projects/Iron Feud",
  "specula_root": "/Users/aron/Documents/coding_projects/Specula"
}
```

---

## 1. North star

> **Still-soft edges are either closed with durable proof or explicitly owned:** hierarchy prefers live BRP parent data when present; multi-view is useful or honestly `views_similar`; entity fovea is aim-correct **and** craft-judgeable (or packet **always** warns `fovea_dark` on shadow/empty crops — including the thr30≈0.28 / mean≈31 class); CD `env_2d` shows env or warns without overclaim; regression never goes silent — unit tests always, **full dogfood workflow** for every still-soft closeout and after eyesight changes.

### Soft register (must review + investigate again)

| ID | Soft edge | Review (what to re-check) | Investigate | Exit (fix or defer) |
|----|-----------|---------------------------|-------------|----------------------|
| **SS1** | **Hierarchy** | Live IF: ChildOf-only vs Name+Transform+ChildOf; OreCrusher multiparts demoted? | Does merge still fill `parent_entity`? Any BRP shape regression? | Parent demotion preferred when ids present; hybrid fallback documented + tested |
| **SS2** | **Multi-view** | Live IF landscape: game vs alt vs side-orbit vs dedicated-side hashes / `views_similar` | Spawn/switch active Camera3d via BRP still blocked? Named `AgentSightSideCamera` usable? | Stronger path **or** honest-only; unit placement tests stay green |
| **SS3** | **Craft in fovea** | OreCrusher `--profile iron-feud` crop open as image; packet `warnings` | Old gate miss (min 0.05 @ thr 30); craft luma + mean; aim coords | Readable craft **or** guaranteed `fovea_dark`; **non-theater** unit + live packet JSON |
| **SS4** | **CD env pack** | `see pack env_2d` full/horizon/center open | Timing, clear color, nebula alpha, crop rects, retry | Env on frame **or** honest `env_2d_dark`; no HUD-only as “env ok” |
| **SS5** | **Not silent CI forever** | GH `ci.yml` = cargo test only; matrix doc | What must stay headless vs full workflow? Drift of “recommended” → ignored | Matrix enforced in docs/skill; unit always CI; **full workflow required** for this plan + eyesight releases |

### Exclusions

| Excluded | Why |
|----------|-----|
| 60 FPS / livestream | Deferred |
| Human editor / gizmos UI | Out of scope |
| Auto taste / beauty scoring | Human-owned |
| New CD/IF features for their own sake | Only if needed to prove SS1–SS4 |
| Parallel dual BRP on one port | Sequential only |
| Fake headless screenshot CI without a window | Out of scope |

---

## 2. Execution order (hard)

```text
SS0  Baseline: cargo test; re-open soft findings + skeptic gap; note any dirty eyesight WIP
SS1  Review + investigate hierarchy (live BRP) → fix or defer
SS2  Review + investigate multi-view (live IF) → fix or defer
SS3  Review + investigate craft-in-fovea → ship honest fovea_dark + live packet proof
SS4  Review + investigate CD env_2d → fix or defer with open PNGs
SS5  CI / non-silent bar: reaffirm matrix; wire any missing unit coverage from SS1–SS3
SS6  Mandatory /agent-sight-dogfood mode=full (overall pass preferred; skeptic green)
SS7  Findings, PROGRESS, ROADMAP, AGENTS, skill; commit
```

**Final proof is SS6 full workflow — not gates-only.**  
Rebuild MCP before SS6 if SS1–SS5 changed platform code.

---

## 3. Phase SS0 — Baseline / re-open

- [ ] `cargo test -p specula -p specula-brp` PASS → `{SCRATCH}/gbr-tests-ss0.log`  
- [ ] Read soft findings residual + last workflow report; write `{SCRATCH}/still-soft-baseline.md` listing SS1–SS5 current symptoms (cite open PNGs / packets if available).  
- [ ] Inventory uncommitted eyesight work (if any): craft-luma / mean `fovea_dark` WIP must be finished under **SS3**, not left half-landed.  
- [ ] Confirm workflow defaults: `.grok/workflows/agent-sight-dogfood.rhai` has `let mode = "full";` (do not change default to gates).

**SS0 exit:** Tests green; still-soft register accepted; WIP owned by a phase.

---

## 4. Phase SS1 — Hierarchy (review + investigate)

### Review

- [ ] Re-read soft findings S1 and code: `merge_childof_parents`, `parent_map_from_childof_query`, structural demotion.  
- [ ] Confirm unit tests still cover bare `ChildOf` u64 merge + demotion with `parent_entity` set.

### Investigate (live Iron Feud, Playing)

- [ ] BRP `world.query` ChildOf-only sample → log parent map size.  
- [ ] BRP Name+Transform+ChildOf multi-component → expect 0 (AND) unless engine changed.  
- [ ] Sample OreCrusher + multiparts: after `query_all_subjects`, do parts have `parent_entity`? Are they demoted?  
- [ ] Write short finding: A (merge healthy) / B (partial) / C (broken or absent).

### Fix (if B or C and platform-owned)

- [ ] Repair merge / extractors; keep hybrid string fallback.  
- [ ] Do **not** demote protected top-level Names (Player, OreCrusher, …).  
- [ ] Unit tests for any new shape.

### Defer only with evidence

- [ ] Findings: “BRP omits ChildOf on this stack” or “game has no hierarchy” + hybrid remains.

**SS1 exit:** Investigation logged; code improved if feasible; tests PASS.

---

## 5. Phase SS2 — Multi-view (review + investigate)

### Review

- [ ] Document current chain: alt nudge → side-orbit → dedicated-side → optional `find_dedicated_view_camera` Names.  
- [ ] Unit tests for pure placement helpers still green.

### Investigate (live IF landscape pack)

- [ ] Capture game / alt / side hashes; note `views_similar` warnings.  
- [ ] Probe: spawn temporary Camera3d / set active camera via BRP — still unreliable for viewport capture?  
- [ ] Does scene already have a secondary camera Name we should prefer?

### Fix options (minimal)

- [ ] **Preferred:** use named dedicated side camera when present (no long StrategyCamera leave).  
- [ ] **Fallback:** keep nudge chain; always honest `views_similar`; never claim multi-angle when similar.  
- [ ] Optional pure-math nudge improvement only if measured gain.

**SS2 exit:** Stronger path or documented limit + tests; no silent fake multi-view.

---

## 6. Phase SS3 — Craft in fovea (review + investigate + skeptic gap)

### Review

- [ ] Soft findings S3 claimed `FOVEA_CROP_NONBLACK_MIN=0.05` @ thr 30 — **insufficient** for deep shadow.  
- [ ] Confirm aim path: `see entity --name OreCrusher --profile iron-feud` → topdown3d coords (~1949,801 class).

### Investigate

- [ ] Live IF: capture entity fovea; **open crop PNG**.  
- [ ] Measure craft_nonblack (stricter luma) + mean Rec.601 luma.  
- [ ] Confirm whether packet includes `fovea_dark` under **current** binary (stale MCP = fail investigate).

### Fix (required if gap still open)

- [ ] Gate: craft nonblack fraction **and/or** mean luma so thr30≈0.28 / mean≈31 **warns**.  
  - Suggested targets (tune only with fixtures): craft luma ≥ ~48; nonblack min ~0.35; mean min ~50.  
- [ ] `see_entity` warning text includes craft_nonblack + mean_luma + entity name + coords.  
- [ ] **Non-theater unit test:** fixture that passes old thr30 min=0.05 but trips new gate; bright craft must not trip.  
- [ ] Live: save entity packet JSON under `{SCRATCH}/eyesight/if-fovea/` proving `warnings` contains `fovea_dark` (or crop is truly craft-readable — open PNG).  
- [ ] Export helpers if CLI/MCP surface needs them (usually internal is enough).

**SS3 exit:** Aim correct + (readable craft **or** guaranteed dark warning with proof); cargo test covers shadow fixture.

---

## 7. Phase SS4 — CD env_2d pack (review + investigate)

### Review

- [ ] Soft E4: wait subjects + 400ms recapture + `env_2d_dark`.  
- [ ] Open last known pack_env_2d_full / horizon if available.

### Investigate (live Crystal Drift)

- [ ] `see pack env_2d` with profile crystal-drift; open full/horizon/center.  
- [ ] Subjects (Nebula*, WarpGate, Station, Player) vs pixel nonblack.  
- [ ] Root cause if dark: load timing, camera, nebula alpha, clear color, crop rects.

### Fix

- [ ] Prefer env visible on frame (wait/retry/crop) over warning-only.  
- [ ] Keep honest `env_2d_dark` when still empty after retry.  
- [ ] Live evidence `{SCRATCH}/eyesight/cd-env2d/`.

**SS4 exit:** Env on pack frames **or** documented limitation with open PNG + warning.

---

## 8. Phase SS5 — Not silent CI forever (review + investigate)

### Review

- [ ] [AGENT_SIGHT_CI_MATRIX.md](AGENT_SIGHT_CI_MATRIX.md) + `.github/workflows/ci.yml` (unit tests only — correct).  
- [ ] Workflow default `mode=full` in `agent-sight-dogfood.rhai`.

### Investigate / design

| Layer | What | Where |
|-------|------|--------|
| **Always (headless)** | `cargo test -p specula -p specula-brp` (+ env) | Local + GitHub CI |
| **GPU / manual or scheduled** | `/agent-sight-dogfood` **mode=full** | **Required** for this plan closeout; **required** after eyesight platform changes before “sight green” claims |
| **Not claimed** | Headless fake screenshot CI without Bevy window | Out of scope |

### Fix / wire

- [ ] Ensure SS1–SS3 pure helpers are covered by unit tests (so CI is not silent on ranking/fovea gates).  
- [ ] Update CI matrix if still-soft changes policy wording.  
- [ ] Skill / ROADMAP / AGENTS: **full workflow is the live sight bar** (not “optional nice-to-have”).  
- [ ] Do **not** switch workflow default to gates; gates-only is debug-only.  
- [ ] Optional: note in CI job comment or matrix that full dogfood is human/GPU.

**SS5 exit:** Matrix clear; unit CI green; full workflow mandated at SS6.

---

## 9. Phase SS6 — Mandatory full workflow

**Required:** project workflow **full**, not gates-only. Default in rhai is already `mode = "full"`.

```text
/agent-sight-dogfood
```

Args as in the Dogfood trees section (`mode: "full"` explicit preferred).

### Pass criteria

- [ ] Gates PASS (`cargo test` green).  
- [ ] CD PASS: primary Player; CargoPod present; no magenta plates; env_2d honest.  
- [ ] IF PASS: Playing; primary not OreCrystal*; OreCrusher in subjects; entity fovea **topdown3d** aim; packs restored.  
- [ ] Entity packet honesty: dark shadow fovea **warns** `fovea_dark` when unreadable (skeptic may re-check).  
- [ ] Skeptic **green** (or only env-only residual documented with open PNG + not platform bug).  
- [ ] Overall `passed: true` preferred.  
- [ ] Report → `{SCRATCH}/workflow-still-soft-full-report.md` (or path logged).

If red: fix platform (or CD env) and re-run **full** until green or residual is explicitly environmental (GPU/window).

**SS6 exit:** Full workflow evidence for this version.

---

## 10. Phase SS7 — Docs and closeout

- [ ] Flip checkboxes in this file.  
- [ ] Write `docs/AGENT_SIGHT_STILL_SOFT_FINDINGS_YYYY-MM-DD.md` (SS1–SS5 investigation, fixes, residual).  
- [ ] Update PROGRESS, ROADMAP, AGENTS, bevy-agent-loop skill, CI matrix if needed.  
- [ ] Soft findings residual pointer: “superseded / extended by still-soft findings”.  
- [ ] Commit Specula (+ CD/IF only if trees changed). No force-push.

---

## 11. Success metrics

1. Each of SS1–SS5 has **fresh investigation notes** in findings (not copy-paste of soft-only).  
2. Hierarchy: parent path verified live or defer documented.  
3. Multi-view: honest similar or measured multi-angle; no silent fake.  
4. Fovea craft: shadow class crops warn **or** are readable; unit fixture is real; live packet proof.  
5. CD env_2d: env visible or honest dark with open PNGs.  
6. CI matrix still enforced; unit tests always green; **no claim that GH CI alone = sight green**.  
7. **`agent-sight-dogfood` mode=full PASS** (or env-only residual accepted in findings).  
8. `cargo test -p specula -p specula-brp` PASS; MCP current if code changed.

---

## 12. Risks

| Risk | Mitigation |
|------|------------|
| ChildOf still AND-empty | Keep second-query merge; hybrid fallback |
| Cannot activate second camera via BRP | Nudge + honest views_similar |
| Dark fovea is pure game lighting | Warning + optional capture-only lift; no taste scorer |
| env_2d flake is timing | Wait + recapture; re-run full |
| Skeptic rejects “theater” again | Live packet JSON + open PNG required for SS3 |
| GPU unavailable for SS6 | Not “complete”; retry; unit tests alone insufficient |
| Soft plan green lulls agents into skip | This plan **re-opens**; SS6 mandatory full |

---

## 13. `/goal` paste template

```text
Execute docs/AGENT_SIGHT_STILL_SOFT_PLAN.md to completion (SS0 → SS7).
I will be away — take as long as needed; NO shortcuts.

Re-review and investigate each still-soft edge:
  SS1 hierarchy, SS2 multi-view, SS3 craft in fovea (skeptic gap),
  SS4 CD env pack, SS5 not silent CI forever.
Fix platform-owned gaps; document the rest.

Order: SS0 baseline → SS1 hierarchy → SS2 multi-view → SS3 fovea craft
→ SS4 CD env → SS5 CI matrix → SS6 mandatory /agent-sight-dogfood mode=full
→ SS7 findings + docs + commit.

Workflow full is REQUIRED for closeout (not gates-only).
Default in agent-sight-dogfood.rhai is already mode=full — keep it.
  mode=full, CD + IF sequential, skeptic green preferred.
Dogfood paths:
  2D: /Users/aron/Documents/coding_projects/Crystal Drift
  3D: /Users/aron/Documents/coding_projects/Iron Feud
Bevy 0.19, remote,capture, BRP 15702. Iron Feud: IRON_FEUD_AUTO_PLAY=1.
Entity: --profile iron-feud|crystal-drift.
Exclude: livestream, editor, taste scoring.
Rebuild MCP after platform changes. Taste/design human-owned.
```

---

## 14. Document history

| Date | Change |
|------|--------|
| 2026-07-25 | Initial still-soft plan: re-open S1–S5 as SS1–SS5; SS3 skeptic gap; SS6 full workflow mandatory |
