# Agent Eyesight 20/20 Plan — acuity upgrade (Specula)

**Status:** implemented (A0–A8 shipped; dogfood Crystal Drift + Iron Feud Playing, 2026-07-21)  
**Audience:** agents implementing Specula; humans setting goals and reviewing MCP dogfood  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**Parent plan (shipped baseline):** [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) (V0–V6 — “eyes open”)  
**This plan’s job:** raise perception from **“can see”** to **observation-grade ~20/20 acuity** — focus, multi-view, time, clean subjects, reliable state — so agents and human MCP users **observe** what the game actually renders.

Related:

- Live loop skill: `.grok/skills/bevy-agent-loop/`  
- Packs reference: `.grok/skills/bevy-agent-loop/references/eyesight-packs.md`  
- Findings (baseline): [AGENT_EYESIGHT_FINDINGS_2026-07-21.md](AGENT_EYESIGHT_FINDINGS_2026-07-21.md)  
- Physics pins: [PHYSICS.md](PHYSICS.md)  
- Product roadmap: [ROADMAP.md](ROADMAP.md)  
- Progress: [PROGRESS.md](../PROGRESS.md)  
- README honesty contract: root [README.md](../README.md) (agent eyes vs human taste)

---

## 1. North star

> **20/20 eyesight = high-fidelity sensory packets the agent (and other MCP users) can open and use as ground truth.**  
> Taste, art direction, and design ownership stay with **human developers**.  
> The agent **sees and builds to their needs** through MCP — it does not replace human judgment.

### What “20/20” means here

| Human vision analogy | Agent capability |
|----------------------|------------------|
| Eyes open | V0–V6 baseline (`see_scene`, crops, motion, packs) — **done** |
| Fovea + focus | World-projected entity crop, not center guess |
| Walk around subject | Multi-view packs (game / top / side) |
| Motion perception | Reliable temporal strips + stimulus contract |
| Knowing object names | Filtered, on-screen subjects |
| Memory of change | Session baselines + before/after |
| Not color-blind to layers | Optional diagnostic frames (unlit / labels) — agent-only |

### Explicit non-goals

- Human taste scoring, auto-beautify, “AI art director”  
- Full Bevy editor, gizmo sandbox, hierarchy UI product  
- Continuous 60 FPS video as default  
- Parallel full BRP browser (prefer `bevy_brp_mcp` when needed)  
- Replacing human design review

### Honest product claim (for README / marketing)

**After this plan ships:** Specula agents and MCP clients can **see at observation grade (≈20/20 acuity)** — composition, subject fovea, multi-view, motion, and diffs — on a running Bevy app.

**Still human-owned:** style direction, fun, balance, narrative, “is this good design?” Agents execute and verify **visually** against what humans ask for; they do not own taste.

---

## 2. Baseline (already shipped — do not re-litigate)

From [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) V0–V6:

| Channel | Tools | Role |
|---------|-------|------|
| E0 full frame | `bevy_see_scene`, `bevy_capture_viewport` | Composition |
| E1 crop | `bevy_see_entity`, `bevy_see_region` | Fovea (center/default rect) |
| E2 motion | `bevy_see_motion` | Temporal strip + montage |
| E3 diff | `bevy_see_diff` | Before/after + abs-diff |
| E4/E5 packs | `bevy_see_pack` | landscape / water / entity_craft / physics_jump / lighting |
| Schema | `specula.sight/v1` | abs_path, subjects, warnings, style_intent |
| Hardening | black-frame heuristic, short strips, CLI `specula see` | V6 |

Pins unchanged unless a phase requires bump: Bevy **0.19**, BRP extras **0.22.1**, port **15702**, features **`remote,capture`**.

### Known acuity gaps (why not 20/20 yet)

| Gap | Effect |
|-----|--------|
| Entity crop defaults to **window center** | Wrong or partial subject when camera/world offset |
| No **world → screen** projection in packet path | Fovea is approximate |
| Single camera for most packs | 3D / landscape / water misread |
| Motion strips short / stimulus optional | Miss jitter, tunneling, one-frame pops |
| Subject lists noisy (e.g. many Stars) | Hard to find design subjects |
| No on-screen filter | Off-screen entities look “present” |
| State races (menu vs Playing) | Wrong world captured |
| Diagnostic layers missing | Can’t separate art vs lighting vs occlusion |
| Chat truncates images | Must keep abs_path discipline |

---

## 3. Design principles (acuity-only)

