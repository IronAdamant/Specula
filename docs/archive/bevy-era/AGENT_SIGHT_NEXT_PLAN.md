# Agent Sight Next Plan — current-gen observation quality

**Status:** implemented (S0–S4; dogfood CD + IF, 2026-07-21)  
**Audience:** agents implementing Specula; humans setting goals  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**Focus:** make **agent sight** as good as current-generation models can use for Bevy game development — stills, names, framing, change, short motion strips.  
**Not taste / not design ownership:** humans own style and creative direction; the agent **sees and builds to their needs**.

### Parent / prior work (already shipped)

| Doc | Role |
|-----|------|
| [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) | V0–V6 — eyes open |
| [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md) | A0–A8 — acuity baseline |
| [AGENT_EYESIGHT_20_20_FINDINGS_2026-07-21.md](AGENT_EYESIGHT_20_20_FINDINGS_2026-07-21.md) | Dogfood lessons |

This plan is the **next increment** driven by dogfood data (noisy subjects, primary_subject wrong, menu-state false confidence, hand-tuned projection, refine loop friction).

Related skills: `.grok/skills/bevy-agent-loop/`, `references/eyesight-packs.md`  
Pins: Bevy **0.19**, BRP extras **0.22.1**, port **15702**, features **`remote,capture`**.

---

## 1. North star

> One verify pass should hand the agent **evidence it can trust**: correct play state, quiet subject list, correct primary subject, full + true fovea (+ zoom), optional multi-view only when distinct, short motion after stimulus, and before/after on refine — all via **MCP tools built and reloaded first**, then proven on real dogfood games.

### Explicit exclusions (do **not** implement in this plan)

These were deferred by product decision. Do not sneak them into PRs under this plan:

| Excluded | Reason |
|----------|--------|
| 60 FPS / livestream / continuous video | Cost; current models don’t process video well enough; humans can run the game |
| Human editor / hierarchy UI / gizmos | Out of scope; single-asset change via agent is enough when asked |
| Full unlit / deep debug material pipelines | Later date |
| Auto taste / beauty scoring | Human-owned; only if many users request later |

If a task would require any of the above, **stop and skip** — use stills, strips, and subject binding instead.

---

## 2. Problem statement (from dogfood)

| Observation | Impact on agent sight |
|-------------|------------------------|
| `primary_subject` often first Crystal / OreCrystal noise | Wrong fovea target |
| Many duplicate Names (OreCrystal0…5) | Packet unreadable |
| Iron Feud MainMenu captures look “valid” | False env claims |
| Projection needs hand-tuned `visible_half_*` | Center fallback / mis-crop |
| Diagnostic pack hardcoded Player | IF has no Player → useless center crop |
| Refine loop needs manual baseline flags | Easy to skip compare |
| Motion without stimulus often static | “Feel” claims empty |

---

## 3. Design principles

1. **Agent sight only** — every feature improves what the model can observe.  
2. **MCP first** — implement and **rebuild/reload** Specula MCP before dogfood launches.  
3. **Stills + short strips** — no video stream.  
4. **Names bind pixels** — prefer Named gameplay entities.  
5. **Fail loud on wrong state** — menu-only must not pass env verify.  
6. **Profiles over freestyle flags** — crystal-drift / iron-feud defaults.  
7. **Skill↔tool contract** — extend `see_*` and packets; no tool sprawl.  
8. **Taste stays human** — implement requirements; do not score beauty.  

---

## 4. Execution order (hard)

```text
S0  MCP/code eyesight improvements + unit tests + cargo build install path
    → reload MCP (document: cargo install --path crates/specula --force / restart client)
S1  Crystal Drift dogfood content (2 additions + 1 improvement + env mod/add)
S2  Iron Feud dogfood content (2 additions + 1 improvement + env mod/add)
S3  Live eyesight verify with NEW MCP only (both games)
S4  Docs: mark checkboxes, findings file, PROGRESS/skill touch-ups
```

**Do not dogfood with a stale MCP binary** that lacks S0 tool params. Verify `specula --version` / tool list includes new flags or instructions mention game profiles before S1–S3 captures.

---

## 5. Phase S0 — Specula agent-sight upgrades (MCP first)

Implement in `crates/specula-brp` + `crates/specula` (MCP + CLI). Unit-test pure helpers. Rebuild CLI/MCP.

### S0.1 Primary subject ranking

