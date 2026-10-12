# Agent Sight Soft Edges Plan — hierarchy, multi-view, fovea craft, CD env packs, CI

**Status:** complete (E0–E7 shipped 2026-07-25) — residual soft re-opened in [AGENT_SIGHT_STILL_SOFT_PLAN.md](AGENT_SIGHT_STILL_SOFT_PLAN.md)  

**Audience:** implementing agent under `/goal`; human may be away for a long session  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**Focus:** **review and investigate** residual soft spots after Trust T0–T6, then fix what investigation proves is platform-owned. Close with a **mandatory full** [`agent-sight-dogfood`](../.grok/workflows/agent-sight-dogfood.rhai) run (`mode=full`) so the version is proven green end-to-end.

### Long-session / no-shortcuts mandate

- Take as long as needed; **correctness over speed**.  
- **Investigate before bulk coding:** each soft area gets a short written finding (cause, BRP evidence, options) then a minimal fix or an explicit defer with reason.  
- **No shortcuts:** no synthetic PNGs as pass proof; no “string-only” hierarchy pretend; no fake multi-view; no skip of live verify when GPU/window is available.  
- Rebuild MCP (`cargo install --path crates/specula --force`) **before** final workflow evidence if platform code changed.  
- Taste/design remain **human-owned** (fovea craft readability is **pixel/lighting honesty**, not beauty scoring).  
- Sequential BRP **15702** only.

### Parent / prior work (shipped)

| Doc / artifact | Role |
|----------------|------|
| [AGENT_SIGHT_TRUST_PLAN.md](AGENT_SIGHT_TRUST_PLAN.md) | T0–T6 structural demotion, pack restore, dedicated-side nudge |
| [AGENT_SIGHT_TRUST_FINDINGS_2026-07-25.md](AGENT_SIGHT_TRUST_FINDINGS_2026-07-25.md) | Residuals this plan opens |
| [AGENT_SIGHT_FIDELITY_PLAN.md](AGENT_SIGHT_FIDELITY_PLAN.md) | Craft fidelity, stems, OreCrusher fovea aim fix |
| `.grok/workflows/agent-sight-dogfood.rhai` | Gates + sequential CD/IF + skeptic — **required full bar for this plan** |

### Dogfood trees

| Dimension | Path | Launch notes |
|-----------|------|----------------|
| **2D** | `/Users/aron/Documents/coding_projects/Crystal Drift` | Playing; `remote,capture` |
| **3D** | `/Users/aron/Documents/coding_projects/Iron Feud` | **`IRON_FEUD_AUTO_PLAY=1`**; `remote,capture` |

Pins: Bevy **0.19**, BRP **15702**. Workflow: **`/agent-sight-dogfood` with `mode=full`** (not gates-only for closeout).

---

## 1. North star

> **Soft edges are either fixed with durable tests or explicitly owned:** BRP hierarchy is real when the engine exposes it; multi-view uses a true secondary view path when nudges fail; entity fovea crops are **aimed and lit enough to judge craft** (or packet warns “dark/unreadable”); CD `env_2d` packs show env, not black/HUD-only voids; regression is **repeatable** via unit tests + full dogfood workflow so sight does not silently rot.

### Soft register (must review + investigate)

| ID | Soft edge | Symptom today | Investigate | Exit (fix or defer) |
|----|-----------|---------------|-------------|----------------------|
| **S1** | **Hierarchy** | Demotion still needs string/co-located fallback when BRP omits parent | Live IF BRP: which parent/ChildOf components reflect? Query shape? | Prefer BRP parent demotion; fallback documented + tested |
| **S2** | **Multi-view** | Still StrategyCamera translation nudges; high-Y parallax weak | Can we spawn temp Camera3d via BRP? Or second camera entity already in scene? | Dedicated side camera path **or** stronger honest-only path with tests |
| **S3** | **Craft in fovea** | Aim can be correct (e.g. OreCrusher 1949,801) but crop deep shadow / empty green | Lighting, exposure, half-extent, camera height after restore; open PNGs | Readable craft OR explicit `fovea_unreadable` / dark warning; unit or live gate |
| **S4** | **CD env pack** | `env_2d` full/horizon sometimes black / HUD-only with `black_frame_warning` | Clear color, camera, nebula alpha, pack crop rects, timing after load | Pack frames show env subjects; nonblack gate sensible; tests where pure |
| **S5** | **Not silent CI forever** | Full trust needs GPU + agents; unit tests alone miss live regressions | What can run headless vs must be workflow/manual? | Document CI matrix; wire unit gates to `cargo test`; full workflow as **required** release/dogfood bar |

### Exclusions

| Excluded | Why |
|----------|-----|
| 60 FPS / livestream | Deferred |
| Human editor / gizmos UI | Out of scope |
| Auto taste / beauty scoring | Human-owned |
| New CD/IF features for their own sake | Only if needed to prove S1–S4 |
| Parallel dual BRP on one port | Sequential only |

---

## 2. Execution order (hard)

