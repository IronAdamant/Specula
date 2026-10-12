# Agent Sight 2D + 3D Plan — dimension-specific observation quality

**Status:** complete (D0–D5 shipped 2026-07-22)  
**Audience:** implementing agent under `/goal`; human sets goal then may be away  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**Focus:** improve **agent sight** for both **2D** and **3D** Bevy games so the current-generation model can observe layouts, assets, and environments reliably — then dogfood on real games with new content and env work.

### Long-session / no-shortcuts mandate (read first)

This plan is written for a session where the human may be **away for several hours**. The implementer is expected to:

- Take **as long as needed** to finish S0→S5 thoroughly.  
- Prefer **correct, durable** code, tests, builds, live captures, and honest findings over speed.  
- **No shortcuts:** no stub-only features, no fake/synthetic screenshots, no skipping live verify when a GPU/window is available, no “looks done” without packet + PNG proof.  
- If blocked (compile, BRP, window), log honestly under `{SCRATCH}` and still prove structural + unit tests — then retry live when possible.  
- Rebuild MCP (`cargo install --path crates/specula --force` if host uses cargo bin) **before** treating dogfood captures as pass evidence.  
- Taste and design direction remain **human-owned**; agent **sees and builds** to requirements.

### Parent / prior work (shipped)

| Doc | Role |
|-----|------|
| [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) | V0–V6 eyes open |
| [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md) | A0–A8 acuity |
| [AGENT_SIGHT_NEXT_PLAN.md](AGENT_SIGHT_NEXT_PLAN.md) | S0–S4 ranking, profiles, verify |
| [AGENT_SIGHT_NEXT_FINDINGS_2026-07-21.md](AGENT_SIGHT_NEXT_FINDINGS_2026-07-21.md) | Lessons |

### Dogfood trees (required)

| Dimension | Path | Launch notes |
|-----------|------|----------------|
| **2D** | `/Users/aron/Documents/coding_projects/Crystal Drift` | Playing by default; features `remote,capture` |
| **3D** | `/Users/aron/Documents/coding_projects/Iron Feud` | **`IRON_FEUD_AUTO_PLAY=1`** for Playing; features `remote,capture` |

Pins: Bevy **0.19**, BRP **15702**, `remote,capture`. Skills: `bevy-agent-loop`, eyesight-packs.

---

## 1. North star

> **2D and 3D each have sight defaults and packs that match how those games look**, proven by live `see verify` / packs on Crystal Drift and Iron Feud with **new Named features+assets displayed on screen**, improved environments (including **3D height variation: flats → hills → mountains**), MCP rebuilt first, and full review evidence.

### Exclusions (do not implement)

| Excluded | Why |
|----------|-----|
| 60 FPS / livestream / continuous video | Deferred; stills + short strips only |
| Human editor / hierarchy UI / gizmos | Out of scope |
| Full unlit material suite | Later |
| Auto taste / beauty scoring | Human-owned |

---

## 2. Problem split (2D vs 3D)

| Space | Gaps to close |
|-------|----------------|
| **Shared** | Profiles/ranking polish; new Names must score in filter; verify habit; baseline after change; multi-view honesty |
| **2D** | Ortho half-extents vs arena; sprite craft (full→fovea→zoom); HUD/region presets; parallax/env pack without Star spam; dark space still not black_frame |
| **3D** | Top-down projection quality; multi-view that differs; env primary over OreCrystal; **terrain height variation** so landscape isn’t a single flat plane; water/terrain packs show relief |

---

## 3. Execution order (hard)

```text
D0  Specula sight code (2D+3D shared + dim-specific helpers) + unit tests
    → cargo test -p specula -p specula-brp
    → cargo build -p specula
    → cargo install --path crates/specula --force   # if MCP uses cargo bin
    → confirm: specula see --help lists verify; instructions mention 2d/3d packs if added

D1  Crystal Drift (2D): 2 new features+assets + env improve + live display
D2  Iron Feud (3D): 2 new features+assets + height-varying terrain + env improve + live display
D3  Live eyesight review both games with NEW binary/MCP (see verify, pack, fovea, open PNGs)
D4  Fix failures found in D3 (code or content); re-verify until green
D5  Docs: checkboxes, PROGRESS, findings + assessments; skill touch-ups
```

