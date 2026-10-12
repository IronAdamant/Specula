# Agent Sight Fidelity Plan — Tier 1→3 + craft-quality dogfood (next cycle)

**Status:** complete (F0–F6 shipped 2026-07-23)  
**Audience:** implementing agent under `/goal`; human may be away for a long session  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**Focus:** raise **craft fidelity** so agent sight judges **recognizable product features**, not block placeholders — dogfood **Crystal Drift** and **Iron Feud** with **exactly one new feature each** and a **full improve-all-existing-features** pass. Existing craft must leave “block soup”: drills read as drills, belts as belts; CD sprites have **true transparent backgrounds** (no purple/magenta square plates); IF terrain is **randomized varying height**, not a flat slab world.

### Long-session / no-shortcuts mandate

- Take as long as needed; **correctness over speed**.  
- **No shortcuts:** no stub meshes, no magenta/purple square plates on 2D sprites, no “rename-only” improves, no synthetic PNGs as pass proof, no skip of live verify when GPU/window is available.  
- Rebuild MCP (`cargo install --path crates/specula --force`) **before** treating dogfood captures as pass evidence whenever Specula sight code changes.  
- Taste/design remain **human-owned**; agent **sees and builds** to this plan’s quotas.  
- **Commit** CD/IF craft in their sibling git repos (already versioned under hardening H1).

### Parent / prior work (shipped)

| Doc | Role |
|-----|------|
| [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) | V0–V6 eyes open |
| [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md) | A0–A8 acuity |
| [AGENT_SIGHT_NEXT_PLAN.md](AGENT_SIGHT_NEXT_PLAN.md) | S0–S4 ranking, profiles, verify |
| [AGENT_SIGHT_2D3D_PLAN.md](AGENT_SIGHT_2D3D_PLAN.md) | D0–D5 2D/3D packs, height bands |
| [AGENT_SIGHT_DEBT_PLAN.md](AGENT_SIGHT_DEBT_PLAN.md) | R0–R4 residual filter/nudge + inventory improve |
| [AGENT_SIGHT_HARDENING_PLAN.md](AGENT_SIGHT_HARDENING_PLAN.md) | H0–H6 complete: pixel gates, side-orbit, PulseMine, LoadingBay, heightfield |
| [AGENT_SIGHT_HARDENING_FINDINGS_2026-07-23.md](AGENT_SIGHT_HARDENING_FINDINGS_2026-07-23.md) | Residuals this plan closes |

### Dogfood trees (required)

| Dimension | Path | Launch notes |
|-----------|------|----------------|
| **2D** | `/Users/aron/Documents/coding_projects/Crystal Drift` | Playing by default; features `remote,capture` |
| **3D** | `/Users/aron/Documents/coding_projects/Iron Feud` | **`IRON_FEUD_AUTO_PLAY=1`**; features `remote,capture` |

Pins: Bevy **0.19**, BRP **15702**, `remote,capture`. Skills: `bevy-agent-loop`, eyesight-packs.  
Versioning: sibling git repos — see [DOGFOOD_REPOS.md](DOGFOOD_REPOS.md).

---

## 1. North star

> **Agent sight is craft-honest at product identity level:** every **existing** gameplay feature on CD/IF is upgraded so silhouettes match intent (not blocks/plates); each game ships **exactly one new Named feature** beyond hardening; CD sprites are **opaque subject + fully transparent BG** so env and other 2D assets show through; IF ground uses **randomized continuous height variation** (playable start pocket kept placeable); platform residual multi-view / gate honesty is closed enough that live packets and opened PNGs agree.

### Why a new cycle (not re-open hardening)

Hardening shipped gates + first complex-craft pass. Residuals and human review still require a **second fidelity pass**:

| Residual / human note | Fidelity answer |
|----------------------|-----------------|
| Strategy multi-view still weak at high Y | Tier 1: dedicated side/orbit camera entity or stronger honest path |
| Not every IF `MachineKind` equally re-sculpted | F2: **all** machine kinds to multi-part recognizable craft |
| Earlier dogfood showed **purple squares** behind sprites | F1: full inventory re-audit; zero true-magenta; zero opaque square plates |
| Heightfield exists but must stay the primary ground story | F2: relief visible in open landscape PNGs; no slab-dominated world |
| Need another product dogfood proof after PulseMine / LoadingBay | F1–F2: **one new feature each** + improve all existing |

### Exclusions (do not implement)

| Excluded | Why |
|----------|-----|
| 60 FPS / livestream / continuous video | Deferred; stills + short strips only |
| Human editor / hierarchy UI / gizmos | Out of scope |
| Full unlit material suite | Later optional (Tier 3 note only) |
| Auto taste / beauty scoring | Human-owned |
| Parallel dual-game BRP on same port without dual-port setup | Sequential default; Tier 3 dual-port is optional recipe |

