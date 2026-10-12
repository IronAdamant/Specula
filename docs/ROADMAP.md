# Roadmap — agent-native game factory (alpha)

**Status:** alpha public demo.  
**North star:** Specula helps agents **create real short Bevy games**, package them for sharing, and later structure them for Steam—without Steam blocking the first demos.

Design, art direction, and design iteration are **user-owned**. Specula owns **structure, systems kit, agent skills, MCP verify, packaging**.

## Horizons

| Horizon | Meaning |
|---------|---------|
| **D — Demo** | 2D + 3D short games meeting [GAME_DOD.md](GAME_DOD.md) |
| **P — Package** | Zip/folder: binary + `assets/`, non-Steam |
| **S — Steam path** | Checklists + optional stubs; upload is a later process |

## Decisions

- Dogfood **in-repo** (`games/demo-2d`, `games/demo-3d`) **and** scaffold for external projects — **same layout contract**  
- **Both 2D and 3D**  
- **Non-Steam first**, Steam-ready structure later  

## Phases

| Phase | Name | Outcome |
|-------|------|---------|
| **G1** | DoD + skills | Agents know when a demo is “done” |
| **G2** | Templates → short games | **Done** — kits meet GAME_DOD |
| **G3** | In-repo dogfood | **Done** — `games/demo-2d`, `games/demo-3d` |
| **G4** | Packaging | **Done** — `scripts/package-demo.sh` + docs/skill |
| **G5** | Steam path | Docs/skill only until packaging works |
| **G6** | Install ergonomics | **Done** — templates embedded in CLI; `SPECULA_TEMPLATE_ROOT` optional override |

```text
G1 ──► G2a (2D kit) ──► G3a (demo-2d)
    └► G2b (3D kit) ──► G3b (demo-3d)
G2 ──► G4 package ──► G5 steam (thin)
G6 parallel after G2
```

## Agentic loop

Skills (HOW) → kit/templates (WHERE) → dogfood or scaffold (BUILD) → **agent eyesight** (see / judge / re-see) → package (SHIP DEMO) → later Steam checklist.

MCP capture is the optic nerve; **agent eyesight** (entity, landscape, water, physics-as-motion) baseline is [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) (V0–V6 shipped). **Acuity 20/20** (true fovea, multi-view, clean subjects, temporal reliability) is [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md) — still agent senses, **not** a Bevy editor; taste/design stay human-owned. Execute with `/goal` against those docs.

## Tracking

Checklist: [PROGRESS.md](../PROGRESS.md) (v0.3+).  
Eyesight baseline: [AGENT_EYESIGHT_PLAN.md](AGENT_EYESIGHT_PLAN.md) (V0–V6).  
Eyesight 20/20: [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md) (A0–A8).  
Agent sight next (profiles, ranking, dogfood): [AGENT_SIGHT_NEXT_PLAN.md](AGENT_SIGHT_NEXT_PLAN.md) (S0–S4 shipped).  
Agent sight **2D + 3D** (profiles/packs, CD+IF height terrain): [AGENT_SIGHT_2D3D_PLAN.md](AGENT_SIGHT_2D3D_PLAN.md) (D0–D5 shipped; findings [AGENT_SIGHT_2D3D_FINDINGS_2026-07-22.md](AGENT_SIGHT_2D3D_FINDINGS_2026-07-22.md)).  
Agent sight **debt** (R0–R4 shipped): [AGENT_SIGHT_DEBT_PLAN.md](AGENT_SIGHT_DEBT_PLAN.md) — residual filter/multi-view/fovea; full CD+IF asset+env improve; findings [AGENT_SIGHT_DEBT_FINDINGS_2026-07-23.md](AGENT_SIGHT_DEBT_FINDINGS_2026-07-23.md).  
Agent sight **hardening** (H0–H6 shipped): [AGENT_SIGHT_HARDENING_PLAN.md](AGENT_SIGHT_HARDENING_PLAN.md) — Tier 1–3 gates; CD PulseMine + transparent sprites; IF LoadingBay + multi-part drills/belts + heightfield; findings [AGENT_SIGHT_HARDENING_FINDINGS_2026-07-23.md](AGENT_SIGHT_HARDENING_FINDINGS_2026-07-23.md).  
Agent sight **fidelity** (F0–F6 shipped): [AGENT_SIGHT_FIDELITY_PLAN.md](AGENT_SIGHT_FIDELITY_PLAN.md) — Tier 1–3; CD CargoPod + transparent sprites; IF OreCrusher + multi-part craft + heightfield; findings [AGENT_SIGHT_FIDELITY_FINDINGS_2026-07-23.md](AGENT_SIGHT_FIDELITY_FINDINGS_2026-07-23.md).  
Agent sight **workflow**: `.grok/workflows/agent-sight-dogfood.rhai` — platform gates + sequential CD/IF dogfood + skeptic (`/agent-sight-dogfood`).  
Agent sight **trust** (T0–T6 shipped): [AGENT_SIGHT_TRUST_PLAN.md](AGENT_SIGHT_TRUST_PLAN.md) — structural demotion, pack restore, dedicated multi-view; findings [AGENT_SIGHT_TRUST_FINDINGS_2026-07-25.md](AGENT_SIGHT_TRUST_FINDINGS_2026-07-25.md).  
Agent sight **soft edges** (E0–E7 shipped): [AGENT_SIGHT_SOFT_PLAN.md](AGENT_SIGHT_SOFT_PLAN.md) — ChildOf merge, fovea_dark, env_2d retry, CI matrix; findings [AGENT_SIGHT_SOFT_FINDINGS_2026-07-25.md](AGENT_SIGHT_SOFT_FINDINGS_2026-07-25.md); full workflow bar.  
Agent sight **still soft** (SS0–SS7 planned): [AGENT_SIGHT_STILL_SOFT_PLAN.md](AGENT_SIGHT_STILL_SOFT_PLAN.md) — re-review hierarchy, multi-view, craft-in-fovea (skeptic gap), CD env pack, non-silent CI; **mandatory** `/agent-sight-dogfood` `mode=full`.  
Session plan detail: maintained in agent sessions; this file is the **durable product roadmap**.
