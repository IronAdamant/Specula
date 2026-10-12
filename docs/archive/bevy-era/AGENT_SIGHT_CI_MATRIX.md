# Agent sight — CI / regression matrix (S5 / SS5)

**Still soft:** [AGENT_SIGHT_STILL_SOFT_PLAN.md](AGENT_SIGHT_STILL_SOFT_PLAN.md) re-opens this bar — **do not treat GH unit CI as “sight green forever.”**

## Always (headless, no GPU window required)

```bash
cargo test -p specula -p specula-brp
```

Covers: ranking, structural demotion, fovea projection / **craft-dark gates**, pack restore helpers, multi-view placement pure math, magenta/nonblack gates.

Wire this in CI for every PR (see `.github/workflows/ci.yml` — unit tests only by design).

## Live bar (GPU / window required) — release & still-soft closeout

```text
/agent-sight-dogfood
# mode MUST be full (not gates-only).
# Workflow default in agent-sight-dogfood.rhai is already mode = "full".
```

| Check | Expected |
|-------|----------|
| Gates | cargo test PASS |
| Crystal Drift | primary Player; dogfood Name present; no magenta plates; env_2d honest |
| Iron Feud | Playing; primary ≠ OreCrystal*; OreCrusher in subjects; entity `--profile iron-feud` topdown aim |
| Fovea craft | readable crop **or** packet `fovea_dark` (shadow class must warn) |
| Multi-view | game≠alt or honest `views_similar` |
| Pack restore | notes `[restored]` / camera back to game pose |
| Skeptic | green preferred |

## Not claimed

- Headless fake “screenshot CI” without a real Bevy window  
- Parallel dual games on BRP 15702  
- “Soft plan once green ⇒ never re-run full workflow”

## Policy

Unit tests prevent silent ranking/projection/fovea-gate rot. Full workflow (`mode=full`) is the **required** live sight bar for:

- [still-soft](AGENT_SIGHT_STILL_SOFT_PLAN.md) plan closeout (SS6)  
- soft-edge / trust / fidelity closeouts historically  
- any claim of “sight green” after eyesight platform changes  

`mode=gates` is debug-only. Do not change the workflow default away from `full`.