---

## 2. Problem split

| Gap | Evidence | Fidelity answer |
|-----|----------|-----------------|
| **Block / plate craft still reads as placeholders** | Cuboid leftovers; opaque sprite cards | F1–F2: complex shapes + transparent BG law |
| **Purple / magenta squares** | Prior Imagine key failures; human-noticed purple BG | Tier 1 + F1: true-magenta gate 0 on every sprite |
| **Env hidden by sprite plates** | 2D assets should show through transparent BG | F1: alpha audit + open full frame |
| **IF terrain flat or slab-only** | Pre-heightfield / landmark-slab dominance | F2: continuous randomized heightfield primary |
| **Multi-view identity** | High strategy cam → similar game/alt | Tier 1: dedicated side cam or honest `views_similar` only |
| **Subject / Name filter** | New Names score 0 → invisible in packets | Tier 1: stem hints + unit tests for new Names |
| **Baseline habit** | Easy to skip after craft change | Tier 2: save + compare once per game |
| **Dual live games** | Port 15702 fights | Tier 3: dual-port recipe only |

---

## 3. Execution order (hard)

```text
F0  Specula residual fidelity (multi-view camera path polish if needed;
    Name stems for planned features; gate docs/tests still green)
    → cargo test -p specula -p specula-brp
    → cargo build -p specula
    → cargo install --path crates/specula --force  (if platform code changed)
    → log surface → {SCRATCH}/mcp-surface.log

F1  Crystal Drift craft pass: 1 NEW feature + improve ALL existing features/sprites
    → transparent BG only; no purple/magenta plates; env shows through; live verify

F2  Iron Feud craft pass: 1 NEW feature + improve ALL existing machine/prop meshes
    → complex silhouettes (drills, belts, furnaces, inserters, poles, … not blocks)
    → randomized varying terrain height (heightfield primary; start pocket placeable)
    → IRON_FEUD_AUTO_PLAY=1 live verify + landscape

F3  Sequential live review both games (current MCP); pixel gates green; open PNGs
F4  Fix loop until green (no block drills, no purple sprite plates, no flat-only world)
F5  Docs: checkboxes, PROGRESS, findings+assessments; skill/ROADMAP/AGENTS
F6  Commit Specula + CD + IF sibling repos
```

**Never treat F1–F3 captures as pass evidence on a stale MCP if F0 changed sight code.**  
**F0 may be light if no platform gaps remain — still re-run tests and document.**

---

## 4. Tier map (explicit)

| Tier | Items | Phase |
|------|-------|-------|
| **Tier 1** | Pixel/magenta gates enforced on dogfood; Name stem onboarding; multi-view honesty (side/orbit or dedicated camera); transparent-BG discipline; sibling-git craft commits | F0, F1–F3 gates |
| **Tier 2** | Baseline save→compare once per game after craft; fovea/entity packs on complex craft; subject quality (primary not OreCrystal*) | F0 surface, F3 |
| **Tier 3** | Dual-port sequential recipe documented; unlit/taste/livestream stay **out** | F5 docs only |

Craft dogfood (F1–F2) is **in addition** to tiers — it is the product proof that sight can judge **real feature identity**, not just Name lists.

---

## 5. Phase F0 — Specula Tier 1 residual (platform)

Implement only what residual multi-view / gates still need. Prefer pure, unit-tested helpers.

### F0.1 Multi-view honesty (Tier 1)

- [x] Confirm BRP camera nudge uses **`[x,y,z]` array** translation (hardening fix); unit tests still pass.  
- [x] When landscape alt remains similar under high strategy camera: prefer a **temporary dedicated side/orbit camera entity** (or larger documented offset) so open PNGs differ **or** leave only honest `views_similar` (no fake multi-angle claims).  
- [x] Unit-test pure placement math if new helpers land.

### F0.2 Pixel / craft gates (Tier 1)

- [x] Reconfirm `png_nonblack_fraction`, `png_true_magenta_pixel_count`, `scripts/check_sprite_transparency.py`.  
- [x] Document thresholds in skill: CD full-frame nonblack ≥ practical threshold in Playing; **true-magenta ≤ 0** on every `assets/sprites/*.png`.  
- [x] Unit tests green for black / magenta plate / clean transparent cases.

### F0.3 Name onboarding (Tier 1)

- [x] Extend `DOGFOOD_NAME_STEMS` / gameplay hints for F1–F2 planned Names (score > 0).  
- [x] Unit tests for new stems.

