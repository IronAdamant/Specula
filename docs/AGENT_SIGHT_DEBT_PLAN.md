# Agent Sight Debt Plan — residual acuity + full dogfood asset/env pass

**Status:** complete (R0–R4 shipped 2026-07-23)  
**Audience:** implementing agent under `/goal`; human may be away for a long session  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**Focus:** close **remaining sight debt** (filter, 3D multi-view, craft fidelity, dual-port honesty), then dogfood by **(1)** one new Named feature+asset per game, **(2)** one new environment element per game, and **(3)** **improving every existing asset and environment** on Crystal Drift and Iron Feud so live packets prove the upgrade.

### Long-session / no-shortcuts mandate

- Take as long as needed; **correctness over speed**.  
- **No shortcuts:** no stub assets, no synthetic screenshots, no “improved” renames without visible change, no skip of live verify when GPU/window available.  
- Rebuild MCP (`cargo install --path crates/specula --force`) **before** treating dogfood captures as pass evidence.  
- Taste/design remain **human-owned**; agent **sees and builds** to this plan’s quotas.  
- External trees are **not** in Specula git — edit them in place; commit Specula docs/code separately.

### Parent / prior work (shipped)

| Doc | Role |
|-----|------|
| [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) | V0–V6 eyes open |
| [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md) | A0–A8 acuity |
| [AGENT_SIGHT_NEXT_PLAN.md](AGENT_SIGHT_NEXT_PLAN.md) | S0–S4 ranking, profiles, verify |
| [AGENT_SIGHT_2D3D_PLAN.md](AGENT_SIGHT_2D3D_PLAN.md) | D0–D5 2D/3D packs, height terrain |
| [AGENT_SIGHT_2D3D_FINDINGS_2026-07-22.md](AGENT_SIGHT_2D3D_FINDINGS_2026-07-22.md) | Residual gaps this plan closes |

### Dogfood trees (required)

| Dimension | Path | Launch notes |
|-----------|------|----------------|
| **2D** | `/Users/aron/Documents/coding_projects/Crystal Drift` | Playing by default; features `remote,capture` |
| **3D** | `/Users/aron/Documents/coding_projects/Iron Feud` | **`IRON_FEUD_AUTO_PLAY=1`**; features `remote,capture` |

Pins: Bevy **0.19**, BRP **15702**, `remote,capture`. Skills: `bevy-agent-loop`, eyesight-packs.

---

## 1. North star

> **Agent sight is honest about assets and environments:** every disk asset is craft-visible under fovea/pack; every Named env reads on full + landscape; residual filter/multi-view/craft debts are closed with unit tests + live dogfood on CD and IF after MCP rebuild; each game ships **exactly one new feature+asset**, **exactly one new environment element**, and a **full pass improving all existing assets and environments**.

### Exclusions (do not implement)

| Excluded | Why |
|----------|-----|
| 60 FPS / livestream / continuous video | Deferred; stills + short strips only |
| Human editor / hierarchy UI / gizmos | Out of scope |
| Full unlit material suite | Later optional |
| Auto taste / beauty scoring | Human-owned |
| Parallel dual-game BRP on same port | Sequential only on 15702 (document dual-port recipe only) |

---

## 2. Remaining sight debt (from findings + assessment)

### Debt register (must close or explicitly defer with reason)

| ID | Debt | Why it hurts agents | Target exit |
|----|------|---------------------|-------------|
| **R1** | **Filter is load-bearing** — new Names vanish if score ≤0 | Dogfood “shipped” content invisible in packets | Checklist + unit tests for every new Name stem; optional `register_gameplay_hint` helper or expanded hints table with tests |
| **R2** | **OreCrystal / child mesh noise** still fragile | Packets can re-fill with ore/children if scoring regresses | Hard demotion tests; optional collapse of local-space children (0,0,0 relative) from subject list |
| **R3** | **3D multi-view often hash-similar** | Agents overclaim multi-angle insight | Side/orbit camera path for landscape alt **or** larger documented nudge + honest `views_similar`; unit/integration note when alt differs |
| **R4** | **3D fovea approximate** | Prop craft hard to judge | Profile half-extents retune for IF; entity pack on WatchPost/OreSilo/WaterBody with zoom ladder; optional AABB inflate for tall meshes |
| **R5** | **2D env subtlety** (nebulas soft on black) | Landscape/env packs under-read env | Full asset+env improve pass (CD) so nebulas/station/debris read on full frame |
| **R6** | **Asset craft uneven** (some CD sprites are tiny procedural; IF tints 16×16 flats) | Fovea shows blobs / uniform slabs | **Improve every existing asset** (regenerate or hand-craft higher-contrast, larger, keyable sprites / richer tints) |
| **R7** | **Baseline habit weak** | Visual regressions not caught | D-phase dogfood: save_baseline → mutate or asset change → compare_baseline once per game |
| **R8** | **Dual BRP not agent-default** | Two games = port fights | Doc + skill only: sequential dogfood; optional second port recipe (no requirement to run dual live) |

