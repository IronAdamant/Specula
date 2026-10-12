# Agent Sight Hardening Plan — Tier 1→3 + craft-quality dogfood

**Status:** complete (H0–H6 shipped 2026-07-23)  
**Audience:** implementing agent under `/goal`; human may be away for a long session  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**Focus:** make sight **good enough for hands-off factory loops** (versioned dogfood, pixel gates, true multi-view honesty), then dogfood **Crystal Drift** and **Iron Feud** with **exactly one new feature each** and a **full improve-all-existing-features** pass that kills blocky/placeholder craft.

### Long-session / no-shortcuts mandate

- Take as long as needed; **correctness over speed**.  
- **No shortcuts:** no stub meshes, no magenta/purple square plates on 2D sprites, no “rename-only” improves, no synthetic PNGs as pass proof, no skip of live verify when GPU/window is available.  
- Rebuild MCP (`cargo install --path crates/specula --force`) **before** treating dogfood captures as pass evidence.  
- Taste/design remain **human-owned**; agent **sees and builds** to this plan’s quotas.  
- **Version dogfood trees** (Tier 1) so CD/IF changes are durable — not disk-only ghosts.

### Parent / prior work (shipped)

| Doc | Role |
|-----|------|
| [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) | V0–V6 eyes open |
| [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md) | A0–A8 acuity |
| [AGENT_SIGHT_NEXT_PLAN.md](AGENT_SIGHT_NEXT_PLAN.md) | S0–S4 ranking, profiles, verify |
| [AGENT_SIGHT_2D3D_PLAN.md](AGENT_SIGHT_2D3D_PLAN.md) | D0–D5 2D/3D packs, height bands |
| [AGENT_SIGHT_DEBT_PLAN.md](AGENT_SIGHT_DEBT_PLAN.md) | R0–R4 residual filter/nudge + inventory improve |
| [AGENT_SIGHT_DEBT_FINDINGS_2026-07-23.md](AGENT_SIGHT_DEBT_FINDINGS_2026-07-23.md) | Magenta plates, env black neighborhoods, hash-only views_similar |

### Dogfood trees (required)

| Dimension | Path | Launch notes |
|-----------|------|----------------|
| **2D** | `/Users/aron/Documents/coding_projects/Crystal Drift` | Playing by default; features `remote,capture` |
| **3D** | `/Users/aron/Documents/coding_projects/Iron Feud` | **`IRON_FEUD_AUTO_PLAY=1`**; features `remote,capture` |

Pins: Bevy **0.19**, BRP **15702**, `remote,capture`. Skills: `bevy-agent-loop`, eyesight-packs.

---

## 1. North star

> **Agent sight is durable and craft-honest:** dogfood is versioned; pixel gates catch invisible env and magenta plates; multi-view either differs or warns honestly; **every existing gameplay feature** on CD/IF is upgraded from blocks/placeholder art to **recognizable complex silhouettes** (drills look like drills, belts look like belts, 2D sprites are transparent-BG and read on full frame); each game ships **exactly one new Named feature**; IF terrain uses **randomized continuous height variation** (not only a few flat slabs).

### Exclusions (do not implement)

| Excluded | Why |
|----------|-----|
| 60 FPS / livestream / continuous video | Deferred; stills + short strips only |
| Human editor / hierarchy UI / gizmos | Out of scope |
| Full unlit material suite | Later optional |
| Auto taste / beauty scoring | Human-owned |
| Parallel dual-game BRP on same port without dual-port setup | Sequential default; Tier 3 dual-port is optional recipe |

---

## 2. Problem split (why this plan exists)

| Gap | Evidence from prior dogfood | Hardening answer |
|-----|----------------------------|------------------|
| **CD/IF not in git** | Sight claims on disk-only trees | Tier 1: version dogfood |
| **Names ≠ pixels** | Env Named/on_screen but full frame black | Tier 1: pixel gates |
| **Magenta / purple squares** | Imagine key failures on pickups | Tier 1 + H2: transparent-BG audit; zero true-magenta plates |
| **Blocky 3D machines** | Cuboid drills/belts/props | H2: complex multi-part silhouettes |
| **IF height “bands only”** | Discrete slabs; still reads as terraced blocks | H2: randomized heightfield / continuous relief |
| **Multi-view fake** | mean_abs ~0 with strategy camera | Tier 1: true side camera when similar |
| **Baseline habit optional** | Easy to skip | Tier 2: scripted baseline once per game |
| **Dual live games** | Port 15702 fights | Tier 3: dual-port recipe only |