- [x] Pure function `rank_primary_subject(subjects) -> Option<String>`  
- [x] Priority tiers (high → low), case-sensitive Name match first, then contains:  
  1. Exact: `Player`, `MainCamera`, `StrategyCamera`, `WaterBody`, `Ground`, `DerelictStation`  
  2. Prefixes: `Nebula`, `RockOutcrop`, `TreeScrub`, `CliffRidge`, `FieldScrap`, `Asteroid`, `Shield`, `Fuel`  
  3. Other positive `gameplay_subject_score`  
  4. Never prefer: `Star*`, pure `OreCrystal*` when better tiers exist, `Menu*`  
- [x] `see_scene` sets `primary_subject` from ranker, not `subjects.first()`  
- [x] Unit tests: Crystal-like list → Player; IF-like list with crystals + WaterBody → WaterBody  

### S0.2 Collapse duplicate Names

- [x] After filter, collapse identical `name` strings into one subject + optional `count` field on `EyesightSubject` (additive JSON)  
- [x] Or packet field `subject_counts: { "OreCrystal0": 3 }` — prefer single subject with `duplicate_count: u32`  
- [x] Cap list default **24** for agent attention (was 48); still configurable  
- [x] Unit tests: 6 OreCrystal → 1 entry with count ≥ 6 or collapsed list length  

### S0.3 Game profiles (projection + wait + require_playing)

- [x] Named profiles applied by MCP/CLI `--profile crystal-drift | iron-feud | default`  
- [x] Profile table:

| Profile | projection | visible_half_w/h | require_playing | wait_for_subjects (default) |
|---------|------------|------------------|-----------------|-----------------------------|
| `crystal-drift` | ortho2d | 640 / 360 (or tuned from dogfood) | false (launches Playing) | `Player` |
| `iron-feud` | topdown3d | 20 / 20 (or retune if needed) | **true** | `StrategyCamera`, `WaterBody`, `Ground` |
| `default` | ortho2d | 640 / 360 | false | (none) |

- [x] Profile fills SeeOptions unless user overrides explicit flags  
- [x] Document in skill + MCP initialize instructions  

### S0.4 Default verify packet shape

- [x] `see_scene` (or new thin helper `see_verify` used by workflow) always ensures:  
  - full frame  
  - if `primary_subject` known: fovea crop + zoom ladder (reuse `see_entity` path)  
- [x] Packet `views` or capture notes list: `full`, `fovea`, `fovea_zoom` when present  
- [x] Optional MCP tool `bevy_see_verify` **or** skill-mandated sequence — prefer one tool if small:  
  - `bevy_see_verify` = scene + primary fovea in one call (recommended for agents)  

### S0.5 Baseline refine defaults

- [x] `see_scene --save-baseline` default path: `{out_dir}/captures/eyesight/baseline_scene.png` when flag `--save-baseline` with no path **or** profile flag `auto_baseline`  
- [x] `--compare-baseline` accepts path or default baseline path if exists  
- [x] Skill rule: after visual mutate/asset change, call compare  

### S0.6 Motion stimulus contract

- [x] `see_motion`: if no keys provided, optional `mutate_entity` + `mutate_translation` params to nudge primary/Player before strip  
- [x] Always set stimulus detail; keep `static_scene` warning when frames nearly identical  
- [x] Frames default 6–8; **no video export**  

### S0.7 Multi-view honesty

- [x] When pack alt view exists, compute mean_abs_diff or byte hash vs game view  
- [x] If nearly identical: packet warning `views_similar` (do not claim multi-angle insight)  
- [x] Keep camera-nudge approach; no editor cameras for humans  

### S0.8 Diagnostic pack fix

- [x] `pack=diagnostic` uses **ranked primary_subject** or first env allowlist name — **never hardcode Player only**  
- [x] Allowlist fallback: WaterBody, FieldScrap_A, Ground, DerelictStation, Player  

### S0.9 MCP surface update (mandatory before dogfood)

- [x] Extend tool schemas: `profile`, ranking reflected in packets, `bevy_see_verify` if added  
- [x] Update `mcp_instructions()` string with profiles + primary ranking + Playing gate  
- [x] Update CLI `specula see` with `--profile`  
- [x] Update `verify_scene` workflow plan text  
- [x] `cargo test -p specula -p specula-brp` green  
- [x] `cargo build -p specula` (and install/reload path for MCP: `cargo install --path crates/specula --force` if that’s how the host runs MCP)  
- [x] Confirm tool list includes new params before S1  

