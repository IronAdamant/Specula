# Specula progress checklist

## v0.1 — public-ready runtime control (done)

Track first public-ready (v0.1) delivery. Check items as they land.

### Research
- [x] Pin Bevy / `bevy_brp_mcp` / `bevy_brp_extras` versions (Bevy **0.19**, BRP stack **0.22.1**)
- [x] Confirm BRP methods, screenshot path, MCP integration model

### Scaffold
- [x] Workspace layout (`specula-env`, `specula-brp`, `specula`, `templates/sample-app`)
- [x] Dual MIT/Apache-2.0 licenses, gitignore, modular crates

### Environment readiness
- [x] Cross-platform detection (Windows / Linux / macOS)
- [x] Rust/Cargo readiness + OS-specific install guidance
- [x] Optional Bevy create+compile probe
- [x] Unit tests for detection + guidance

### MCP / BRP
- [x] BRP HTTP client (query / mutate / generic call)
- [x] MCP stdio server with agent tools
- [x] Integration path with `bevy_brp_mcp` (install/delegate)
- [x] Sample template with RemotePlugin / BrpExtras + feature flags

### Visual capture
- [x] Screenshot via `brp_extras/screenshot`
- [x] Image return adapter (PNG → MCP image content)
- [x] Fixture-based unit tests for image adapter
- [x] Live capture verified (2560×1440 PNG)

### Docs & OSS hygiene
- [x] README fast-start + Grok Build MCP snippets
- [x] CONTRIBUTING, CHANGELOG, multi-platform CI
- [x] Troubleshooting + compatibility matrix

### Verification
- [x] CLI env-check evidence
- [x] BRP control evidence (query + mutate)
- [x] Capture evidence (live PNG)
- [x] Template build (`remote,capture`)

---

## v0.2 — production games (done)

Skills define HOW; scaffold defines WHERE; MCP verifies WHAT.
See [docs/PRODUCTION_GAMES.md](docs/PRODUCTION_GAMES.md).

### Positioning
- [x] README production section + demo vs game distinction
- [x] `docs/PRODUCTION_GAMES.md` skill map and layout contract
- [x] PROGRESS v0.2 checklist

### Skill pack
- [x] Root `AGENTS.md` (pins, skill routing, anti-demo rules)
- [x] `.grok/skills/bevy-production`
- [x] `.grok/skills/bevy-2d-game`
- [x] `.grok/skills/bevy-3d-game`
- [x] `.grok/skills/bevy-agent-loop`

### Production templates
- [x] `templates/game-2d` playable vertical slice (states, movement, disk asset)
- [x] `templates/game-3d` playable vertical slice (states, movement, disk asset)
- [x] Keep `templates/sample-app` as BRP integration fixture only

### Scaffold
- [x] `scaffold --kind 2d|3d|demo` copies templates (source of truth)
- [x] Scaffold writes project `AGENTS.md` + asset dirs
- [x] Integration test: scaffold tree (+ cargo check in verification)

### Phase 4 production depth
- [x] `docs/ASSET_CONVENTIONS.md` (sprites/models/ui/audio)
- [x] `docs/SHIPPING.md` (`cargo build --release`, packaging notes)
- [x] Scaffolded project README/AGENTS surface ship + asset paths

### MCP skill routing
- [x] Richer `initialize.instructions` (skills + scaffold kinds)
- [x] MCP prompts: start_2d_game, start_3d_game, iterate_scene, prepare_ship
- [x] `bevy_workflow` tool (goal → steps + skills + tools)

---

## v0.3 — game factory (alpha): demos that are real games

See [docs/ROADMAP.md](docs/ROADMAP.md) and [docs/GAME_DOD.md](docs/GAME_DOD.md).  
Steam is **later** (G5); non-Steam package is G4.

### G1 — DoD + agent contract
- [x] `docs/GAME_DOD.md` + `docs/ROADMAP.md`
- [x] Skill `bevy-demo-game`
- [x] Production / 2d / 3d / agent-loop skills reference DoD
- [x] MCP prompts + `bevy_workflow` goals for complete demos / package

### G2 — Templates are short games
- [x] `templates/game-2d` meets GAME_DOD (objective, win/lose, challenge, HUD)
- [x] `templates/game-3d` meets GAME_DOD
- [x] Structural tests + cargo check with `remote,capture`