### Debt → phase map

```text
R0  Specula sight debt code (R1–R4, R7 surface) + unit tests + install --force
R1  Crystal Drift: 1 new feature+asset + 1 new env + improve ALL existing assets/env + live verify
R2  Iron Feud: 1 new feature+asset + 1 new env + improve ALL existing assets/env/height + live verify
R3  Sequential live review both games (new MCP); open PNGs; fix loop
R4  Docs: checkboxes, PROGRESS, findings+assessments; skill/ROADMAP
```

**Never treat R1–R3 captures as pass evidence on a stale MCP.**

---

## 3. Phase R0 — Specula residual sight code

Implement in `crates/specula-brp` / `crates/specula`. Pure logic unit-tested.

### R0.1 Filter / ranking debt (R1, R2)

- [x] Document **Name onboarding rule** in skill + code comment: every new dogfood Name stem must score `gameplay_subject_score > 0`.  
- [x] Add/extend unit tests: every stem planned in R1/R2 (new feature + new env Names) scores >0 and survives `GameplayPrefer` with OreCrystal spam present.  
- [x] Harden demotion of `OreCrystal*` and child mesh parts; ensure `WaterBody` never treated as noise.  
- [x] Optional pure helper: `list_subject_stems_requiring_hints(&[...])` or table of dogfood stems shared with tests.  
- [x] Prefer collapsing **local-space children** (translation ~0 parent-relative already collapsed by name where possible) so WatchPostLegs etc. never crowd max_subjects.

### R0.2 3D multi-view debt (R3)

- [x] Landscape/water alt: try **side nudge** (XZ offset) when projection is TopDown3d if pure Y-lift still yields `views_similar`.  
- [x] Keep `views_similar` warning honest.  
- [x] Unit-test pure nudge selection if extracted; otherwise document behavior in skill.

### R0.3 3D fovea debt (R4)

- [x] Retune `iron-feud` `visible_half_*` if dogfood fovea still misses tall props (WatchPost ~3u height).  
- [x] Optional: taller entities get larger default crop half when name matches tall prefixes (WatchPost, OreSilo, Relay, TerrainPeak).  
- [x] Unit tests for any pure crop/half helper.

### R0.4 Baseline + surface (R7)

- [x] Skill + MCP instructions: after asset/env change, `save_baseline` then `compare_baseline` once per game in R3.  
- [x] CLI/MCP still expose baseline flags on scene/verify.  
- [x] `cargo test -p specula -p specula-brp` PASS; `cargo build -p specula` PASS.  
- [x] `cargo install --path crates/specula --force`; log `specula see --help` + pack list to `{SCRATCH}/mcp-surface.log`.

**R0 exit:** Tests green; binary rebuilt; no excluded features.

---

## 4. Phase R1 — Crystal Drift (2D): new + full improve pass

**Path:** `/Users/aron/Documents/coding_projects/Crystal Drift`

### Quotas (all required)

| Quota | Requirement | Concrete direction (names may vary; counts may not) |
|-------|-------------|-----------------------------------------------------|
| **1 new feature + asset** | One new gameplay-relevant **Named** entity + **new disk sprite** under `assets/sprites/`, spawned in Playing, systems if collect/combat | e.g. **`IceShardField`** or **`SolarFlareBuoy`** — hazard or bonus with sprite + score/fuel/shield hook |
| **1 new environment** | One new Named env prop + disk asset, visible in env_2d/landscape | e.g. **`WarpGateRing`** or **`DustCloudBelt`** — large backdrop/landmark, not a tiny unreadable speck |
| **Improve ALL existing assets** | Every file under `assets/sprites/` must be **visibly upgraded** (regenerate/replace; not byte-identical noop) and remain referenced | See inventory table below |
| **Improve ALL existing environments** | Every Named env + setup path retuned (position/scale/tint/layering/readability) | Nebulas, DerelictStation, DebrisRing, backdrop; keep CometFragment/SignalSat/Beacon/Rescue readable |