---

## 3. Execution order (hard)

```text
H0  Specula sight hardening (pixel helpers, side-view pack path, sprite audit hooks) + unit tests
    → cargo test -p specula -p specula-brp
    → cargo build -p specula
    → cargo install --path crates/specula --force
    → log surface → {SCRATCH}/mcp-surface.log

H1  Version dogfood trees (git init + initial commit OR document submodule layout)
    → both CD and IF have .git and a commit containing current + post-H2 tree

H2  Crystal Drift craft pass: 1 new feature + improve ALL existing features/sprites
    → transparent BG only; no purple/magenta plates; live verify env+craft

H3  Iron Feud craft pass: 1 new feature + improve ALL existing machine/prop meshes
    → complex silhouettes (not block soup); randomized varying terrain height
    → IRON_FEUD_AUTO_PLAY=1 live verify + landscape

H4  Sequential live review both games (new MCP); pixel gates green; open PNGs
H5  Fix loop until green
H6  Docs: checkboxes, PROGRESS, findings+assessments; skill/ROADMAP
```

**Never treat H2–H4 captures as pass evidence on a stale MCP.**  
**H1 may run before H2 if cleaner, but H0 must complete first.**

---

## 4. Phase H0 — Specula Tier 1–2 sight code (platform)

Implement pure, unit-tested helpers where possible.

### H0.1 Pixel / craft gates (Tier 1.3–1.4)

- [x] Pure helper(s) e.g. `png_nonblack_fraction`, `png_true_magenta_pixel_count` (strict: high R+B, very low G — not purple craft).  
- [x] Optional CLI or script under `scripts/` or game trees: `check_sprite_transparency.py` / eyesight gate using shipped decode path.  
- [x] Unit tests: solid black → low nonblack; magenta plate → magenta count >0; clean transparent craft → magenta 0.  
- [x] Document gate thresholds in skill (e.g. full-frame nonblack ≥ threshold for CD Playing; true-magenta ≤ 0 on sprites).

### H0.2 True multi-view when similar (Tier 1.2)

- [x] When landscape/water alt fails perceptual differ (`captures_look_similar`), **spawn or nudge a temporary side/orbit camera** (or second capture path) so alt is a real lateral angle — **or** leave only the warning and do not claim multi-angle (prefer implement side path).  
- [x] Keep `views_similar` if even side path matches.  
- [x] Unit-test pure camera placement math if extracted.

### H0.3 Name onboarding + baseline (Tier 2.5–2.6)

- [x] Extend `DOGFOOD_NAME_STEMS` / hints for H2–H3 planned Names.  
- [x] Skill: after craft change, `save_baseline` → `compare_baseline` once per game in H4.  
- [x] Optional: `see verify --save-baseline` already exists — document mandatory use in H4 checklist.

### H0.4 Surface

- [x] MCP/CLI instructions mention pixel gates + transparent-BG rule + complex-mesh dogfood.  
- [x] `cargo test` + `cargo build` PASS; install `--force`; `{SCRATCH}/mcp-surface.log`.

**H0 exit:** Tests green; binary rebuilt; no exclusions.

---

## 5. Phase H1 — Version dogfood (Tier 1.1)

- [x] `/Users/aron/Documents/coding_projects/Crystal Drift` has `.git` (init if missing) and at least one commit after H2 (or commit pre+post).  
- [x] `/Users/aron/Documents/coding_projects/Iron Feud` same.  
- [x] Specula docs note how dogfood is versioned (sibling repos; not necessarily monorepo).  
- [x] Optional: `.gitignore` for `target/`, `captures/` (keep intentional eyesight samples if desired).  
- [x] **Do not** force-push dogfood history unless human asks.

**H1 exit:** Both trees are recoverable git projects; H2/H3 changes can be committed there.