1. **Perception over judgment** — plan delivers better *signals*; humans (and agent brains) still decide taste.  
2. **Agent eyes, not editor** — every feature answers: “Does this improve what the agent can observe?”  
3. **Pixels primary; ECS grounds them** — crop without wrong entity is still bad sight.  
4. **Skill↔tool contract** — extend `see_*` and packet schema; avoid tool sprawl.  
5. **Dogfood external trees** — Crystal Drift + Iron Feud are required proof targets (see §8).  
6. **Honest README** — claim 20/20 *sight*, not 20/20 *design sense*.  
7. **Other MCP users** — same contract for human devs driving Specula MCP (not Grok-only).  
8. **Reversible, testable** — pure helpers unit-tested; live BRP when GPU/window available.

---

## 4. Target sensory model (20/20 stack)

```text
┌──────────────────────────────────────────────────────────────┐
│  A0  Full frame + state gate        composition, correct act  │
│  A1  True fovea (world→screen)      craft of the right entity │
│  A2  Multi-view pack                3D / landscape / water    │
│  A3  Temporal acuity                motion + stimulus         │
│  A4  Clean subjects                 filter + on-screen        │
│  A5  Diff memory                    session baseline / after  │
│  A6  Diagnostic frames (optional)   unlit / labels / bounds   │
│  +   abs_path + dims + black-frame  always openable evidence  │
└──────────────────────────────────────────────────────────────┘
```

Schema remains `specula.sight/v1` with **additive** fields (backward compatible). Suggested additions:

```json
{
  "schema": "specula.sight/v1",
  "acuity": "20/20-candidate",
  "primary_subject": "Player",
  "subjects": [
    {
      "name": "Player",
      "entity": 42,
      "translation": [0, 0, 0],
      "screen_xy": [640, 360],
      "on_screen": true,
      "screen_aabb": [600, 320, 80, 80]
    }
  ],
  "subject_filter": { "mode": "gameplay_prefer", "max": 48 },
  "views": ["game", "top", "side"],
  "baseline_path": "captures/eyesight/baseline_scene.png",
  "app_state": "Playing"
}
```

---

## 5. Phased delivery (for `/goal`)

Execute **one phase per goal** when possible; OBJECTIVE may batch. Each phase: implement → unit tests → dogfood Crystal Drift and/or Iron Feud → update checkboxes + [PROGRESS.md](../PROGRESS.md) + findings note if material.

### Phase A0 — State gate & capture reliability

**Outcome:** Captures happen in the **correct AppState** with non-empty gameplay subjects when expected.

- [x] Document and enforce **state wait** in skill + optional CLI/MCP flags (`wait_for_subjects`, `expected_names[]`).  
- [x] Crystal Drift: already launches into Playing — assert subjects include `Player` + at least one env Name.  
- [x] Iron Feud: require `IRON_FEUD_AUTO_PLAY=1` (or Enter) in dogfood docs; fail fast if only `MenuCamera` when Playing intended.  
- [x] Packet field `app_state` when discoverable; warning if subjects look menu-only.  
- [x] README note: agents must wait for play state before claiming environment sight.

**Exit criteria:** Dogfood scripts never claim “saw water/rocks” from MainMenu packets.

**Dogfood:** Iron Feud Playing required; Crystal Drift Playing smoke.

---

### Phase A1 — True fovea (highest leverage)

**Outcome:** `bevy_see_entity` crops the **actual entity**, not window center.

- [x] Compute **world → screen** for orthographic 2D (Crystal Drift primary) and document 3D strategy-cam approximation (Iron Feud top-down).  
- [x] Project entity translation (+ optional half-extents heuristic) to `screen_xy` / `screen_aabb`.  
- [x] Crop PNG using projected AABB with padding; fallback center only with packet `note` + warning.  
- [x] Optional **scale ladder**: full + 1× fovea + 2× zoom crop of same subject.  
- [x] Unit tests: pure projection math + crop rect from synthetic dims.  
- [x] Live dogfood: Crystal Drift `Player` / `DerelictStation` / `Crystal` crop contains subject (manual open + non-empty crop bytes).  
- [x] Live dogfood: Iron Feud `WaterBody` or `FieldScrap_A` crop when Playing.

**Exit criteria:** Center-default crops are fallback only; packets include `screen_xy` for primary subject when projection succeeds.

**Implementation lean:** client-side crop of full screenshot (no editor overlay). Prefer BRP Name + Transform already queried.

---

### Phase A2 — Multi-view (3D / landscape / water)

**Outcome:** Agent can “walk around” environments with a small view set.

