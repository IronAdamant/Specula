# Agent Sight Trust Plan — structural ranking, pack camera honesty, multi-view, regression

**Status:** complete (T0–T6 shipped 2026-07-25)  
**Audience:** implementing agent under `/goal`; human may be away for a long session  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**Focus:** make agent sight **trustworthy for factory loops** — subject lists that survive multiparts without string-table sprawl; landscape packs that **always restore** the game camera; multi-view that is **useful or honestly similar**; durable **packet↔pixel** regression via unit tests + the project workflow.

### Long-session / no-shortcuts mandate

- Take as long as needed; **correctness over speed**.  
- **No shortcuts:** no stub demotion tables that only list today’s Names; no fake multi-view; no synthetic PNGs as pass proof; no skip of live verify when GPU/window is available.  
- Rebuild MCP (`cargo install --path crates/specula --force`) **before** treating dogfood captures as pass evidence whenever Specula sight code changes.  
- Taste/design remain **human-owned**; agent **sees and proves** gates.  
- Sequential BRP **15702** only (one game at a time).

### Parent / prior work (shipped)

| Doc / artifact | Role |
|----------------|------|
| [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) | V0–V6 eyes open |
| [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md) | A0–A8 acuity |
| [AGENT_SIGHT_NEXT_PLAN.md](AGENT_SIGHT_NEXT_PLAN.md) | S0–S4 ranking, profiles, verify |
| [AGENT_SIGHT_2D3D_PLAN.md](AGENT_SIGHT_2D3D_PLAN.md) | D0–D5 packs, height bands |
| [AGENT_SIGHT_DEBT_PLAN.md](AGENT_SIGHT_DEBT_PLAN.md) | R0–R4 residual acuity |
| [AGENT_SIGHT_HARDENING_PLAN.md](AGENT_SIGHT_HARDENING_PLAN.md) | H0–H6 pixel gates, side-orbit, craft |
| [AGENT_SIGHT_FIDELITY_PLAN.md](AGENT_SIGHT_FIDELITY_PLAN.md) | F0–F6 craft fidelity + stems/scoring |
| `see entity --profile iron-feud` | Fovea XZ aim fix (shipped 2026-07-25) — **T0 verifies** |
| `.grok/workflows/agent-sight-dogfood.rhai` | Gates + sequential CD/IF + skeptic |

### Dogfood trees (required for T5)

| Dimension | Path | Launch notes |
|-----------|------|----------------|
| **2D** | `/Users/aron/Documents/coding_projects/Crystal Drift` | Playing by default; features `remote,capture` |
| **3D** | `/Users/aron/Documents/coding_projects/Iron Feud` | **`IRON_FEUD_AUTO_PLAY=1`**; features `remote,capture` |

Pins: Bevy **0.19**, BRP **15702**, `remote,capture`. Skills: `bevy-agent-loop`.  
Workflow: `/agent-sight-dogfood` (`mode=full`, `skip_install` per policy after install).

---

## 1. North star

> **Agent sight stays honest under craft density and camera mutation:** multiparts do not steal subject slots without a hierarchy/local-space rule; `see_pack` landscape/water never leave StrategyCamera (or MainCamera) on an alt pose after the pack; multi-view either differs perceptually or reports `views_similar` honestly; entity fovea uses the correct projection profile; regression is automatic enough that ranking/pack/fovea bugs fail tests or the dogfood workflow before a human notices.

### Map: leverage items + execution order

| User order | Leverage item | Plan phase |
|------------|---------------|------------|
| *(shipped)* | **L3** Entity fovea always profile-aware | **T0** verify + keep green |
| **3** | **L1** Structural subject demotion (not only string tables) | **T1** |
| **4** | Camera restore inside `see_pack` | **T2** |
| **5** / **L2** | Stronger multi-view (dedicated side cam or equivalent) | **T3** |
| **L4** | Packet↔pixel regression (workflow + durable tests) | **T4** (+ **T5** live proof) |

### Exclusions (do not implement)

| Excluded | Why |
|----------|-----|
| 60 FPS / livestream / continuous video | Deferred; stills + short strips only |
| Human editor / hierarchy UI / gizmos | Out of scope (BRP hierarchy *read* for demotion is OK) |
| Full unlit material suite | Out of scope |
| Auto taste / beauty scoring | Human-owned |
| New CD/IF craft features (unless needed to prove ranking) | Trust plan is **platform**; craft is optional proof only |
| Parallel dual-game BRP on one port | Sequential only |

---

## 2. Problem split (why this plan exists)