---

## 6. Phase H2 — Crystal Drift: 1 new feature + improve all features

**Path:** `/Users/aron/Documents/coding_projects/Crystal Drift`

### Quotas

| Quota | Requirement |
|-------|-------------|
| **1 new feature + asset** | Exactly one new gameplay-relevant **Named** entity + new disk sprite under `assets/sprites/`, spawned in Playing, system hook if collect/combat |
| **Improve ALL existing features** | Every gameplay/visual feature that uses a sprite or Named prop is upgraded so craft is **recognizable** (not a colored blob or plate) |
| **Transparent background law** | Every `assets/sprites/*.png`: subject opaque, **background fully transparent**; **zero true-magenta / purple square plates** (prior dogfood failure mode) |
| **Env shows through** | Full frame must show env (nebulas, WarpGate, station, debris) **and** gameplay sprites with correct alpha — no opaque square cards |

### Suggested new feature (non-binding)

| Name | Role |
|------|------|
| `MineDrone` or `PulseMine` | Weak enemy/mine with distinct sprite + damage or score hook |

Implementer may rename; count is exact (one new).

### Existing features / sprites to improve (checklist)

Each row: regenerate or replace PNG + keep path stable + referenced in load/spawn.

| Asset / feature | Improve bar |
|-----------------|-------------|
| Player ship | Clear thruster + cockpit silhouette; transparent BG |
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
| SolarFlareBuoy | Energy buoy (fix any residual plate) |
| Nebula | Soft gas, transparent edges (purple **craft** OK; square plate **not** OK) |
| Station | Modular mass |
| DebrisRing | Ring structure |
| WarpGateRing | Portal ring; transparent center/BG |

### Systems / env

- [x] New feature: components + spawn + system + Name + GAMEPLAY_NAME_HINTS.  
- [x] All features re-checked for on-screen start composition where practical.  
- [x] Env still readable on full (pixel gate: nonblack fraction / region luminance).  
- [x] Build `remote,capture` → `{SCRATCH}/crystal-drift-build.log` PASS.  
- [x] Sprite audit: true-magenta count 0 for all inventory PNGs (script or unit).  

### Eyesight (H4)

- `primary_subject=Player`  
- New feature Name in subjects  
- Open full + fovea: **no purple squares**; env visible; craft silhouettes readable  
- Log `{SCRATCH}/dogfood-see-2d.log`; copy `{SCRATCH}/eyesight/cd/`  

---

## 7. Phase H3 — Iron Feud: 1 new feature + improve all features + heightfield

**Path:** `/Users/aron/Documents/coding_projects/Iron Feud`  
**Playing:** `IRON_FEUD_AUTO_PLAY=1` mandatory  

### Quotas

| Quota | Requirement |
|-------|-------------|
| **1 new feature + asset** | Exactly one new Named gameplay/prop feature + mesh/tint asset, visible in Playing |
| **Improve ALL existing features** | Every machine kind mesh + env props upgraded from **block soup** to **complex, recognizable silhouettes** |
| **Terrain** | Ground is **not** a single flat plane or only a few flat slabs: **randomized continuous (or dense sample) height variation** across the playable landscape, with **start factory pocket kept placeable/flat enough** |
| **Belts / drills / etc.** | Transport belts read as **belts** (deck + roller/link suggestion, direction cue); mining drills read as **drills** (tower + bit + chassis, not a brown cube stack only) |

### Suggested new feature (non-binding)

| Name | Role |
|------|------|
| `PipeJunction` or `LoadingBay` | Industrial prop with multi-part mesh + Name |

### Machine / prop craft inventory (must improve)

For each `MachineKind` spawn path in `mesh/mod.rs` (and env props in `spawn.rs`), multi-part geometry with readable silhouette under strategy camera:

| Feature | Minimum craft bar (not single cuboid) |
|---------|----------------------------------------|
| Burner / Electric mining drill | Chassis + tower + rotating/bit mass + optional cab |
| Yellow transport belt | Long deck + segment/roller cues + direction chevrons |
| Burner / Electric inserter | Base + arm + grabber (already partial — strengthen silhouette) |
| Stone furnace | Body + chimney + mouth opening |
| Offshore pump | Pipe riser + intake + platform |
| Boiler | Tank body + firebox + pipe stubs |
| Steam engine | Housing + flywheel/turbine cue |
| Wooden / Iron chest | Lid seam + feet or band |
| Small electric pole | Pole + crossarm + insulator cues |
| Solar panel | Frame + panel face + stand |
| Accumulator | Body + terminal posts |
| Assembling machine | Housing + rotor/arm cue |
| Env props | Rock, tree, cliff, ash, scrap, Relay, Supply, WatchPost, OreSilo, RadarDome, TerrainSaddle — non-cube silhouettes where currently blocky |
| Ore patches | Crystals readable (already multi — keep/improve) |
| Water | Clear surface vs raised land edge |

### Terrain (non-negotiable craft)

- [x] Replace or augment pure flat `Ground` plane + few height slabs with a **height-varying mesh** (heightfield, multi-vertex mesh, or dense multi-sample heights).  
- [x] Heights **randomized** (seeded RNG OK for reproducibility in tests).  
- [x] **≥ continuous range** from flat → mid → high still measurable (samples or vertex Y stats).  
- [x] **Start cells (~0..10)** remain placeable (flat or gently sloped; document reserved cells).  
- [x] Unit test: height variance / band spread still holds (`height_bands_are_distinct` updated or replaced with variance assertion).  
- [x] Landscape pack: relief **visible** in opened PNGs.  

### Systems

- [x] New feature spawn + Name + hints.  
- [x] Mesh code modular; keep factory sim playable.  
- [x] Build log `{SCRATCH}/iron-feud-build.log` PASS.  

### Eyesight (H4)

- `app_state=Playing`; primary ∈ {StrategyCamera, WaterBody, Ground} — not OreCrystal*  
- New feature Name present  
- Open landscape: **non-blocky** machines readable as intended types; terrain relief not a flat slab-only world  
- Log `{SCRATCH}/dogfood-see-3d.log`; `{SCRATCH}/eyesight/if/`  

---

## 8. Phase H4 — Live review (both games, new MCP)

Sequential port **15702**.

### H4.1 Crystal Drift

```text
1. killall crystal_drift iron_feud
2. Launch CD (warm binary, remote,capture)
3. specula brp wait --port 15702
4. specula see verify --profile crystal-drift --out-dir <CD> --save-baseline ...
5. pack env_2d | hud | landscape
6. entity Player + new feature Name
7. Sprite audit + full-frame nonblack / region luminance gates
8. OPEN full + fovea: no purple squares; env shows; craft readable
9. Log → {SCRATCH}/dogfood-see-2d.log; copy eyesight/cd/
```

### H4.2 Iron Feud

```text
1. Stop CD
2. IRON_FEUD_AUTO_PLAY=1 launch iron_feud
3. brp wait
4. see verify --profile iron-feud --save-baseline ...
5. pack landscape + water; entity on drill / belt / new prop
6. Assert Playing; height variance visible; machines non-blocky in open images
7. If views_similar, side-camera path should differ OR honest warning only
8. Log → {SCRATCH}/dogfood-see-3d.log; eyesight/if/
```

### H4.3 Review matrix

| Check | CD | IF |
|-------|----|----|
| Build remote,capture | ✓ | ✓ |
| Git history exists (H1) | ✓ | ✓ |
| 1 new feature Name | ✓ | ✓ |
| All existing features craft-improved | ✓ sprites | ✓ meshes |
| Transparent BG / no magenta plates | ✓ | n/a (3D) |
| Env / terrain readable | ✓ | ✓ randomized height |
| primary sensible | Player | camera/water/ground |
| Pixel gates | ✓ | ✓ landscape relief note |
| PNG opened | ✓ | ✓ |
| Baseline once | ✓ | ✓ |

---

## 9. Phase H5 — Fix loop

- [x] Fix magenta plates, invisible env, blocky machines, flat-only terrain, filter score 0, Menu-only IF, black_frame false positive.  
- [x] Re-run only failed game.  
- [x] Do not declare done with cuboid drills or purple square sprites.  

---

## 10. Phase H6 — Docs and closeout