### Existing asset inventory (each must improve)

| Asset path | Role today | Improve bar (minimum) |
|------------|------------|------------------------|
| `sprites/player.png` | Player craft | Higher contrast silhouette, readable thrusters at fovea 192px |
| `sprites/asteroid_large.png` | Large rock | Clearer crater/edge; not muddy on black |
| `sprites/asteroid_medium.png` | Med rock | Same family as large; distinct size class |
| `sprites/asteroid_small.png` | Small rock | Same family; legible at small scale |
| `sprites/boost_flame.png` | Boost FX | Brighter, readable trail tongue |
| `sprites/crystal.png` | Crystal pickup | Faceted, high chroma vs space |
| `sprites/fuel_canister.png` | Fuel pickup | Distinct can silhouette (was often tiny) |
| `sprites/enemy_scout.png` | Enemy | Hostile silhouette vs player |
| `sprites/scrap.png` | Scrap | Metal shards readable |
| `sprites/shield_orb.png` | Shield powerup | Glow orb distinct from crystal |
| `sprites/nebula.png` | Env cloud | Soft but **visible** color masses on black (fix R5) |
| `sprites/station.png` | DerelictStation | Larger readable station mass |
| `sprites/beacon_buoy.png` | Beacon | Nav light readable |
| `sprites/rescue_pod.png` | Rescue pod | Capsule silhouette clear |
| `sprites/debris_ring.png` | DebrisRing | Ring/arc structure visible full frame |
| `sprites/comet_fragment.png` | CometFragment (D1) | Sharper ice + tail |
| `sprites/signal_sat.png` | SignalSat (D1) | Clear panels + dish |

**Improve means:** replace PNG with better craft (keyable bg removed / transparent), keep paths stable, rebuild referenced. Log before/after byte sizes optional; live fovea is the proof.

### Existing environment improve checklist

- [x] `SpaceBackdrop` — keep deep black or subtle gradient; must not defeat black_frame honesty  
- [x] `NebulaCloud_*` — larger, higher alpha, spread for env_2d horizon/center packs  
- [x] `DerelictStation` — scale/tint/position so full frame reads landmark  
- [x] `DebrisRing` — scale/tint; not lost in stars  
- [x] Starfield — if present, ensure not subject spam (Stars already demoted)  
- [x] Gameplay Named props (BeaconBuoy, RescuePod, CometFragment, SignalSat) — positions for simultaneous on-screen at start where possible  

### Implementation checklist

- [x] New feature sprite on disk + `GameAssets` + loading + `Name::new` + system  
- [x] New env sprite on disk + spawn Named  
- [x] All existing sprites replaced/improved and referenced  
- [x] Env setup pass applied  
- [x] GAMEPLAY_NAME_HINTS stems for new Names (R0)  
- [x] Build: `cargo build --features remote,capture` → `{SCRATCH}/crystal-drift-build.log` PASS  
- [x] No unreferenced dead assets  

### Eyesight expectations (R3)

- `primary_subject=Player` when present  
- Subjects include **new feature Name** + **new env Name** + prior D1 Names  
- Full frame: env readable (not pure black with one ship only)  
- Fovea: Player + at least one improved prop crop non-empty, craft-visible  
- Open full + fovea; short observation notes  

---

## 5. Phase R2 — Iron Feud (3D): new + full improve pass

**Path:** `/Users/aron/Documents/coding_projects/Iron Feud`  
**Playing:** `IRON_FEUD_AUTO_PLAY=1` mandatory for env claims  

### Quotas (all required)

| Quota | Requirement | Concrete direction |
|-------|-------------|--------------------|
| **1 new feature + asset** | One new Named prop + mesh/tint under `assets/models/`, visible in Playing | e.g. **`RadarDome`** or **`FuelTankStack`** — industrial silhouette distinct from WatchPost/OreSilo |
| **1 new environment** | One new Named env element + asset | e.g. **`TerrainSaddle`** (mid-band bridge) or **`LavaFissure`** / **`MistBasin`** — must add readable world feature without breaking placement grid |
| **Improve ALL existing assets** | Every `assets/models/*.png` tint (and any future mesh textures) visibly richer | See inventory; 16×16 flat noise → higher-detail seamless tints or larger textures |
| **Improve ALL existing environments** | Every env spawn: Ground, water, rocks, trees, cliff, ash, scrap, height bands, prior props | Heights stay 3+ bands; start flat playable; water edge vs raised land clearer |