| Gap | Evidence | Trust answer |
|-----|----------|--------------|
| **Multipart Names crowd max_subjects** | ScrapPipe / SaddleAbut* / Crusher* parts; OreCrusher dropped until stem +120 | **T1:** demote by parent/child structure + local Transform, keep string table as fallback only |
| **String demotion is brittle** | `SolarFlareBuoy` almost demoted by `starts_with("Solar")`; every mesh rename risks regression | **T1:** structure first; names last |
| **Pack leaves camera on alt** | Landscape nudge; entity fovea misaimed even after “restore” was only prompt-level | **T2:** `see_pack` **always** restores camera Transform after alt/side paths |
| **Multi-view weak at high strategy Y** | Side-orbit helps but often still similar | **T3:** dedicated temporary side/orbit camera **or** larger honest path; never claim multi-angle if similar |
| **Entity fovea wrong without profile** | Ortho2d XY → (1303,776) vs topdown XZ → (1949,801) for OreCrusher | **T0:** already fixed; tests must stay green |
| **Regression is manual** | Dogfood workflow found bugs; not all covered by unit tests alone | **T4–T5:** pure tests + workflow full run + findings |

---

## 3. Execution order (hard)

```text
T0  Verify shipped fovea-profile fix + cargo test baseline
    → cargo test -p specula -p specula-brp
    → document T0 green

T1  Hierarchy / local-space child demotion (structural ranking)
    → pure helpers + unit tests; string PARTS table becomes fallback, not sole gate
    → cargo test; install --force

T2  see_pack always restores game camera after alt / side-orbit
    → unit-testable restore path; mutate back even on error paths where possible
    → cargo test; install --force

T3  Stronger multi-view (dedicated side camera or equivalent)
    → when alt similar, spawn/query temp camera or stronger side path
    → honest views_similar if still match; unit tests for placement
    → cargo test; install --force

T4  Packet↔pixel regression harness (code + workflow alignment)
    → unit tests: dogfood stems survive multipart flood; topdown fovea coords;
      pack restore invariant (mock or pure)
    → workflow prompts match code (profile on entity; no rely on restore-only-in-prompt)
    → optional: cargo test documents CI path

T5  Live sequential dogfood (CD then IF) — workflow preferred
    → /agent-sight-dogfood mode=full OR manual see verify + packs + entity --profile
    → skeptic-grade checks: OreCrusher/CargoPod in subjects; fovea note topdown3d XZ;
      landscape game≠alt or honest similar; camera pose game after pack
    → logs + PNGs under {SCRATCH}/eyesight/

T6  Docs closeout: checkboxes, PROGRESS, findings, ROADMAP, AGENTS, skill; commit
```

**Never treat T5 captures as pass evidence on a stale MCP if T1–T3 changed code.**

---

## 4. Phase T0 — Baseline: fovea profile (L3, already shipped)

**Goal:** Confirm entity fovea profile wiring stays green before structural work.

### Requirements

- [x] `cargo test -p specula -p specula-brp` PASS (include `topdown3d_projects_ore_crusher_off_center`, `match_subjects_prefers_exact_name`).  
- [x] CLI: `see entity` supports `--profile iron-feud|crystal-drift`.  
- [x] MCP: `bevy_see_entity` documents/accepts `profile`.  
- [x] Skill `bevy-agent-loop` states profile is required for IF entity fovea.  
- [x] Optional live (if GPU): IF `see entity --name OreCrusher --profile iron-feud` → screen_xy right of center (sx ≫ 1280 on 2560w), note contains `topdown3d`.

**T0 exit:** Tests green; no regression of fovea profile fix.

---

## 5. Phase T1 — Structural subject demotion (L1 / order 3)

**Goal:** Multiparts do not occupy subject slots when they are **children** of a Named parent or **local-space mesh parts**, without relying only on `is_child_mesh_part` string lists.

### Design constraints

- Prefer **pure, unit-testable** helpers.  
- Accept BRP data available today: Name + Transform (+ optional Parent / GlobalTransform if queryable without new Bevy plugins).  
- **Fallback:** keep/extend string demotion for known parts when hierarchy is unavailable.  
- **Never demote** top-level dogfood Names (`OreCrusher`, `CargoPod`, `LoadingBay`, `Player`, `WaterBody`, …).  
- Exact Name match for entity fovea already prefers exact — keep that.

### Implementation checklist

- [x] Investigate BRP-queryable components for parent link (Bevy 0.19 `ChildOf` / `Parent` / hierarchy). If queryable:  
  - [x] Query Name + Transform + parent component in `query_all_subjects` (or second query).  
  - [x] Pure helper e.g. `is_structural_child(name, parent_entity, local_translation) -> bool`.  
  - [x] Demote subjects that are children of a Named gameplay parent **or** near-zero local translation under a parent.  