### F0.4 Surface

- [x] MCP/CLI instructions still state transparent-BG law, complex-mesh dogfood, pixel gates.  
- [x] `cargo test -p specula -p specula-brp` PASS; rebuild/install if code changed; `{SCRATCH}/mcp-surface.log`.

**F0 exit:** Tests green; no exclusions; install current if platform changed.

---

## 6. Phase F1 — Crystal Drift: 1 new feature + improve all features

**Path:** `/Users/aron/Documents/coding_projects/Crystal Drift`

### Quotas

| Quota | Requirement |
|-------|-------------|
| **1 new feature + asset** | Exactly one new gameplay-relevant **Named** entity + new disk sprite under `assets/sprites/`, spawned in Playing, with a system hook (collect / combat / hazard / score) |
| **Improve ALL existing features** | Every gameplay/visual feature that uses a sprite or Named prop is upgraded so craft is **recognizable** (not a colored blob or opaque plate) |
| **Transparent background law** | Every `assets/sprites/*.png`: subject opaque, **background fully transparent**; **zero true-magenta / purple square plates** (human-noticed failure mode) |
| **Env shows through** | Full frame must show env (nebulas, WarpGate, station, debris) **and** gameplay sprites with correct alpha — no opaque square cards hiding the scene |

### Suggested new feature (non-binding — pick one)

| Name | Role |
|------|------|
| `RepairDrone` | Ally/auto drone with distinct silhouette + heal or score hook |
| `IonTurret` | Stationary defense with clear barrel + base |
| `CargoPod` | Collectible multi-part cargo with score/weight hook |

Implementer may rename; count is exact (**one** new; do **not** re-count `PulseMine`).

### Existing features / sprites to improve (checklist)

Each row: regenerate or replace PNG + keep path stable when possible + referenced in load/spawn.  
**Bar:** complex, readable silhouette; transparent BG; env visible around/through alpha.

| Asset / feature | Improve bar |
|-----------------|-------------|
| Player ship | Clear thruster + cockpit; transparent BG |
| Asteroid L/M/S | Distinct size classes; cratered rock; no plate |
| Crystal | Faceted gem; transparent BG |
| Fuel canister | Can silhouette |
| Enemy scout | Hostile craft vs player |
| Scrap | Metal shards |
| Shield orb | Soft sphere, transparent outside |
| Boost flame | Trail tongue |
| Beacon buoy | Pole + light |
| Rescue pod | Capsule |
| CometFragment | Ice + tail |
| SignalSat | Body + panels + dish |
| SolarFlareBuoy | Energy buoy (no residual plate) |
| PulseMine | Keep identity; polish silhouette if still plate-ish |
| Nebula | Soft gas, transparent edges (purple **craft** OK; square plate **not** OK) |
| Station | Modular mass |
| DebrisRing | Ring structure |
| WarpGateRing | Portal ring; transparent center/BG |

### Systems / env

- [x] New feature: components + spawn + system + `Name` + GAMEPLAY_NAME_HINTS / stems.  
- [x] All features re-checked for on-screen start composition where practical.  
- [x] Env still readable on full (pixel gate: nonblack fraction / region luminance).  
- [x] Build `remote,capture` → `{SCRATCH}/crystal-drift-build.log` PASS.  
- [x] Sprite audit: `check_sprite_transparency.py` (or equivalent) → **true-magenta count 0** for all inventory PNGs.  
- [x] Commit in Crystal Drift sibling repo.

### Eyesight (F3)

- `primary_subject=Player`  
- New feature Name in subjects  
- Open full + fovea: **no purple squares**; env visible; craft silhouettes readable  
- Log `{SCRATCH}/dogfood-see-2d.log`; copy `{SCRATCH}/eyesight/cd/`  

---

## 7. Phase F2 — Iron Feud: 1 new feature + improve all features + heightfield

**Path:** `/Users/aron/Documents/coding_projects/Iron Feud`  
**Playing:** `IRON_FEUD_AUTO_PLAY=1` mandatory  

### Quotas

| Quota | Requirement |
|-------|-------------|
| **1 new feature + asset** | Exactly one new Named gameplay/prop feature + mesh/tint asset, visible in Playing |
| **Improve ALL existing features** | Every machine kind mesh + env props upgraded from **block soup** to **complex, recognizable silhouettes** |
| **Terrain** | Ground is **not** a single flat plane or only a few flat slabs: **randomized continuous (or dense sample) height variation** across the playable landscape, with **start factory pocket kept placeable/flat enough** |
| **Belts / drills / etc.** | Transport belts read as **belts** (deck + roller/link suggestion, direction cue); mining drills read as **drills** (tower + bit + chassis, not a brown cube stack only) |