- [x] Flip checkboxes in this file when truly done.  
- [x] Update [PROGRESS.md](../PROGRESS.md).  
- [x] Write `docs/AGENT_SIGHT_HARDENING_FINDINGS_YYYY-MM-DD.md` (Tier items, craft inventories, packet paths, height description, assessments).  
- [x] Skill + eyesight-packs: transparent-BG law, complex-mesh dogfood, pixel gates, side-view behavior.  
- [x] Link ROADMAP / AGENTS.  
- [x] Commit Specula; commit CD/IF in their own repos.  

---

## 11. Tier map (explicit)

| Tier | Items in this plan | Phase |
|------|--------------------|-------|
| **Tier 1** | Version dogfood; true multi-view/side path; pixel gates; sprite audit / transparent-BG discipline | H0, H1, H4 gates |
| **Tier 2** | Baseline default in dogfood loop; tall/fovea polish; subject quality stress | H0.3, H4 |
| **Tier 3** | Dual-port recipe (optional); unlit suite / taste / livestream stay **out** | H6 docs only for dual-port note |

Craft dogfood (H2–H3) is **in addition** to tiers — it is the product proof that sight can judge **real feature identity**, not just Name lists.

---

## 12. Success metrics (definition of done)

1. **H0** shipped + MCP rebuilt before final dogfood evidence.  
2. **H1** both dogfood trees are git projects with commits.  
3. **CD:** 1 new feature; all sprites transparent-BG; no true-magenta plates; env+craft visible on full; verify pass.  
4. **IF:** 1 new feature; machines/props non-blocky recognizable craft; randomized varying terrain height; start playable; Playing verify pass.  
5. Live eyesight both games: non-empty PNGs, open/read observations, logs under `{SCRATCH}`.  
6. Unit tests for new pure helpers PASS; `cargo test -p specula -p specula-brp` PASS.  
7. **No exclusions** implemented.  
8. Findings + assessments; plan checkboxes complete.  
9. **No shortcuts** on complex shapes or transparent BG.  

---

## 13. Risks

| Risk | Mitigation |
|------|------------|
| Complex meshes break placement / performance | Keep footprints; LOD not required; start flat |
| Heightfield breaks factory grid | Reserved flat cells; document |
| “Recognizable” is subjective | Checklist silhouettes + open fovea/full notes; human taste still owns beauty |
| Magenta returns via Imagine | Prefer code-built or carefully keyed assets; automated magenta audit |
| Side camera BRP mutate flaky | Prefer spawn temporary camera; restore; honest views_similar fallback |
| Scope of improve-all machines | Work machine-by-machine; do not ship half the catalog as cubes |

---

## 14. `/goal` paste template

```text
Execute docs/AGENT_SIGHT_HARDENING_PLAN.md to completion (H0 → H6).
I will be away — take as long as needed; NO shortcuts;
long-term correctness over speed. Prefer durable tests, live captures, honest logs.

Order: Specula hardening first (rebuild install), version CD+IF git,
then dogfood craft:
  2D: /Users/aron/Documents/coding_projects/Crystal Drift
  3D: /Users/aron/Documents/coding_projects/Iron Feud
Each game: exactly 1 new feature; improve ALL existing features.
CD: transparent sprite BGs only — no purple/magenta square plates; env shows through.
IF: complex non-block silhouettes (drills look like drills, belts like belts);
    randomized varying terrain heights; start pocket placeable.
Live see verify + packs both; pixel gates; fix until green.
Exclude: livestream/60fps, human editor, unlit suite, auto taste scoring.
Bevy 0.19, remote,capture, BRP 15702. Iron Feud: IRON_FEUD_AUTO_PLAY=1.
Update plan checkboxes + PROGRESS + findings. Taste/design human-owned; agent sight only.
```

---

## 15. Document history

| Date | Change |
|------|--------|
| 2026-07-23 | Initial hardening plan: Tier 1–3 + CD/IF craft dogfood (1 new feature, improve all, complex shapes, transparent BG, randomized IF height) |
| 2026-07-23 | H0–H6 complete: pixel gates, side-orbit, CD PulseMine, IF LoadingBay+heightfield, findings |