- [x] If parent component **not** reliably queryable via BRP:  
  - [x] Document limitation.  
  - [x] Implement best-effort: demote by **relative** heuristic only where safe **plus** refine string table.  
  - [x] Still add pure helper API so hierarchy path can land when BRP allows.  
- [x] Integrate into `gameplay_subject_score` / `is_noise_name` / `filter_subjects` so multiparts score ≤0 or are filtered.  
- [x] Unit tests:  
  - [x] Flood of child Names + one `OreCrusher` → OreCrusher in top-N (extends `filter_keeps_ore_crusher_when_multiparts_crowd`).  
  - [x] Top-level `SolarFlareBuoy` not demoted.  
  - [x] Exact dogfood stems still score >0.  
- [x] Reduce dependence on over-broad prefixes (`starts_with("Solar")` class bugs).  
- [x] `cargo test -p specula -p specula-brp` PASS.  
- [x] `cargo install --path crates/specula --force`.

**T1 exit:** Structural (or documented hybrid) demotion with unit tests; OreCrusher-class features survive multipart floods without only relying on +120 stem boost.

---

## 6. Phase T2 — `see_pack` always restores camera (order 4)

**Goal:** After landscape/water (or any pack that nudges camera), **game camera Transform is restored** so subsequent `see entity` / verify is not on alt pose.

### Requirements

- [x] In `capture_with_camera_nudge` / pack path: always attempt restore of prior translation (and log failure if restore mutate fails).  
- [x] Restore runs on **success and early-exit** paths where mutate succeeded (use defer/finally-style so alt path cannot leave camera dirty).  
- [x] Packet note when restore failed (warning).  
- [x] Unit tests for pure translation round-trip / restore value formatting (`translation_value_for_brp` already array — keep).  
- [x] Optional integration note: after pack landscape, StrategyCamera translation equals pre-pack (live T5).  
- [x] Skill + workflow: restore is **code-guaranteed**, not only agent prompt.  
- [x] `cargo test` + install `--force`.

**T2 exit:** Packs cannot silently leave StrategyCamera on side-orbit; warnings if restore fails.

---

## 7. Phase T3 — Stronger multi-view (L2 / order 5)

**Goal:** When game vs first alt are similar, produce a **more informative** second view **or** only honest `views_similar`.

### Requirements

- [x] Prefer **temporary dedicated side/orbit camera** entity (spawn via BRP if feasible) **or** stronger pure placement than high-Y strategy nudge alone.  
- [x] Keep `side_orbit_camera_translation` high-Y drop behavior; unit-test pure math.  
- [x] If second path still similar: set `views_similar` / warning; **do not** claim multi-angle insight.  
- [x] Do not require dual live games.  
- [x] Unit tests for placement helpers.  
- [x] Live T5: IF landscape game hash ≠ alt **or** honest similar flag; open PNGs.  
- [x] `cargo test` + install `--force`.

**T3 exit:** Multi-view path is stronger or more honest; tests green.

---

## 8. Phase T4 — Packet↔pixel regression (L4)

**Goal:** Durable checks so ranking/fovea/pack restore regressions fail **without** a full human session.

### Requirements

- [x] Unit tests covering:  
  - [x] Dogfood stems survive multipart flood (T1).  
  - [x] Topdown OreCrusher-like projection off-center (T0 already).  
  - [x] Pack restore invariant (pure or mocked mutate sequence if possible).  
- [x] Align `.grok/workflows/agent-sight-dogfood.rhai`:  
  - [x] `see entity --profile iron-feud` mandatory (already).  
  - [x] Do not rely solely on agent memory for camera restore after T2.  
  - [x] Gates phase still runs `cargo test -p specula -p specula-brp`.  
- [x] Document how to run: `/agent-sight-dogfood` with `mode=gates|full`.  
- [x] Evidence path convention: prefer game/repo `captures/`, not shared `/tmp`.

**T4 exit:** Tests + workflow text match code contracts.

---

## 9. Phase T5 — Live sequential dogfood proof

**Goal:** Prove T1–T3 on CD + IF with open PNGs.

### T5.1 Crystal Drift

```text
killall crystal_drift iron_feud
Launch CD remote,capture → brp wait
see verify --profile crystal-drift --save-baseline
packs env_2d, hud
Assert: primary=Player; CargoPod (or current dogfood Name) in subjects if spawned
OPEN full + fovea; magenta plates absent
Log → {SCRATCH}/dogfood-see-2d.log; copy eyesight/cd/
```

### T5.2 Iron Feud