- [x] Pack expansion: `landscape` and `water` guarantee **≥2 distinct views** (not only crops of one frame when multi-view available).  
- [x] **Agent debug cameras** under feature or BRP-spawned temporary cameras (code in dogfood games if needed — **not** human editor UI):  
  - `game` (existing StrategyCamera / MainCamera)  
  - `top` elevated orthographic or high Y look-down  
  - `side` optional for 3D readability  
- [x] Capture sequence writes distinct paths (`view_game.png`, `view_top.png`, …) with **different content hashes** when views differ.  
- [x] Iron Feud dogfood: Playing + water pack shows WaterBody; landscape pack shows Ground/Cliff/rocks.  
- [x] Crystal Drift dogfood: landscape pack still useful (nebula/station composition); multi-view may be 2D pan/zoom equivalents if no second camera.

**Exit criteria:** Iron Feud multi-view pack produces ≥2 unique MD5 PNGs and subjects include environment Names.

**Non-goal:** permanent multi-camera game design for players.

---

### Phase A3 — Temporal acuity

**Outcome:** Motion and physics *observation* are reliable enough for “what happened over 0.5–1s.”

- [x] Defaults: 8–12 frames, 50–100 ms interval (configurable; cap ≤12 for cost).  
- [x] Stimulus contract documented + exercised:  
  - `keys` via `brp_extras/send_keys` when available  
  - `mutate` Transform / velocity when Reflect allows  
- [x] Packet lists ordered `frame` captures + optional strip; note stimulus applied.  
- [x] Crystal Drift: motion after brief WASD or mutate Player translation — strip frames not all identical MD5.  
- [x] Iron Feud: optional pan or sim tick motion — strip shows some change OR honest “static scene” note.  
- [x] Skill: physics/motion claims require E2/A3 evidence paths.

**Exit criteria:** Dogfood motion packets either show frame diversity or explicit static warning — never silent identical strips without note.

---

### Phase A4 — Clean subjects (signal hygiene)

**Outcome:** Packets surface **design-relevant** entities first.

- [x] Subject filter modes: `all` | `gameplay_prefer` (default) | `names_only`.  
- [x] Prefer Names matching prefixes/allowlist (Player, Camera, Water, Rock, Tree, Cliff, Scrap, Station, Nebula, Ground, …); deprioritize Star/Particle spam.  
- [x] Cap list (`max_subjects`, default ~48) with `subjects_truncated` flag.  
- [x] `on_screen` boolean from projection vs viewport.  
- [x] `primary_subject` field when `name` arg or best gameplay match.  
- [x] Unit tests on filter/sort pure functions.  
- [x] Dogfood: Crystal Drift scene packet lists Player/env before hundreds of stars (or stars capped).  
- [x] Dogfood: Iron Feud lists Rock/Tree/Cliff/Scrap/Water/Ground.

**Exit criteria:** Default filter makes human/agent glance of subject list usable in &lt;5 seconds.

---

### Phase A5 — Diff memory (observe change)

**Outcome:** Session-level **before/after** is first-class for iteration loops.

- [x] Optional MCP/CLI: register baseline path (`see baseline set` or arg on `see_scene`).  
- [x] `bevy_see_diff` remains; add convenience `see_scene --compare-baseline PATH`.  
- [x] Skill rule: after asset/param change for visual work, produce after packet citing baseline.  
- [x] Dogfood: Crystal Drift — capture baseline, mutate player translation, diff mean score &gt; 0 or visible diff path.  
- [x] Document for human MCP users in README / skill.

**Exit criteria:** One-command before/after evidence path exists and is dogfooded once per external game.

---

### Phase A6 — Diagnostic frames (optional but 20/20-complete)

**Outcome:** Separate “empty/off-screen” vs “bad art” vs “bad light” with agent-only views.

- [x] Optional capture flags: `labels` (Name tags in-engine or post), `bounds` wire/AABB overlay — **agent feature flag**, default off for beauty stills.  
- [x] `unlit` / simplified material only if dogfood game exposes a safe toggle; otherwise document as game-side opt-in.  
- [x] Pack role names: `diagnostic_labels`, `diagnostic_unlit`.  
- [x] Dogfood on one game minimum (prefer Iron Feud lighting complexity).  

**Exit criteria:** At least one diagnostic path documented and exercised; beauty packs remain clean by default.

**Non-goal:** permanent debug UI for players.

---

### Phase A7 — README + docs honesty + MCP user contract

**Outcome:** Public docs state clearly what 20/20 means.