### G3 — In-repo dogfood
- [x] `games/demo-2d` workspace member
- [x] `games/demo-3d` workspace member
- [x] README default path: run dogfood demos

### G4 — Package (non-Steam)
- [x] Package script/docs (binary + assets → dist/zip)
- [x] Skill `bevy-package` + workflow `package_demo`
- [x] CI check demos; optional release artifact

### G5 — Steam path (later)
- [ ] `docs/STEAM_PATH.md` + skill checklist
- [ ] Optional steam feature stub

### G6 — Install ergonomics
- [x] Reliable templates after install / embedded templates (`include_dir` in `specula`; fallback extract to cache when monorepo path missing)
- [x] Optional kit feature `physics` (avian2d/avian3d **0.7**); default kits stay transform-based

---

## Agent eyesight (long-horizon) — planned

**Not an editor.** Sensory stack so agents can see graphics (entity, landscape, water) and motion/physics, judge for human taste, and re-see after fixes.

Durable plan + phase checkboxes: [docs/AGENT_EYESIGHT_PLAN.md](docs/AGENT_EYESIGHT_PLAN.md).  
Execute with `/goal` against one phase at a time.

| Phase | Name | Status |
|-------|------|--------|
| **V0** | Eyesight discipline (skills/docs; open captures as evidence) | **done** |
| **V1** | `bevy_see_scene` packet (full frame + thin context) | **done** |
| **V2** | Fovea: `bevy_see_entity` / `bevy_see_region` crops | **done** |
| **V3** | Temporal + physics eyesight (`bevy_see_motion`) | **done** |
| **V4** | Diff + multi-view packs (landscape / water / 3D) | **done** |
| **V5** | Style-aware art loop (see → regen → see_diff) | **done** |
| **V6** | Harden multi-target / black-frame / CI | **done** |

- [x] Plan file landed (`docs/AGENT_EYESIGHT_PLAN.md`) + roadmap link
- [x] Phase V0 complete (exit criteria in plan)
- [x] Phase V1 complete
- [x] Phase V2 complete
- [x] Phase V3 complete
- [x] Phase V4 complete
- [x] Phase V5 complete
- [x] Phase V6 complete
- [x] Findings: [docs/AGENT_EYESIGHT_FINDINGS_2026-07-21.md](docs/AGENT_EYESIGHT_FINDINGS_2026-07-21.md)

---

## Agent eyesight 20/20 (acuity) — planned

Raise perception from “can see” to observation-grade acuity. **Not** human taste/design product.

Plan + phases A0–A8: [docs/AGENT_EYESIGHT_20_20_PLAN.md](docs/AGENT_EYESIGHT_20_20_PLAN.md).  
Dogfood: Crystal Drift + Iron Feud (Playing: `IRON_FEUD_AUTO_PLAY=1`).

| Phase | Name | Status |
|-------|------|--------|
| **A0** | State gate (Playing vs menu) | **done** |
| **A1** | True fovea (world→screen crop) | **done** |
| **A2** | Multi-view landscape/water/3D | **done** |
| **A3** | Temporal acuity + stimulus | **done** |
| **A4** | Clean subjects (filter + on-screen) | **done** |
| **A5** | Diff memory / baselines | **done** |
| **A6** | Diagnostic frames (optional) | **done** |
| **A7** | README honesty + MCP user contract | **done** |
| **A8** | Hardening multi-target / CI | **done** |

- [x] Plan file landed (`docs/AGENT_EYESIGHT_20_20_PLAN.md`) + roadmap/AGENTS/README links  
- [x] Phase A0–A8 complete (exit criteria in plan)  
- [x] Findings: [docs/AGENT_EYESIGHT_20_20_FINDINGS_2026-07-21.md](docs/AGENT_EYESIGHT_20_20_FINDINGS_2026-07-21.md)

---

## Agent sight next — planned

Current-gen observation quality (ranking, collapse, profiles, verify packet).  
**MCP first**, then dogfood. No livestream / editor / unlit suite / taste scoring.

Plan: [docs/AGENT_SIGHT_NEXT_PLAN.md](docs/AGENT_SIGHT_NEXT_PLAN.md).

| Phase | Name | Status |
|-------|------|--------|
| **S0** | MCP/sight code + rebuild (before dogfood) | **done** |
| **S1** | Crystal Drift: 2 add + 1 improve + env mod + env add | **done** |
| **S2** | Iron Feud: 2 add + 1 improve + env mod + env add | **done** |
| **S3** | Live eyesight verify both games (new MCP) | **done** |
| **S4** | Docs / findings / checkboxes | **done** |