### Existing asset inventory (each must improve)

| Asset path | Used by | Improve bar |
|------------|---------|-------------|
| `models/ground_tint.png` | Ground / TerrainFlat | Subtle grass/dirt variation; still tile-friendly |
| `models/rock_tint.png` | RockOutcrop | Stone grain, readable on green ground |
| `models/tree_tint.png` | Tree canopy | Foliage green variation |
| `models/scrap_tint.png` | FieldScrap | Metallic scrap read |
| `models/cliff_tint.png` | CliffRidge | Stratified rock |
| `models/water_tint.png` | WaterBody | Clearer blue; slight depth cue |
| `models/relay_tint.png` | RelayTower | Metal panel |
| `models/crate_tint.png` | SupplyCrate | Wood/crate board |
| `models/ash_tint.png` | AshPlateau | Ash grey variation |
| `models/watch_tint.png` | WatchPost | Structure metal |
| `models/silo_tint.png` | OreSilo | Industrial rust/metal |
| `models/terrain_hill_tint.png` | TerrainHill_* | Hill vegetation/dirt |
| `models/terrain_peak_tint.png` | TerrainPeak_* | Peak rock/snow edge cue |

### Existing environment improve checklist

- [x] `Ground` plane — tint/material; still placement-safe  
- [x] `TerrainFlat` / `TerrainHill_*` / `TerrainPeak_*` — scale/position so **three bands read** in landscape full+alt; tops still match/update `height_terrain_samples()` tests  
- [x] `WaterBody` — size/edge vs land; emissive/tint for pack water  
- [x] `RockOutcrop_A/B` — scale, place on slopes near hills  
- [x] `TreeScrub_A/B` — height/canopy contrast  
- [x] `CliffRidge_West` — height/depth against peaks  
- [x] `AshPlateau` — scale/tint  
- [x] `FieldScrap_*` — silhouette vs ground  
- [x] Prior props: RelayTower, SupplyCrate, WatchPost, OreSilo — mesh proportions/materials for fovea  
- [x] Lighting (Sun / FillLight) — if env washed or too dark, retune illuminance (still not “unlit suite”)  
- [x] Document reserved start cells if height/env blocks placement  

### Implementation checklist

- [x] New feature spawn + tint asset + Name  
- [x] New env spawn + asset + Name  
- [x] All tints replaced; meshes/materials re-checked  
- [x] Height bands still distinct (`height_bands_are_distinct` + live landscape)  
- [x] `IRON_FEUD_AUTO_PLAY=1` → Playing  
- [x] Build log `{SCRATCH}/iron-feud-build.log` PASS  
- [x] Hints for new Names in R0  

### Eyesight expectations (R3)

- `app_state=Playing`  
- `primary_subject` in {StrategyCamera, WaterBody, Ground} — never OreCrystal*  
- Subjects include new feature + new env + height band Names  
- Landscape: height relief **visible** after opening PNGs  
- Water pack: water surface readable  
- Open full + landscape (+ water); note height bands  

---

## 6. Phase R3 — Live eyesight review (both games, new MCP)

Sequential on port **15702**.

### R3.1 Crystal Drift

```text
1. killall crystal_drift iron_feud
2. Launch CD warm binary (remote,capture)
3. specula brp wait --port 15702
4. specula see verify --profile crystal-drift --out-dir <CD>
5. specula see pack env_2d|hud|landscape --profile crystal-drift
6. specula see entity --name Player (and new feature Name)
7. save_baseline → small mutate or note → compare_baseline once
8. Assert: primary=Player; new Names present; improved assets visible in fovea; PNG bytes>0
9. OPEN full + fovea + env pack; write observations
10. Log → {SCRATCH}/dogfood-see-2d.log; copy to {SCRATCH}/eyesight/cd/
```

### R3.2 Iron Feud

```text
1. Stop CD
2. IRON_FEUD_AUTO_PLAY=1 launch iron_feud
3. brp wait
4. see verify --profile iron-feud
5. pack landscape + water; entity WatchPost or new prop; entity WaterBody
6. Assert: Playing; primary not OreCrystal*; height bands Names + visible relief; new Names
7. OPEN images; note height + improved tints
8. Log → {SCRATCH}/dogfood-see-3d.log; copy {SCRATCH}/eyesight/if/
```

### R3.3 Review matrix