**Never start D1–D3 captures as pass/fail evidence on a stale MCP.**

---

## 4. Phase D0 — Specula agent-sight upgrades (both dimensions)

Implement in `crates/specula-brp` / `crates/specula` (MCP + CLI). Unit-test pure logic.

### D0.1 Shared polish

- [x] Confirm/extend **game profiles** `crystal-drift` / `iron-feud` (retune `visible_half_*` if dogfood shows bad fovea).  
- [x] Ensure **GAMEPLAY_NAME_HINTS** (or equivalent) includes any new dogfood Name stems planned in D1/D2 so they survive `gameplay_prefer`.  
- [x] **`bevy_see_verify`** remains the default one-shot; skill text says use it first.  
- [x] Baseline: after visual change, `save_baseline` / `compare_baseline` path documented and used in D3.  
- [x] Motion: stills-only strips; optional mutate stimulus; no video.  
- [x] Multi-view: keep `views_similar` warning when alt ≈ game; prefer larger nudge if similar.  

### D0.2 2D-specific sight

- [x] Profile or pack helpers for **2D sprite craft**: after verify, ensure fovea+zoom on `Player` (or ranked primary).  
- [x] Optional pack or documented preset: **`hud` / region** — e.g. top-left HUD crop via `see_region` constants in skill (or `bevy_see_pack pack=hud` if small).  
- [x] Optional pack **`env_2d`**: full + horizon-ish band + station/debris crop (composition for parallax worlds).  
- [x] Unit tests for any new pure helpers (region defaults, profile apply).  

### D0.3 3D-specific sight

- [x] Profile **iron-feud** (or `topdown3d`) remains require_playing + wait for camera/water/ground.  
- [x] Landscape pack documents **height readability**: full + alt view should show relief when terrain has hills/mountains (D2).  
- [x] Optional: packet note when subjects include height-band Names (e.g. `TerrainFlat`, `TerrainHill`, `TerrainPeak`) if games expose them.  
- [x] Diagnostic primary continues to use ranker/allowlist (WaterBody/Ground/…), not Player-only.  

### D0.4 MCP surface

- [x] Schemas/instructions mention 2D vs 3D profiles and new packs if added.  
- [x] CLI `--profile` + any new pack names.  
- [x] `cargo test` green; binary installed/rebuilt; log surface to `{SCRATCH}/mcp-surface.log`.  

**D0 exit criteria:** Tests green; `see verify --profile crystal-drift|iron-feud` works from rebuilt binary; no excluded features.

---

## 5. Phase D1 — Crystal Drift (2D) content + env

**Path:** `/Users/aron/Documents/coding_projects/Crystal Drift`

### Required quotas (all must ship and appear in live BRP Names)

| Quota | Requirement | Concrete direction (names may vary; counts may not) |
|-------|-------------|-----------------------------------------------------|
| **2 new features + assets** | Two new gameplay-relevant **Named** entities **and** disk assets under `assets/`, **spawned in Playing**, visible in captures | **(1)** e.g. `CometFragment` — collectible/hazard sprite + system hook (score/damage/fuel). **(2)** e.g. `SignalSat` or `MineDrone` — Named prop or weak ally/enemy with sprite |
| **Environment improve** | Modify existing env so landscape/env pack reads better | Improve nebulas and/or DebrisRing/DerelictStation (position, scale, tint, layering) — not a no-op rename |
| **Display + test** | Build, run, eyesight | `cargo build --features remote,capture`; live `see verify --profile crystal-drift`; subjects include both new Names + Player |

### Implementation checklist

- [x] Asset files on disk; paths in `resources` / loading  
- [x] Components + spawn with **`Name::new(...)`**  
- [x] Systems for any collect/combat behavior (keep modular; pure sim if pattern exists)  
- [x] Env improve applied in `gameplay_setup` (or equivalent)  
- [x] Build log `{SCRATCH}/crystal-drift-build.log` PASS  
- [x] No dead unused assets left unreferenced  

### Eyesight expectations (D3)

- `primary_subject` prefers **Player** when present  
- Subjects include both new feature Names  
- Env changes visible in full and/or landscape pack (observation notes in findings)  
- Open full + fovea PNGs (agent image-read)  

---

## 6. Phase D2 — Iron Feud (3D) content + height-varying terrain