- [x] Plan file landed  
- [x] S0–S4 complete  
- [x] Findings: [docs/AGENT_SIGHT_NEXT_FINDINGS_2026-07-21.md](docs/AGENT_SIGHT_NEXT_FINDINGS_2026-07-21.md)

---

## Agent sight 2D + 3D — done

Dimension-specific observation + dogfood. MCP first; IF height terrain flat→hills→mountains.  
**No** livestream / editor / unlit suite / taste scoring. Long-session: no shortcuts.

Plan: [docs/AGENT_SIGHT_2D3D_PLAN.md](docs/AGENT_SIGHT_2D3D_PLAN.md).

| Phase | Name | Status |
|-------|------|--------|
| **D0** | Specula 2D/3D sight + MCP rebuild | **done** |
| **D1** | Crystal Drift: 2 features+assets, env improve, live | **done** |
| **D2** | Iron Feud: 2 features+assets, height terrain, live | **done** |
| **D3** | Live eyesight review both games | **done** |
| **D4** | Fix + re-verify | **done** |
| **D5** | Docs / findings / assessments | **done** |

- [x] Plan file landed  
- [x] D0–D5 complete  
- [x] Findings: [docs/AGENT_SIGHT_2D3D_FINDINGS_2026-07-22.md](docs/AGENT_SIGHT_2D3D_FINDINGS_2026-07-22.md)

---

## Agent sight debt — done

Residual acuity + **full dogfood asset/env pass** (CD + IF): filter/multi-view/fovea debts; each game **1 new feature+asset**, **1 new env**, **improve all existing assets and environments**.

Plan: [docs/AGENT_SIGHT_DEBT_PLAN.md](docs/AGENT_SIGHT_DEBT_PLAN.md).  
Findings: [docs/AGENT_SIGHT_DEBT_FINDINGS_2026-07-23.md](docs/AGENT_SIGHT_DEBT_FINDINGS_2026-07-23.md).

| Phase | Name | Status |
|-------|------|--------|
| **R0** | Specula residual sight + MCP rebuild | **done** |
| **R1** | Crystal Drift: 1 new + 1 env + improve all assets/env | **done** |
| **R2** | Iron Feud: 1 new + 1 env + improve all assets/env | **done** |
| **R3** | Live review both + fix loop | **done** |
| **R4** | Docs / findings / assessments | **done** |

- [x] Plan file landed  
- [x] R0–R4 complete  
- [x] Findings: [docs/AGENT_SIGHT_DEBT_FINDINGS_2026-07-23.md](docs/AGENT_SIGHT_DEBT_FINDINGS_2026-07-23.md)

---

## Agent sight hardening — done

Tier 1→3 (version dogfood, pixel gates, true multi-view, baseline) + craft dogfood: **1 new feature each** on CD/IF; **improve all existing features** (transparent 2D sprites; non-blocky IF machines; randomized IF height).

Plan: [docs/AGENT_SIGHT_HARDENING_PLAN.md](docs/AGENT_SIGHT_HARDENING_PLAN.md).

| Phase | Name | Status |
|-------|------|--------|
| **H0** | Specula gates + side-view + rebuild | **done** |
| **H1** | Version CD + IF as git projects | **done** |
| **H2** | Crystal Drift: 1 new + improve all sprites | **done** |
| **H3** | Iron Feud: 1 new + complex meshes + heightfield | **done** |
| **H4–H5** | Live review + fix | **done** |
| **H6** | Docs / findings | **done** |

- [x] Plan file landed  
- [x] H0–H6 complete
- [x] Findings: [docs/AGENT_SIGHT_HARDENING_FINDINGS_2026-07-23.md](docs/AGENT_SIGHT_HARDENING_FINDINGS_2026-07-23.md)

---

## Agent sight fidelity — done

Next Tier 1→3 cycle after hardening: **1 new feature each** on CD/IF; **improve all existing features** to complex non-block silhouettes; CD transparent sprite BGs (no purple/magenta plates); IF randomized varying height; residual multi-view honesty.

Plan: [docs/AGENT_SIGHT_FIDELITY_PLAN.md](docs/AGENT_SIGHT_FIDELITY_PLAN.md).