**S0 exit criteria:** Unit tests for rank/collapse/profile apply; MCP instructions mention profiles; binary rebuilt.

---

## 6. Phase S1 — Crystal Drift dogfood content

**Path:** `/Users/aron/Documents/coding_projects/Crystal Drift`  
**Features:** `remote,capture`  
**State:** Playing (default launch)

### Required content quotas (all must ship)

| Quota | Requirement | Concrete suggestion (implementer may refine names but must meet counts) |
|-------|-------------|--------------------------------------------------------------------------|
| **2 additions** | Two new tangible Named entities and/or disk assets used in play | **(1)** `BeaconBuoy` — floating nav buoy sprite + entity near arena. **(2)** `RescuePod` — collectible pod sprite; score or fuel bonus on pickup |
| **1 improvement** | Improve an existing visual or system using agent sight | Improve **Player or Crystal** presentation (scale/tint/sprite clarity) **or** HUD contrast so fovea/scene reads clearer in captures — change must be intentional and verified by before/after eyesight |
| **Environment modification** | Change existing env | Retint/reposition **nebula clouds** and/or move **DerelictStation** so composition is distinct in landscape pack |
| **Environment addition** | New env piece | **`DebrisRing`** or **`IceField`** — named backdrop/prop cluster (sprite or colored region) with `Name` for BRP |

### Implementation notes

- [x] Assets under `assets/sprites/` with paths constants in `resources.rs`  
- [x] Load in loading; spawn in gameplay_setup or systems with **Name**  
- [x] Wire any new collectibles into systems; pure sim helpers if pattern exists  
- [x] `cargo build --features remote,capture` PASS  
- [x] No dead code left from the change  

### Eyesight expectations (S3)

Subjects should include: `Player`, new additions’ Names, env Names (`Nebula*`, `DerelictStation`, new env Name).  
Primary rank should prefer **Player** over Crystal when both present.

---

## 7. Phase S2 — Iron Feud dogfood content

**Path:** `/Users/aron/Documents/coding_projects/Iron Feud`  
**Features:** `remote,capture`  
**State:** **Playing** via `IRON_FEUD_AUTO_PLAY=1` (mandatory for all env eyesight)

### Required content quotas (all must ship)

| Quota | Requirement | Concrete suggestion |
|-------|-------------|---------------------|
| **2 additions** | Two new Named entities/assets in Playing world | **(1)** `RelayTower` — tall prop (mesh + Name). **(2)** `SupplyCrate` — salvageable or placeable prop (mesh + Name); optional G-salvage or inspect text |
| **1 improvement** | Improve existing content | Improve **WaterBody** readability (tint/opacity/size) **or** FieldScrap mesh/tint so water/scrap fovea crops are clearer |
| **Environment modification** | Change existing env | Shift/scale **CliffRidge_West** and/or rock outcrops so landscape pack alt view differs |
| **Environment addition** | New env piece | **`AshPlateau`** or **`RidgeOutcrop_East`** — Named terrain prop on free grid cells (not blocking critical factory start) |

### Implementation notes

- [x] Prefer existing spawn helpers pattern (`spawn_rock_outcrop`, etc.)  
- [x] Use textures under `assets/models/` if new tints needed  
- [x] Keep water_tint wired if water touched  
- [x] `cargo build --features remote,capture` PASS  
- [x] `IRON_FEUD_AUTO_PLAY=1` still enters Playing  

### Eyesight expectations (S3)

Subjects include: `WaterBody`, `Ground`, new Names, rocks/trees/cliff/scrap as relevant.  
Primary prefers **WaterBody** or **StrategyCamera** over OreCrystal spam when ranker works.  
`require_playing` + profile `iron-feud` must not return Menu-only.

---

## 8. Phase S3 — Live agent-sight verification (new MCP only)

Run **after S0 binary is the one on PATH / MCP**. Log under goal `{SCRATCH}` and each game’s `captures/eyesight/`.

### S3.1 Crystal Drift

```text
1. pkill old games on 15702 if needed
2. cargo build --features remote,capture  (Crystal Drift)
3. Launch crystal_drift
4. specula brp wait --port 15702
5. specula see scene --profile crystal-drift --out-dir <Crystal Drift> --save-baseline …
6. specula see entity --profile crystal-drift --name Player (or primary)
7. specula see pack landscape --profile crystal-drift
8. Optional: mutate Player; see motion with stimulus; see scene --compare-baseline
9. Assert: app_state Playing; primary_subject Player (or documented rank); new Names present;
          abs_path PNGs bytes>0; black_frame_warning absent on play captures
10. Open primary full + fovea images (agent image-read) — observation only
```