| Check | CD | IF |
|-------|----|----|
| Build remote,capture | ✓ | ✓ |
| Live Playing | ✓ | ✓ AUTO_PLAY |
| 1 new feature Name | ✓ | ✓ |
| 1 new env Name | ✓ | ✓ |
| All existing assets improved (visual) | ✓ | ✓ |
| All existing env improved | ✓ | ✓ + height bands |
| primary sensible | Player | camera/water/ground |
| PNG non-empty + opened | ✓ | ✓ |
| Baseline once | ✓ | ✓ |

**R3 exit:** Matrix green or R3.5 fix loop until green.

### R3.5 Fix loop

- [x] Fix missing Names, filter score 0, flat-only IF, Menu-only IF, black_frame false positive, unreferenced assets.  
- [x] Re-run only failed game.  
- [x] Do not declare done with stub improves or missing new quotas.  

---

## 7. Phase R4 — Docs and closeout

- [x] Flip checkboxes in this file when truly done.  
- [x] Update [PROGRESS.md](../PROGRESS.md).  
- [x] Write `docs/AGENT_SIGHT_DEBT_FINDINGS_YYYY-MM-DD.md` (features, every asset path touched, packets, height notes, assessments).  
- [x] Skill `bevy-agent-loop` / eyesight-packs: debt fixes, Name onboarding rule, dual-port sequential note.  
- [x] Link from ROADMAP / AGENTS.  

---

## 8. Success metrics (definition of done)

1. **R0** shipped + **MCP rebuilt** before final dogfood evidence.  
2. **Crystal Drift:** 1 new feature+asset, 1 new env, **all** listed sprites improved, env improved, live verify pass.  
3. **Iron Feud:** 1 new feature+asset, 1 new env, **all** listed tints improved, env+height improved, Playing verify pass.  
4. Live eyesight both games: non-empty PNGs, open/read notes, logs under `{SCRATCH}`.  
5. Unit tests for new pure helpers PASS; `cargo test -p specula -p specula-brp` PASS.  
6. Debt R1–R4 addressed in code or deferred with written reason in findings.  
7. **No exclusions** implemented.  
8. Findings + assessments written; plan checkboxes complete.  
9. **No shortcuts** on “improve all” — each inventory row has a real asset replace + live observation path.

---

## 9. Suggested names (non-binding)

| Game | New feature | New env |
|------|-------------|---------|
| CD | `SolarFlareBuoy` or `IceShardField` | `WarpGateRing` or `DustCloudBelt` |
| IF | `RadarDome` or `FuelTankStack` | `TerrainSaddle` or `MistBasin` |

Implementer may rename but **must** meet counts and the full improve inventory.

---

## 10. Risks

| Risk | Mitigation |
|------|------------|
| “Improve all” scope explosion | Inventory tables are the checklist; one asset at a time; live fovea proof |
| Height changes break placement | Keep start 0..10 flat; unit test reserved cells |
| New Names filtered | R0 hints + tests before dogfood green |
| Stale MCP | install --force before R3 pass |
| CD/IF not in Specula git | Edit external paths; commit only Specula in this repo |
| Tiny procedural sprites history | Prefer real regenerated art (game-asset-core); never leave 100-byte placeholders |
| Alt views still similar | Side nudge + honest views_similar; document residual |

---

## 11. `/goal` paste template

```text
Execute docs/AGENT_SIGHT_DEBT_PLAN.md to completion (R0 → R4).
I will be away — take as long as needed; NO shortcuts;
long-term correctness over speed. Prefer durable tests, live captures, honest logs.

Order: Specula residual sight debt first (rebuild install), then dogfood
  2D: /Users/aron/Documents/coding_projects/Crystal Drift
  3D: /Users/aron/Documents/coding_projects/Iron Feud
Each game: 1 new feature+asset; 1 new environment; improve ALL existing assets
  and ALL existing environments (inventory tables in plan).
IF: keep 3+ height bands; start playable. Live see verify + packs both; fix until green.
Exclude: livestream/60fps, human editor, unlit suite, auto taste scoring.
Bevy 0.19, remote,capture, BRP 15702. Iron Feud: IRON_FEUD_AUTO_PLAY=1.
Update plan checkboxes + PROGRESS + findings. Taste/design human-owned; agent sight only.
```

---

## 12. Document history

| Date | Change |
|------|--------|
| 2026-07-22 | Initial residual sight debt plan; full CD/IF asset+env improve inventories; one new feature + one new env per game |
| 2026-07-23 | R0–R4 complete: residual sight helpers; CD SolarFlareBuoy+WarpGateRing; IF RadarDome+TerrainSaddle; full asset improve; findings |