- [x] Update root **README.md** (see §9 template).  
- [x] Cross-link plan from AGENTS.md, ROADMAP.md, PROGRESS.md.  
- [x] Skill `bevy-agent-loop`: acuity rules (open abs_path, fovea when refining craft, multi-view for env, filter subjects).  
- [x] Short “for human MCP users” section: same tools, same honesty — agent/MCP *sees*; humans *own design*.  
- [x] Findings file after implementation goal (e.g. `docs/AGENT_EYESIGHT_20_20_FINDINGS_YYYY-MM-DD.md`).

**Exit criteria:** README can be quoted without overclaiming design taste.

---

### Phase A8 — Hardening & multi-user MCP

**Outcome:** 20/20 works under real agent stress and multi-instance.

- [x] Named targets / ports documented for dual dogfood (don’t bind two apps to 15702).  
- [x] Black-frame: keep dark-space-safe heuristic; add regression fixture with bright sprite on black.  
- [x] Packet validation tests expanded for new fields.  
- [x] CI: unit tests always; live capture optional/GPU.  
- [x] Performance: no default 60 FPS stream; multi-view ≤3 cameras; motion ≤12 frames.

**Exit criteria:** Full unit suite green; dual-game dogfood documented without port conflict.

---

## 6. Tool / CLI / MCP surface (target)

Extend existing tools; prefer additive params.

| Tool / CLI | Acuity additions |
|------------|------------------|
| `bevy_see_scene` / `see scene` | subject filter, primary_subject, state wait, baseline register |
| `bevy_see_entity` / `see entity` | world→screen crop, padding, zoom ladder, screen_xy in packet |
| `bevy_see_region` / `see region` | optional world AABB → screen rect |
| `bevy_see_motion` / `see motion` | better defaults, stimulus required-or-noted |
| `bevy_see_diff` / `see diff` | session baseline helper |
| `bevy_see_pack` / `see pack` | multi-view camera sequence; diagnostic flags |

Workflow goal `verify_scene` should mention **acuity packet** (full + fovea + optional pack), not raw screenshot alone.

---

## 7. Skill contract (HOW)

Update `.grok/skills/bevy-agent-loop/SKILL.md` and `references/eyesight-packs.md`:

1. Open **every** capture `abs_path`.  
2. For craft claims: **true fovea** on named subject (A1).  
3. For environment claims: **multi-view pack** (A2) + Playing state (A0).  
4. For motion/physics claims: **motion strip** + stimulus note (A3).  
5. After visual change: **diff vs baseline** (A5).  
6. **Do not claim taste** as product feature — implement to human intent.  
7. Style intent remains optional field for asset loops; judgment is not automated “beauty score.”

---

## 8. Dogfood contract (required targets)

### Crystal Drift — `/Users/aron/Documents/coding_projects/Crystal Drift`

| Need | Notes |
|------|--------|
| Features | `remote,capture` |
| State | Playing (default launch) |
| Subjects to observe | `Player`, `NebulaCloud_*`, `DerelictStation`, `Crystal`, asteroids, scrap/shield when present |
| Acuity proofs | true fovea on Player; landscape pack; motion diversity; black-frame not false-positive; filtered subjects |
| Env | Nebula field + station already present |

### Iron Feud — `/Users/aron/Documents/coding_projects/Iron Feud`

| Need | Notes |
|------|--------|
| Features | `remote,capture` |
| State | **Playing** via `IRON_FEUD_AUTO_PLAY=1` or Enter — mandatory for env claims |
| Subjects to observe | `Ground`, `WaterBody`, `RockOutcrop_*`, `TreeScrub_*`, `CliffRidge_*`, `FieldScrap_*`, `StrategyCamera` |
| Acuity proofs | multi-view landscape/water; fovea on WaterBody or scrap; water_tint material remains wired; unique MD5s across views |
| Env | Rocks, trees, cliff, scrap, water, plateau |

### Shared dogfood steps (template for implementer)

```text
1. cargo build --features remote,capture  (each game)
2. Launch (Iron: IRON_FEUD_AUTO_PLAY=1)
3. specula brp wait --port 15702
4. specula see scene --out-dir <game>
5. specula see entity --name <primary> --out-dir <game>
6. specula see pack landscape|water --out-dir <game>
7. specula see motion --frames 8 --out-dir <game>
8. Assert: packet JSON schema, abs_path exists, bytes>0, subjects include env Names
9. Open PNGs (agent image-read) — describe what was seen (observation, not taste lecture)
10. Log to goal scratch; cite paths in findings
```

In-repo `games/demo-2d` / `demo-3d` remain secondary smoke targets; **external trees are primary** for this plan.