### Suggested new feature (non-binding — pick one)

| Name | Role |
|------|------|
| `PipeJunction` | Multi-part industrial pipe hub + Name |
| `OreCrusher` | Multi-part crusher with jaw/hopper silhouette |
| `SignalRelay` | Tall antenna/relay distinct from LoadingBay |

Implementer may rename; count is exact (**one** new; do **not** re-count `LoadingBay`).

### Machine / prop craft inventory (must improve)

For each `MachineKind` spawn path in mesh code (and env props in spawn), multi-part geometry with readable silhouette under strategy camera:

| Feature | Minimum craft bar (not single cuboid) |
|---------|----------------------------------------|
| Burner / Electric mining drill | Chassis + tower + bit + optional cab/treads |
| Yellow transport belt | Long deck + segment/roller cues + direction chevrons |
| Burner / Electric inserter | Base + arm + grabber |
| Stone furnace | Body + chimney + mouth opening |
| Offshore pump | Pipe riser + intake + platform |
| Boiler | Tank body + firebox + pipe stubs |
| Steam engine | Housing + flywheel/turbine cue |
| Wooden / Iron chest | Lid seam + feet or band |
| Small electric pole | Pole + crossarm + insulator cues |
| Solar panel | Frame + panel face + stand |
| Accumulator | Body + terminal posts |
| Assembling machine | Housing + rotor/arm cue |
| LoadingBay | Keep identity; polish if still blocky |
| Env props | Rock, tree, cliff, ash, scrap, Relay, Supply, WatchPost, OreSilo, RadarDome, TerrainSaddle — non-cube silhouettes |
| Ore patches | Crystals readable |
| Water | Clear surface vs raised land edge |

### Terrain (non-negotiable craft)

- [x] Height-varying mesh remains **primary** Ground (heightfield / multi-vertex); not slab-only landmarks.  
- [x] Heights **randomized** (seeded RNG OK for reproducible tests).  
- [x] Continuous range flat → mid → high measurable (vertex Y stats or samples).  
- [x] **Start cells (~0..10)** placeable (flat or gently sloped; document reserved pocket).  
- [x] Unit tests: height variance + **upward (+Y) winding** so landscape is not black void.  
- [x] Landscape pack: relief **visible** in opened PNGs.  

### Systems

- [x] New feature spawn + Name + hints.  
- [x] Mesh code modular; keep factory sim playable.  
- [x] Build log `{SCRATCH}/iron-feud-build.log` PASS.  
- [x] Commit in Iron Feud sibling repo.

### Eyesight (F3)

- `app_state=Playing`; primary ∈ {StrategyCamera, WaterBody, Ground} — not OreCrystal*  
- New feature Name present  
- Open landscape: **non-blocky** machines readable as intended types; terrain relief not flat-slab-only  
- Multi-view: game≠alt after side/orbit path **or** honest `views_similar` only  
- Log `{SCRATCH}/dogfood-see-3d.log`; `{SCRATCH}/eyesight/if/`  

---

## 8. Phase F3 — Live review (both games, current MCP)

Sequential port **15702**.

### F3.1 Crystal Drift

```text
1. killall crystal_drift iron_feud
2. Launch CD (warm binary, remote,capture)
3. specula brp wait --port 15702
4. specula see verify --profile crystal-drift --out-dir <CD> --save-baseline ...
5. pack env_2d | hud | landscape
6. entity Player + new feature Name
7. Sprite audit + full-frame nonblack / magenta gates
8. OPEN full + fovea: no purple squares; env shows through; craft readable
9. Log → {SCRATCH}/dogfood-see-2d.log; copy eyesight/cd/
```

### F3.2 Iron Feud

```text
1. Stop CD
2. IRON_FEUD_AUTO_PLAY=1 launch iron_feud
3. brp wait
4. see verify --profile iron-feud --save-baseline ...
5. pack landscape + water; entity on drill / belt / new prop
6. Assert Playing; height variance visible; machines non-blocky in open images
7. If views_similar, side/orbit path should differ OR honest warning only
8. Log → {SCRATCH}/dogfood-see-3d.log; eyesight/if/
```

### F3.3 Review matrix

| Check | CD | IF |
|-------|----|----|
| Build remote,capture | ✓ | ✓ |
| Sibling git commit after craft | ✓ | ✓ |
| 1 new feature Name (beyond hardening) | ✓ | ✓ |
| All existing features craft-improved | ✓ sprites | ✓ meshes |
| Transparent BG / no magenta/purple plates | ✓ | n/a (3D) |
| Env / terrain readable | ✓ through alpha | ✓ randomized height |
| primary sensible | Player | camera/water/ground |
| Pixel gates | ✓ | ✓ landscape relief note |
| PNG opened | ✓ | ✓ |
| Baseline once | ✓ | ✓ |