| Phase | Name | Status |
|-------|------|--------|
| **F0** | Specula residual platform (multi-view / gates / stems) | **done** |
| **F1** | Crystal Drift: CargoPod + improve all sprites | **done** |
| **F2** | Iron Feud: OreCrusher + complex meshes + heightfield | **done** |
| **F3–F4** | Live review + fix | **done** |
| **F5–F6** | Docs / findings / commits | **done** |

- [x] Plan file landed  
- [x] F0–F6 complete  
- [x] Findings: [docs/AGENT_SIGHT_FIDELITY_FINDINGS_2026-07-23.md](docs/AGENT_SIGHT_FIDELITY_FINDINGS_2026-07-23.md)

---

## Agent sight trust — done

Trustworthy factory sight: hierarchy/local-space demotion; `see_pack` camera restore; stronger multi-view; packet↔pixel regression (fovea profile already shipped — T0 verifies).

Plan: [docs/AGENT_SIGHT_TRUST_PLAN.md](docs/AGENT_SIGHT_TRUST_PLAN.md).

| Phase | Name | Status |
|-------|------|--------|
| **T0** | Verify fovea `--profile` fix + cargo test | **done** |
| **T1** | Structural child demotion (hierarchy / local-space) | **done** |
| **T2** | `see_pack` always restores camera | **done** |
| **T3** | Stronger multi-view (dedicated side path) | **done** |
| **T4** | Packet↔pixel regression tests + workflow | **done** |
| **T5** | Live CD + IF / agent-sight-dogfood full | **done** |
| **T6** | Docs / findings / commits | **done** |

- [x] Plan file landed  
- [x] T0–T6 complete  
- [x] Findings: [docs/AGENT_SIGHT_TRUST_FINDINGS_2026-07-25.md](docs/AGENT_SIGHT_TRUST_FINDINGS_2026-07-25.md)  
- [x] Workflow full PASS (agent-sight-dogfood-3, skeptic green)

---

## Agent sight soft edges — done

Residual soft spots after trust: hierarchy BRP, multi-view camera, fovea craft readability, CD env_2d packs, non-silent CI. Closeout requires **workflow mode=full**.

Plan: [docs/AGENT_SIGHT_SOFT_PLAN.md](docs/AGENT_SIGHT_SOFT_PLAN.md).

| Phase | Name | Status |
|-------|------|--------|
| **E0** | Baseline tests + soft evidence | **done** |
| **E1** | Hierarchy (S1) ChildOf second-query merge | **done** |
| **E2** | Multi-view (S2) dedicated-view helper + honest nudge path | **done** |
| **E3** | Fovea craft (S3) `fovea_dark` warning | **done** |
| **E4** | CD env_2d (S4) wait + recapture | **done** |
| **E5** | CI matrix doc | **done** |
| **E6** | `/agent-sight-dogfood` **mode=full** PASS | **done** |
| **E7** | Findings / docs / commits | **done** |

- [x] Plan file landed  
- [x] E0–E7 complete  
- [x] Findings: [docs/AGENT_SIGHT_SOFT_FINDINGS_2026-07-25.md](docs/AGENT_SIGHT_SOFT_FINDINGS_2026-07-25.md)  
- [x] CI matrix: [docs/AGENT_SIGHT_CI_MATRIX.md](docs/AGENT_SIGHT_CI_MATRIX.md)  
- [x] Workflow full PASS (agent-sight-dogfood-4)

---

## Agent sight still soft — planned

Soft E0–E7 shipped, but residual soft edges need **re-review + investigate** (not closed forever). Includes S3 skeptic gap (dark fovea warning theater). Closeout requires **`/agent-sight-dogfood` mode=full** (workflow default is already `full`).

Plan: [docs/AGENT_SIGHT_STILL_SOFT_PLAN.md](docs/AGENT_SIGHT_STILL_SOFT_PLAN.md).

| Phase | Name | Status |
|-------|------|--------|
| **SS0** | Baseline + re-open soft register | **planned** |
| **SS1** | Hierarchy (review + investigate) | **planned** |
| **SS2** | Multi-view (review + investigate) | **planned** |
| **SS3** | Craft in fovea / skeptic gap | **planned** |
| **SS4** | CD env pack (review + investigate) | **planned** |
| **SS5** | Not silent CI forever | **planned** |
| **SS6** | `/agent-sight-dogfood` **mode=full** | **planned** |
| **SS7** | Findings / docs / commits | **planned** |

- [x] Plan file landed  
- [ ] SS0–SS7 complete