---

## 9. README update requirements (honesty)

When implementing A7 (or earlier if desired), root README **must** include a short section along these lines (edit for voice, keep claims):

### Agent eyesight (honest scope)

- Specula gives coding agents **live eyes on a running Bevy window** via MCP: full scene, subject crops, motion strips, multi-view packs, and before/after diffs.  
- After the **20/20 acuity** work, agents can **observe** graphics and environments at high fidelity (composition + focus + time + change).  
- **Taste, art direction, and design decisions remain human.** The agent is here to **see** what was built and **implement** what human developers need — not to own creative judgment.  
- Human developers using the same MCP get the **same sensory tools** for their own workflows.  
- Link: [docs/AGENT_EYESIGHT_20_20_PLAN.md](docs/AGENT_EYESIGHT_20_20_PLAN.md), baseline [docs/AGENT_EYESIGHT_PLAN.md](docs/AGENT_EYESIGHT_PLAN.md).

Avoid phrases that imply “the AI decides if your game looks good.” Prefer “the AI can see the viewport and verify against your requirements.”

Also update:

- “What you get” bullet: eyesight tools (`bevy_see_*` / `specula see`)  
- Portfolio section: mention observation-grade capture when 20/20 ships  

---

## 10. Success metrics (definition of 20/20 shipped)

All must hold:

1. **True fovea:** primary named entity crop uses projection when possible; packet includes `screen_xy` / `on_screen`.  
2. **Multi-view:** Iron Feud landscape or water pack ≥2 distinct views (unique hashes).  
3. **Temporal:** motion strip with documented stimulus; diversity or explicit static note.  
4. **Clean subjects:** default filter usable; Stars/particles do not dominate.  
5. **State:** Playing-only env claims on Iron Feud with AUTO_PLAY.  
6. **Diff:** one baseline→after path dogfooded.  
7. **Honesty:** README states 20/20 sight, human-owned taste.  
8. **Tests:** pure helpers unit-tested; live dogfood logs + non-empty PNGs.  
9. **No editor creep:** no PR whose primary surface is human scene-graph UX.

---

## 11. Risks & open decisions

| Risk / decision | Lean |
|-----------------|------|
| 3D projection accuracy on strategy cam | Approximate top-down map; document error bounds |
| Debug cameras pollute game | Feature-gate; despawn after capture |
| send_keys unreliable | Mutate Transform as portfolio fallback |
| External games out of Specula git | Document paths; commit only Specula unless user asks |
| Overclaiming “20/20” | Tie claim to §10 metrics, not marketing fluff |

---

## 12. `/goal` paste template

```text
Execute docs/AGENT_EYESIGHT_20_20_PLAN.md Phase A{N} only (or A0–A8 if OBJECTIVE batches).
Respect: agent eyesight acuity, not editor; not human taste product.
Dogfood: /Users/aron/Documents/coding_projects/Crystal Drift and
         /Users/aron/Documents/coding_projects/Iron Feud (Playing: IRON_FEUD_AUTO_PLAY=1).
Bevy 0.19, remote,capture, BRP 15702.
Update plan checkboxes + PROGRESS.md; write findings if phase completes exit criteria.
README honesty when A7 (or final batch): 20/20 sight; taste/design human-owned.
```

---

## 13. Mapping to product surface

| Layer | Responsibility |
|-------|----------------|
| **Skills** | HOW to open eyes, which pack, no taste theater |
| **MCP / CLI** | WHAT evidence packets to fetch |
| **eyesight.rs** | Pure crop/project/filter/montage + BRP orchestration |
| **Dogfood games** | Named entities, env, Playing gate, optional debug cams |
| **README** | Honest public contract |

Agentic loop becomes:

```text
Skills → kits → build → ACUITY packet (A0–A6) → implement to human need → re-see → package
```

---

## 14. Immediate next step

Start **Phase A0 + A1** together if a single goal allows: state gate + true fovea unlock almost all later value. Then A2 (Iron Feud multi-view) and A4 (subject filter). README honesty (A7) can land with the first public-facing milestone or at the end of the batch.

---

## Document history

| Date | Change |
|------|--------|
| 2026-07-21 | Initial 20/20 acuity plan: A0–A8, dogfood CD + IF, README honesty contract |
| 2026-07-21 | Linked from README, ROADMAP, AGENTS, PROGRESS; baseline plan points here |
| 2026-07-21 | A0–A8 implemented: acuity fovea/filter/multi-view/baseline/diagnostic; MCP+CLI; dogfood |