```text
E0  Baseline cargo test + document current soft evidence from prior findings/workflow reports
E1  Investigate + fix S1 hierarchy (BRP parent path)
E2  Investigate + fix S2 multi-view (dedicated camera or honest upgrade)
E3  Investigate + fix S3 fovea craft readability
E4  Investigate + fix S4 CD env_2d pack reliability
E5  S5 CI / non-silent regression matrix + workflow full wiring
E6  Mandatory /agent-sight-dogfood mode=full (overall pass preferred; skeptic green)
E7  Findings, PROGRESS, ROADMAP, AGENTS, skill; commit
```

**Final proof is E6 full workflow — not gates-only.**  
Rebuild MCP before E6 if E1–E5 changed platform code.

---

## 3. Phase E0 — Baseline

- [x] `cargo test -p specula -p specula-brp` PASS → `{SCRATCH}/gbr-tests-e0.log`  
- [x] Summarize soft evidence from Trust findings + last green/red dogfood notes into `{SCRATCH}/soft-baseline.md` (or findings draft)  
- [x] Confirm workflow path exists: `.grok/workflows/agent-sight-dogfood.rhai`

**E0 exit:** Tests green; soft register accepted as investigation list.

---

## 4. Phase E1 — Hierarchy (S1)

### Investigate

- [x] On live Iron Feud (Playing): BRP `world.query` for Name+Transform+**ChildOf/Parent** (try Bevy 0.19 FQNs and aliases).  
- [x] Log raw sample of OreCrusher children vs parent ids → `{SCRATCH}/hierarchy-brp.json` (or .log).  
- [x] Decide: (A) parent query works and demotion can prefer it; (B) partial; (C) not reflected → document.

### Fix (if A or B)

- [x] Expand `query_all_subjects` / extractors so parent_entity fills when present.  
- [x] Prefer structural parent demotion over string PARTS when parent known.  
- [x] Unit tests with synthetic subjects (parent_entity set) — already have structural test; extend with live-shape fixtures from BRP JSON if useful.  
- [x] Do **not** demote protected top-level Names.

### Defer only if C

- [x] Findings section: “BRP does not expose ChildOf on this stack” + keep hybrid fallback; list next engine/plugin step.

**E1 exit:** Investigation logged; code improved if feasible; tests PASS.

---

## 5. Phase E2 — Multi-view (S2)

### Investigate

- [x] Document current path: alt nudge → side-orbit → dedicated-side (all StrategyCamera mutates).  
- [x] Probe BRP: can we spawn a temporary Camera3d / copy StrategyCamera components?  
- [x] Measure: high-Y strategy cam — how often dedicated-side still `views_similar` (live IF landscape).

### Fix options (pick minimal that works)

- [x] **Preferred:** temporary side camera entity for pack alt/side, then despawn; game camera never leaves game pose for longer than one capture (still restore StrategyCamera if nudged).  
- [x] **Fallback:** improve dedicated-side pure math + always honest warnings; no multi-angle claim when similar.  
- [x] Unit tests for pure placement helpers.  
- [x] Live: landscape game hash ≠ alt **or** honest `views_similar` only.

**E2 exit:** Stronger path or documented limit + tests; no silent fake multi-view.

---

## 6. Phase E3 — Craft in fovea (S3)

### Investigate

- [x] Live IF: `see entity --name OreCrusher --profile iron-feud` after pack restore.  
- [x] Open crop PNG: is miss **aim** (coords) or **readability** (shadow/black)?  
- [x] Check half-extent, camera Y after restore, lights, material; optional temporary lift/nudge **only for fovea capture** with restore after.

### Fix options

- [x] If aim wrong: fix projection/profile (should already be fixed — regression test must stay).  
- [x] If dark/unreadable:  
  - [x] Larger half / tall-entity already inflated — retune IF entity half if needed.  
  - [x] Optional fovea capture path: brief camera or exposure note; **or** packet warning `fovea_dark` / low nonblack on crop.  
  - [x] Do not claim craft identity from empty/black fovea.  
- [x] Unit or pure gate: crop nonblack fraction helper reuse for fovea crops in see_entity warnings.  
- [x] Live open PNG evidence under `{SCRATCH}/eyesight/if-fovea/`.

**E3 exit:** Aim correct + (readable craft OR explicit unreadable warning); no false “craft ok” on black crops.

---

## 7. Phase E4 — CD env_2d pack (S4)

### Investigate

- [x] Live CD: `see pack env_2d` with profile crystal-drift; open full/horizon/center.  
- [x] Compare subject list (Nebula*, WarpGate, Station) vs pixel nonblack.  
- [x] Root cause candidates: load timing, camera scale, nebula alpha, SpaceBackdrop z-order, pack crop rects, clear color.

### Fix

- [x] Code and/or CD spawn/timing so env is on frame when pack captures (wait subjects / wait frames).  
- [x] Adjust env_2d crops if rects miss env.  
- [x] Soft nonblack / black_frame honesty: warn when env pack is empty; prefer fix over warning-only.  
- [x] Live evidence `{SCRATCH}/eyesight/cd-env2d/`.  
- [x] Optional pure tests for region presets if math-related.