**Path:** `/Users/aron/Documents/coding_projects/Iron Feud`  
**Playing:** `IRON_FEUD_AUTO_PLAY=1` mandatory for all env claims  

### Required quotas

| Quota | Requirement | Concrete direction |
|-------|-------------|--------------------|
| **2 new features + assets** | Two new **Named** entities with mesh/tint assets, visible in Playing | **(1)** e.g. `WatchPost` — elevated lookout mesh. **(2)** e.g. `OreSilo` or `PipeJunction` — industrial prop with Name |
| **Environment improve + height range** | Ground is **not** a single flat plane only | Implement **varying terrain height**: continuous range from **flat** → **rolling hills** → **mountain/ridge** peaks (multiple height bands, not one decorative cube). Existing Ground may be replaced/augmented with heightfield, multi-plane, or multi-mesh height samples. Names recommended: e.g. `TerrainFlat`, `TerrainHill_*`, `TerrainPeak_*` or one `HeightTerrain` plus visible relief in captures |
| **Env improve (existing)** | Improve water and/or existing rocks/cliffs so packs show depth | e.g. water edge vs raised land, rocks on slopes |
| **Display + test** | Build, run Playing, eyesight | Build log `{SCRATCH}/iron-feud-build.log`; live `see verify --profile iron-feud`; landscape/water packs show height variation |

### Height terrain acceptance (non-negotiable)

- [x] At least **three distinct height bands** present in world (flat / mid / high), measurable in code (e.g. Y of mesh tops or vertex heights) and **visible** in landscape pack PNGs.  
- [x] Factory start area remains playable (placement not broken; document any grid cells reserved).  
- [x] Strategy camera still sees start + some elevated terrain.  
- [x] Live packet subjects include new feature Names + evidence of terrain Names or clear observation that relief is visible in captures.  

### Implementation checklist

- [x] Spawn helpers for new props + terrain  
- [x] Textures under `assets/models/` if needed  
- [x] Water_tint still wired if water touched  
- [x] `IRON_FEUD_AUTO_PLAY=1` still enters Playing  
- [x] Build PASS; fix height/placement bugs found in play  

### Eyesight expectations (D3)

- `app_state=Playing`; not Menu-only  
- `primary_subject` prefers WaterBody/StrategyCamera/Ground over OreCrystal  
- New feature Names present  
- Landscape pack: full + alt; height relief described in findings after opening images  

---

## 7. Phase D3 — Live eyesight review (both games, new MCP)

Sequential on port **15702** (stop one game before starting the other).

### D3.1 Crystal Drift

```text
1. killall crystal_drift iron_feud (exact names; avoid pkill -f self-match)
2. Launch CD binary (warm target/debug)
3. specula brp wait --port 15702
4. specula see verify --profile crystal-drift --out-dir <CD path>
5. specula see entity --name Player (and one new feature Name)
6. specula see pack landscape --profile crystal-drift  (or env_2d if added)
7. Optional: baseline → small mutate → compare_baseline
8. Assert: primary=Player; new Names in subjects; captures bytes>0; no false black_frame
9. OPEN primary full + fovea images; note observation (not taste essay)
10. Log → {SCRATCH}/dogfood-see-2d.log; copy packets/PNGs under {SCRATCH}/eyesight/
```

### D3.2 Iron Feud

```text
1. Stop CD
2. IRON_FEUD_AUTO_PLAY=1 launch iron_feud
3. brp wait
4. see verify --profile iron-feud --out-dir <IF path>
5. see entity --name WaterBody (or WatchPost / new prop)
6. see pack landscape + water --profile iron-feud
7. Assert: Playing; new Names; terrain height visible in landscape captures;
          primary not OreCrystal*; abs_path PNGs bytes>0
8. OPEN full + landscape (+ water) images; note height bands observed
9. Log → {SCRATCH}/dogfood-see-3d.log
```

### D3.3 Review criteria (must pass or go to D4)

| Check | CD | IF |
|-------|----|----|
| Build features remote,capture | ✓ | ✓ |
| Live Playing capture | ✓ | ✓ AUTO_PLAY |
| 2 new feature Names in packet | ✓ | ✓ |
| Env improve reflected | ✓ | ✓ + height bands |
| primary ranking sensible | Player | WaterBody/camera/ground |
| PNG evidence non-empty | ✓ | ✓ |
| Image opened / observed | ✓ | ✓ |