### S3.2 Iron Feud

```text
1. Stop CD if same port
2. cargo build --features remote,capture
3. IRON_FEUD_AUTO_PLAY=1 launch iron_feud
4. brp wait
5. see scene --profile iron-feud --require-playing --out-dir <Iron Feud>
6. see entity --name WaterBody (or RelayTower / new prop)
7. see pack water + landscape --profile iron-feud
8. Assert: no MenuCamera-only; WaterBody/Ground/new env Names; multi-view or views_similar warning;
          diagnostic uses non-Player primary if no Player
9. Open PNGs; observation only
```

### S3.3 MCP regression

- [x] `bevy_see_scene` / `bevy_see_verify` (if added) appear in tools/list after reload  
- [x] initialize.instructions mention profiles and Playing gate  

**S3 exit criteria:** Both games produce non-empty eyesight packets with new content Names; S0 ranking/filter visible in JSON.

---

## 9. Phase S4 — Docs and closeout

- [x] Checkboxes in this file flipped to `[x]` for completed work  
- [x] [PROGRESS.md](../PROGRESS.md) section for this plan  
- [x] Findings: `docs/AGENT_SIGHT_NEXT_FINDINGS_YYYY-MM-DD.md` (features, dogfood names, packet paths, suggestions)  
- [x] Skill `bevy-agent-loop` + eyesight-packs: profiles, primary ranking, MCP-first note  
- [x] Brief ROADMAP/AGENTS link to this plan if not already  

---

## 10. Success metrics (definition of done)

All must hold:

1. **MCP first:** S0 merged/built before dogfood captures used for pass/fail.  
2. **Sight upgrades:** primary ranking, name collapse, profiles, default full+fovea path, baseline compare path, motion stimulus option, views_similar honesty, diagnostic without Player-only hardcode.  
3. **Crystal Drift:** ≥2 additions, ≥1 improvement, env modification **and** env addition; build PASS; live see packets include new Names.  
4. **Iron Feud:** same quotas; Playing via AUTO_PLAY; live see packets include new Names + Water/Ground.  
5. **Unit tests** for rank/collapse/profile pure logic PASS.  
6. **No excluded items** (video stream, editor UI, unlit suite, taste scorer).  
7. **Findings file** written; plan checkboxes updated.  

---

## 11. Target MCP / CLI additions (summary)

| Surface | Additions |
|---------|-----------|
| `bevy_see_scene` | `profile`, better primary, collapse, optional embed fovea / `bevy_see_verify` |
| `bevy_see_entity` | uses ranker when name=`*` or empty; profile projection |
| `bevy_see_motion` | optional mutate stimulus params |
| `bevy_see_pack` | views_similar; diagnostic primary from ranker |
| CLI | `--profile crystal-drift\|iron-feud` |
| instructions | profiles, Playing gate, human-owned taste, no livestream |

---

## 12. Risks

| Risk | Mitigation |
|------|------------|
| Stale MCP | Rebuild + reload before S3; version stamp already in env_check |
| Port 15702 conflict | Sequential dogfood; kill previous process |
| Projection still wrong | Profiles + document retune; loud fallback warning |
| Content bloat | Keep additions small Named props; no full rewrite |
| External trees not in Specula git | Document paths; commit Specula separately; IF/CD commits if those repos exist |

---

## 13. `/goal` paste template

```text
Execute docs/AGENT_SIGHT_NEXT_PLAN.md to completion (S0 → S4).
Order is mandatory: MCP/sight code first, rebuild MCP, then dogfood
  /Users/aron/Documents/coding_projects/Crystal Drift
  /Users/aron/Documents/coding_projects/Iron Feud
Each game: 2 additions, 1 improvement, env modification AND env addition.
Do NOT implement: livestream/60fps, human editor, unlit suite, auto taste scoring.
Bevy 0.19, remote,capture, BRP 15702. Iron Feud: IRON_FEUD_AUTO_PLAY=1.
Update plan checkboxes + PROGRESS + findings file.
Taste/design human-owned; agent sight only.
```

---

## 14. Document history

| Date | Change |
|------|--------|
| 2026-07-21 | Initial Agent Sight Next plan: S0 MCP-first upgrades; CD/IF dogfood quotas; exclusions explicit |
| 2026-07-21 | S0–S4 implemented: ranking, profiles, verify, dogfood CD/IF |