**E4 exit:** env_2d frames show env (not HUD-only black); or documented game-side limitation with open PNG proof.

---

## 8. Phase E5 — Non-silent CI / regression bar (S5)

### Investigate / design

- [x] Split matrix:

| Layer | What | Where |
|-------|------|--------|
| **Always (headless)** | `cargo test -p specula -p specula-brp` | Local + CI |
| **GPU/manual or scheduled** | `/agent-sight-dogfood` **mode=full** | Required for this plan closeout; recommend for releases |
| **Not claimed** | Headless “screenshot CI” without window | Out of scope unless host has xvfb-class setup |

### Fix / wire

- [x] Ensure unit tests cover S1–S3 pure helpers landed in E1–E3.  
- [x] Document in skill + ROADMAP: **full workflow is the live sight bar**.  
- [x] Workflow args default / docs emphasize `mode=full` for soft-edge verification.  
- [x] Optional: CI job runs unit tests only; README note that full dogfood needs GPU.  
- [x] Capture `{SCRATCH}/ci-matrix.md` summary.

**E5 exit:** Clear “what is silent vs not” matrix; unit tests green; full workflow mandated at E6.

---

## 9. Phase E6 — Mandatory full workflow

**Required:** run project workflow **full**, not gates-only.

```text
/agent-sight-dogfood
# args:
{
  "mode": "full",
  "skip_install": true,   // false if E1–E5 just installed MCP
  "require_cd_name": "CargoPod",
  "require_if_name": "OreCrusher",
  "cd_path": "/Users/aron/Documents/coding_projects/Crystal Drift",
  "if_path": "/Users/aron/Documents/coding_projects/Iron Feud",
  "specula_root": "/Users/aron/Documents/coding_projects/Specula"
}
```

### Pass criteria

- [x] Gates PASS (`cargo test` green).  
- [x] CD PASS: primary Player; CargoPod present; no magenta plates.  
- [x] IF PASS: Playing; primary not OreCrystal*; OreCrusher in subjects; entity fovea with **topdown3d** aim; packs restored.  
- [x] Skeptic **green** (or only env-only residual documented with open PNG + not platform bug).  
- [x] Overall `passed: true` preferred.  
- [x] Report copied to `{SCRATCH}/workflow-soft-full-report.md` (or path logged).

If red: fix platform (or CD env) and re-run full until green or residual is explicitly environmental (GPU/window).

**E6 exit:** Full workflow evidence for this version.

---

## 10. Phase E7 — Docs and closeout

- [x] Flip checkboxes in this file.  
- [x] Write `docs/AGENT_SIGHT_SOFT_FINDINGS_YYYY-MM-DD.md` (S1–S5 investigation results, fixes, residual).  
- [x] Update PROGRESS, ROADMAP, AGENTS, bevy-agent-loop skill.  
- [x] Commit Specula (+ CD/IF only if trees changed). No force-push.

---

## 11. Success metrics

1. Each of S1–S5 has **investigation notes** in findings (not skipped).  
2. Hierarchy: parent path used when BRP allows; tests cover structural demotion.  
3. Multi-view: dedicated camera path **or** documented nudge-only limit + honest similar.  
4. Fovea craft: aim regression stays green; dark fovea warned or improved.  
5. CD env_2d: env visible on pack frames or documented with evidence.  
6. CI matrix written; unit tests always green.  
7. **`agent-sight-dogfood` mode=full PASS** (or env-only residual accepted in findings).  
8. `cargo test -p specula -p specula-brp` PASS; MCP current if code changed.

---

## 12. Risks

| Risk | Mitigation |
|------|------------|
| ChildOf never on BRP | Hybrid fallback; findings defer; don’t block E6 on perfect hierarchy |
| Cannot spawn cameras via BRP | Keep dedicated-side nudge; honest views_similar |
| Dark fovea is game lighting | Warning + optional capture-only nudge; no taste scorer |
| env_2d flake is timing | Wait subjects/frames; re-capture once |
| GPU unavailable for E6 | Capture failure under `{SCRATCH}`; unit tests + prior green not enough for “complete” without re-try |

---

## 13. `/goal` paste template

```text
Execute docs/AGENT_SIGHT_SOFT_PLAN.md to completion (E0 → E7).
I will be away — take as long as needed; NO shortcuts.
Investigate each soft edge (hierarchy, multi-view, fovea craft, CD env_2d, CI matrix),
fix what is platform-owned, document the rest.

Order: E0 baseline → E1 hierarchy → E2 multi-view → E3 fovea craft → E4 CD env pack
→ E5 CI/regression matrix → E6 mandatory /agent-sight-dogfood mode=full
→ E7 findings + docs + commit.

Workflow full is required for closeout (not gates-only):
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
| 2026-07-25 | S3 skeptic gap closed: craft luma + mean `fovea_dark`; live OreCrusher packet proof; MCP reinstall |
| 2026-07-25 | E0–E7 complete: S1 ChildOf merge, S3 fovea_dark, S4 env_2d retry, full workflow PASS |
| 2026-07-25 | Initial soft-edges plan: S1–S5 review/investigate; E6 full workflow mandatory |