---

## 9. Phase F4 — Fix loop

- [x] Fix magenta/purple plates, invisible env, blocky machines, flat-only terrain, filter score 0, Menu-only IF, black frame / -Y winding void, silent BRP mutate.  
- [x] Re-run only failed game.  
- [x] **Do not declare done** with cuboid drills, opaque purple square sprites, or slab-only ground.

---

## 10. Phase F5 — Docs and closeout

- [x] Flip checkboxes in this file when truly done.  
- [x] Update [PROGRESS.md](../PROGRESS.md).  
- [x] Write `docs/AGENT_SIGHT_FIDELITY_FINDINGS_YYYY-MM-DD.md` (Tier items, craft inventories, packet paths, height description, assessments).  
- [x] Skill + eyesight-packs: reaffirm transparent-BG law, complex-mesh dogfood, pixel gates, multi-view honesty.  
- [x] Link ROADMAP / AGENTS.  

---

## 11. Phase F6 — Git

- [x] Specula: commit platform + docs (no force-push).  
- [x] Crystal Drift: commit new feature + sprite inventory.  
- [x] Iron Feud: commit new feature + mesh/terrain pass.  

---

## 12. Success metrics (definition of done)

1. **F0** residual platform solid; MCP current if code changed.  
2. **CD:** 1 new feature beyond PulseMine; **all** sprites transparent-BG; no true-magenta/purple plates; env+craft visible on full; verify pass.  
3. **IF:** 1 new feature beyond LoadingBay; **all** machine/prop kinds non-blocky recognizable craft; randomized varying terrain height; start playable; Playing verify pass.  
4. Live eyesight both games: non-empty PNGs, open/read observations, logs under `{SCRATCH}`.  
5. Unit tests for pure helpers PASS; `cargo test -p specula -p specula-brp` PASS.  
6. **No exclusions** implemented.  
7. Findings + assessments; plan checkboxes complete.  
8. **No shortcuts** on complex shapes, transparent BG, or flat terrain.

---

## 13. Risks

| Risk | Mitigation |
|------|------------|
| Complex meshes break placement / performance | Keep footprints; LOD not required; start flat pocket |
| Heightfield winding flips to black void | Keep +Y winding unit test; open landscape PNG |
| “Recognizable” is subjective | Checklist silhouettes + open fovea/full notes; human taste owns beauty |
| Magenta/purple returns via Imagine | Prefer code-built or carefully keyed assets; automated magenta audit |
| Side camera BRP mutate flaky | Array translation; surface failures; dedicated camera entity; honest views_similar |
| Scope of improve-all machines | Work machine-by-machine; do not ship half the catalog as cubes |
| Double-counting PulseMine/LoadingBay | New Names only; improve-all includes polishing prior features |

---

## 14. `/goal` paste template

```text
Execute docs/AGENT_SIGHT_FIDELITY_PLAN.md to completion (F0 → F6).
I will be away — take as long as needed; NO shortcuts;
long-term correctness over speed. Prefer durable tests, live captures, honest logs.

Order: Specula residual platform first (rebuild install if code changed),
then dogfood craft:
  2D: /Users/aron/Documents/coding_projects/Crystal Drift
  3D: /Users/aron/Documents/coding_projects/Iron Feud
Each game: exactly 1 NEW feature (beyond PulseMine / LoadingBay);
improve ALL existing features to complex non-block silhouettes.
CD: transparent sprite BGs only — no purple/magenta square plates; env shows through.
IF: drills look like drills, belts like belts; all MachineKinds multi-part;
    randomized varying terrain heights; start pocket placeable; +Y heightfield winding.
Live see verify + packs both; pixel gates; open PNGs; fix until green.
Exclude: livestream/60fps, human editor, unlit suite, auto taste scoring.
Bevy 0.19, remote,capture, BRP 15702. Iron Feud: IRON_FEUD_AUTO_PLAY=1.
Update plan checkboxes + PROGRESS + findings. Taste/design human-owned; agent sight only.
```

---

## 15. Document history

| Date | Change |
|------|--------|
| 2026-07-23 | F0–F6 complete: stems, CargoPod, OreCrusher, heightfield, findings |
| 2026-07-23 | Initial fidelity plan: Tier 1–3 + CD/IF craft dogfood cycle after hardening (1 new feature each, improve all, complex shapes, transparent BG, randomized IF height) |