**D3 exit criteria:** All review criteria pass, or D4 fixes then re-run until pass.

---

## 8. Phase D4 — Fix and re-verify

- [x] Fix any compile, spawn, BRP Name missing, filter score 0, height flat, menu-state, or black-frame false positive.  
- [x] Re-run only the failed game’s D3 steps.  
- [x] Do not declare done with known missing Names or flat-only terrain on IF.  

---

## 9. Phase D5 — Docs and closeout

- [x] Flip all checkboxes in this file to `[x]` when truly done.  
- [x] Update [PROGRESS.md](../PROGRESS.md) with this plan section.  
- [x] Write `docs/AGENT_SIGHT_2D3D_FINDINGS_YYYY-MM-DD.md`:  
  - Features added (Specula + each game)  
  - Asset paths  
  - Packet paths / primary_subject  
  - Height terrain description (IF)  
  - **Assessments** (what sight improved, residual gaps)  
- [x] Skill `bevy-agent-loop` / eyesight-packs: 2D vs 3D profile notes, height terrain dogfood, no-shortcut note if useful  
- [x] Link from ROADMAP / AGENTS  

---

## 10. Success metrics (definition of done)

1. **D0** shipped and **MCP/binary rebuilt** before final dogfood evidence.  
2. **Crystal Drift:** ≥2 new features with assets **displayed** and Named in live packets; env improved; verify pass.  
3. **Iron Feud:** ≥2 new features with assets displayed; **terrain height varies flat→hill→mountain**; env improved; Playing verify pass.  
4. **Live eyesight** both games: non-empty PNGs, open/read observations, logs under `{SCRATCH}`.  
5. **Unit tests** for new pure helpers PASS; `cargo test -p specula -p specula-brp` PASS.  
6. **No exclusions** implemented.  
7. **Findings + assessments** written; plan checkboxes complete.  
8. **No shortcuts** — if time is long, finish correctly rather than stub.  

---

## 11. Suggested asset/feature names (non-binding)

| Game | Features | Env |
|------|----------|-----|
| CD | `CometFragment`, `SignalSat` | improve nebulas/station; optional new belt if not already DebrisRing |
| IF | `WatchPost`, `OreSilo` | `TerrainFlat` / `TerrainHill_*` / `TerrainPeak_*` or equivalent heightfield |

Implementer may rename but **must** meet counts and IF height range.

---

## 12. Risks

| Risk | Mitigation |
|------|------------|
| Height terrain breaks placement grid | Keep start cells flat; document reserved cells |
| New Names filtered out | Update GAMEPLAY_NAME_HINTS / scores in D0 |
| Stale MCP | install --force + session reload before D3 pass |
| Port 15702 conflict | Sequential games only |
| External trees not in Specula git | Edit external paths; commit Specula docs/code separately |
| Context limits mid-goal | Prefer finishing D0 fully, then one game fully, then the other; use todos |

---

## 13. `/goal` paste template

```text
Execute docs/AGENT_SIGHT_2D3D_PLAN.md to completion (D0 → D5).
I will be away for several hours — take as long as needed; NO shortcuts;
long-term correctness over speed. Prefer durable tests, live captures, honest logs.

Order mandatory: Specula sight/MCP first (rebuild install), then dogfood
  2D: /Users/aron/Documents/coding_projects/Crystal Drift
  3D: /Users/aron/Documents/coding_projects/Iron Feud
Each game: 2 new features+assets displayed+tested; improve existing environments.
3D: varying ground height flat → hills → mountains (visible in landscape packs).
Live see verify + review both; fix until green. Findings + assessments.
Exclude: livestream/60fps, human editor, unlit suite, auto taste scoring.
Bevy 0.19, remote,capture, BRP 15702. Iron Feud: IRON_FEUD_AUTO_PLAY=1.
Update plan checkboxes + PROGRESS. Taste/design human-owned; agent sight only.
```

---

## 14. Document history

| Date | Change |
|------|--------|
| 2026-07-22 | Initial 2D+3D agent sight plan; CD/IF dogfood; IF height terrain; long-session no-shortcut mandate |
| 2026-07-22 | D0–D5 complete: 2D packs hud/env_2d, IF height bands, CD CometFragment+SignalSat, IF WatchPost+OreSilo; findings doc |