```text
Stop CD
IRON_FEUD_AUTO_PLAY=1 launch IF → brp wait
see verify --profile iron-feud --save-baseline
see pack landscape; see pack water
Assert camera restored (query StrategyCamera ≈ pre-pack or re-verify game view)
see entity --name OreCrusher --profile iron-feud
Assert fovea note contains topdown3d; screen_xy not near false center (1303,776 class)
OPEN landscape game/alt + entity crop
Log → {SCRATCH}/dogfood-see-3d.log; eyesight/if/
```

### T5.3 Preferred one-shot

- [x] Run `/agent-sight-dogfood` `mode=full` with `skip_install=false` if T1–T3 just installed, else `true` after install.  
- [x] Overall pass preferred; if skeptic red, fix code (not only prompts) and re-run failed game.

### T5.4 Review matrix

| Check | CD | IF |
|-------|----|----|
| Build remote,capture | ✓ | ✓ |
| primary sensible | Player | not OreCrystal* |
| Dogfood Name in subjects | CargoPod (or current) | OreCrusher (or current) |
| Multiparts not crowding out feature | ✓ | ✓ |
| Pack leaves camera restored | n/a or cam | ✓ |
| Entity fovea profile / aim | ortho profile | topdown3d XZ correct aim |
| Multi-view honest | packs ran | game≠alt or views_similar |
| PNG opened | ✓ | ✓ |

**T5 exit:** Live evidence under `{SCRATCH}`; skeptic-grade honesty.

---

## 10. Phase T6 — Docs and closeout

- [x] Flip checkboxes in this file when truly done.  
- [x] Write `docs/AGENT_SIGHT_TRUST_FINDINGS_YYYY-MM-DD.md` (T0–T5 results, packet paths, residual).  
- [x] Update [PROGRESS.md](../PROGRESS.md), [ROADMAP.md](ROADMAP.md), [AGENTS.md](../AGENTS.md), `bevy-agent-loop` skill.  
- [x] Commit Specula (and CD/IF only if dogfood trees changed).  
- [x] No force-push.

---

## 11. Success metrics (definition of done)

1. **T0** fovea profile tests remain green.  
2. **T1** structural (or hybrid) demotion: multiparts do not drop dogfood Names from top-N under flood tests.  
3. **T2** `see_pack` restores camera; restore failure is visible in packet warnings.  
4. **T3** multi-view stronger or honestly similar; pure helpers unit-tested.  
5. **T4** regression tests + workflow aligned with code.  
6. **T5** live CD+IF evidence; entity OreCrusher fovea aims correctly with profile; open PNGs.  
7. **T6** findings + docs + commits.  
8. **No exclusions** implemented.  
9. `cargo test -p specula -p specula-brp` PASS; MCP install after platform edits.

---

## 12. Risks

| Risk | Mitigation |
|------|------------|
| Parent component not BRP-visible | Hybrid demotion + document; still improve pure API |
| Restore race with Bevy frame | Wait/query after mutate; re-verify game view |
| Dedicated camera spawn fails on minimal apps | Fallback to nudge + honest views_similar |
| Workflow agents ignore profile | Code defaults: consider applying iron-feud when Name+Transform Z-dominant heuristic — optional; prefer explicit profile |
| Scope creep into craft art | Trust plan is platform; no require new CD/IF features |

---

## 13. `/goal` paste template

```text
Execute docs/AGENT_SIGHT_TRUST_PLAN.md to completion (T0 → T6).
I will be away — take as long as needed; NO shortcuts;
long-term correctness over speed. Prefer durable tests, live captures, honest logs.

Order:
  T0  Verify fovea --profile fix + cargo test
  T1  Hierarchy/local-space child demotion (structural ranking)
  T2  see_pack always restores camera after alt/side-orbit
  T3  Stronger multi-view (dedicated side cam or honest views_similar)
  T4  Packet↔pixel regression tests + workflow alignment
  T5  Live sequential dogfood CD then IF (prefer /agent-sight-dogfood full)
  T6  Findings, PROGRESS, ROADMAP, AGENTS, skill, commit

Dogfood:
  2D: /Users/aron/Documents/coding_projects/Crystal Drift
  3D: /Users/aron/Documents/coding_projects/Iron Feud
Bevy 0.19, remote,capture, BRP 15702. Iron Feud: IRON_FEUD_AUTO_PLAY=1.
Entity fovea: always --profile iron-feud|crystal-drift.
Exclude: livestream/60fps, human editor, unlit suite, auto taste scoring.
Rebuild MCP after platform changes. Taste/design human-owned; agent sight only.
```

---

## 14. Document history

| Date | Change |
|------|--------|
| 2026-07-25 | T0–T6 complete: structural demotion, pack restore, dedicated multi-view, workflow PASS |
| 2026-07-25 | Initial trust plan: L1–L4 + order 3–5 (hierarchy demotion, pack restore, multi-view, regression) |
