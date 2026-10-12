# Specula MCP — Assessment of the Updated Surface (2026-07-19)

**Scope:** Agent-facing evaluation of the **updated** Specula MCP (post launch-path fix), informed by live Crystal Drift dogfood the same day, schema inspection of the reloaded server, and comparison with earlier Iron Feud notes.  
**CLI pin observed:** `specula 0.2.0`  
**Author:** Grok Build agent  
**Status:** Durable assessment (not a single-project dogfood log). Related: `AGENT_DOGFOOD_REPORT_CRYSTAL_DRIFT.md`, `AGENT_DOGFOOD_REPORT_IRON_FEUD.md`.

---

## 1. Executive judgment

The updated MCP is a **meaningful step from “BRP thin client that sometimes hangs agents” toward “agent-native Bevy verify loop.”** The decisive product change is the **split of launch vs readiness**:

| Before (Crystal Drift first attempt) | After (reloaded server schema) |
|--------------------------------------|--------------------------------|
| `bevy_launch_app` effectively waited for BRP inside a host tool budget (~120s) | `bevy_launch_app` is **non-blocking** by default (`wait_secs=0`) |
| Cold compile + first window often **timed out the whole MCP call** | Spawn returns; readiness is **`bevy_wait_brp`** with tunable `timeout_secs` (prefer 180 cold / 30 warm) |
| Agents fell back to shell `cargo run` + ad-hoc curl | Shell remains recommended for **first** compile; MCP loop is documented and tool-shaped |

**Overall score for the updated MCP as an agent harness: 4.6 / 5**  
(Up from ~4.2 on Crystal Drift pre-fix, and aligned with Iron Feud’s later “4.5 after launch-path fix” trajectory.)

What it is **not:** a game factory that invents fun, art direction, or balance. It is structure + scaffold + live eyes. That boundary remains correct for alpha.

---

## 2. What “updated” means in practice

### 2.1 Tool surface (as reloaded)

| Tool | Role | Agent reliability |
|------|------|-------------------|
| `bevy_env_check` | Host readiness (rustc/cargo/OS) | Excellent — instant structured READY/not |
| `bevy_launch_app` | Spawn app; optional short BRP wait (cap 60s); default **return immediately** | **Fixed class** of timeout failures |
| **`bevy_wait_brp` (new/emphasized)** | Poll `rpc.discover` until BRP is up or timeout | **Critical** companion to launch |
| `bevy_brp_discover` | OpenRPC / method inventory | Solid |
| `bevy_brp_query` | `world.query` components | Excellent for named entities / transforms |
| `bevy_brp_mutate` | Field mutate by path | Excellent for “teleport / poke state” loops |
| `bevy_brp_call` | Generic JSON-RPC | Escape hatch for `brp_extras/*` |
| `bevy_capture_viewport` | Screenshot via extras | Strong; image embedding in chat can still truncate |
| `bevy_list_targets` / `bevy_register_target` | Multi-instance naming | Good bookkeeping |
| `bevy_workflow` | Goal → ordered skills/steps | Router only — **not autopilot** (description now honest) |
| `bevy_brp_mcp_status` | Optional full-stack install check | Useful; thin surface is enough most days |

### 2.2 Documented agent loop (server blurb)

```text
bevy_env_check
  → bevy_launch_app (non-blocking, wait_secs=0)
  → bevy_wait_brp
  → query / mutate
  → bevy_capture_viewport
```

Cold compile guidance is now first-class:

> Prefer shell `cargo run --features remote,capture` then `bevy_wait_brp`; MCP launch is best after a warm `target/`.

That matches real agent experience and should be treated as **the** happy path for greenfield Bevy 0.19 builds (multi-minute first compile).

---

## 3. Evidence from Crystal Drift (same-day dogfood)

Crystal Drift exercised the **pre-fix** launch path first, then continued with shell dual-launch + full BRP. Outcomes remain valid validation of the **non-launch** tools; launch assessment is dual-era.

### 3.1 What worked extremely well

1. **Scaffold (`specula scaffold --kind 2d`)**  
   Highest leverage tool in the product, even when invoked via CLI: Bevy 0.19, `remote`/`capture`, plugin layout, asset roots, AGENTS.md. Crystal Drift never re-solved project layout.

2. **`bevy_env_check`**  
   Instant confidence before long compiles. Correctly reported macOS + rustc/cargo ready.

3. **BRP query as “eyes”**  
   Named entities proved Playing state without human eyes: `Player`, `MainCamera`, `Asteroid_*`, `LaserBeam`, `HudScore`, stars, enemies, crystals. Count histograms are enough for many acceptance checks.

4. **Mutate as closed-loop control**  
   Player `Transform.translation` set to `[50, 30, 5]` and re-queried successfully. This is the difference between “screenshot and hope” and “agent can act on the world.”

5. **Screenshot path**  
   `brp_extras/screenshot` / `bevy_capture_viewport` wrote real PNGs under `captures/` (~78KB early, ~137KB after art pass). Sufficient for portfolio and regression.

6. **Thin surface sufficiency**  
   Full `bevy_brp_mcp` was not required for a complete 2D vertical-slice dogfood.

### 3.2 What hurt before the update (and why the update matters)

| Pain | Impact on Crystal Drift | How the update addresses it |
|------|-------------------------|------------------------------|
| `bevy_launch_app` host timeout ~120s | First MCP launch failed mid-wait while cargo/window still starting | Non-blocking spawn + separate wait with 180s cold budget |
| Agents reimplemented wait with shell sleep/curl | Extra friction, inconsistent logs | `bevy_wait_brp` is the shared primitive |
| Ambiguity whether workflow “builds the game” | Risk of over-trusting router | Description: **router, not autopilot** |

### 3.3 Residual friction (still real)

1. **Cold compile cost is physics, not tooling.** Even with perfect MCP, first Bevy 0.19 build can exceed several minutes. Agents must plan for warm vs cold.  
2. **Binary vs monorepo lag.** After MCP source changes, reinstall/reload is mandatory or schemas lie. Session saw a live server blurb update mid-goal — good when reloaded, catastrophic when stale.  
3. **Scaffold into `.` with `--force`** failed removing the existing directory (`EINVAL`); subdir + promote worked. Edge-case polish for empty-but-present workspace roots.  
4. **Capture in chat UI** may truncate image payloads; filesystem path is the durable artifact. Description still says “3D scene” while 2D works fine — minor copy debt.  
5. **Query surface is type-path fragile.** Fully-qualified component paths (`bevy_ecs::name::Name`, `bevy_transform::components::transform::Transform`) require agent knowledge or discover/schema digs. A short “common FQNs” helper would raise success rate.  
6. **GAME_DOD vs product briefs.** Scaffold and skills push menu/pause/win; many vertical slices (Crystal Drift) intentionally skip menu. Agents need a clear “DoD vs OBJECTIVE” rule — not an MCP bug, but a product tension.

---

## 4. Scorecard (updated MCP)

| Capability | Score | Notes |
|------------|------:|-------|
| Env / doctor | 5.0 | Fast, structured, actionable |
| Scaffold (CLI + kit) | 5.0 | Best onboarding in the stack |
| Launch (non-blocking) | 4.5 | Class of failures fixed; still need warm binary hygiene |
| Wait / readiness | 4.7 | `bevy_wait_brp` is the right abstraction |
| Discover | 4.5 | Solid OpenRPC dump |
| Query | 4.8 | Production-useful for agents |
| Mutate | 4.8 | Closed-loop gold |
| Capture | 4.2 | Reliable on disk; chat embed flaky |
| Workflow router | 4.0 | Honest routing; value is skill ordering not automation |
| Optional full BRP MCP | 3.5 | Install optional; thin path wins for demos |
| Docs / skills coupling | 4.5 | Skills + GAME_DOD + agent-loop are coherent |
| **Overall harness** | **4.6** | Ready for serious 2D/3D agent dogfood |

---

## 5. Recommended agent policy (copy into skills / AGENTS)

### Canonical live-verify loop

```text
1. bevy_env_check (compile_probe=false unless diagnosing toolchain)
2. If no warm binary: shell `cargo build --features remote,capture`
3. bevy_launch_app(manifest_path, name, features="remote,capture", wait_secs=0)
   OR shell `cargo run --features remote,capture` in background
4. bevy_wait_brp(target|port, timeout_secs=180 cold | 30 warm)
5. bevy_brp_query(Name, Transform, …) — assert Player / MainCamera / scene markers
6. Optional: bevy_brp_mutate → re-query
7. bevy_capture_viewport(path under project captures/ or session scratch)
8. Fix code → rebuild → from step 3
```

### Acceptance patterns that actually gate

- **Structural:** named entities present; no panic in launch log.  
- **Behavioral (when pure helpers exist):** unit tests on sim/rules without a window.  
- **Visual:** one PNG on black/expected backdrop; do not claim “feels good” from a single frame alone.

### Anti-patterns

- Treating `bevy_workflow` as “implement the game.”  
- Waiting for BRP only via host tool timeout on `bevy_launch_app` with large `wait_secs` (prefer `bevy_wait_brp`).  
- Fabricating captures when BRP is down — log `mcp_env` failure and fall back to structural + unit bar.

---

## 6. Product / roadmap implications

Aligned with `docs/ROADMAP.md` (agent-native game factory alpha):

| Horizon | MCP fit today |
|---------|----------------|
| **D — Demo** | Strong: scaffold + BRP verify + capture |
| **P — Package** | MCP not primary; CLI/scripts + skills |
| **S — Steam** | Out of band; no MCP gap |

### High-value follow-ups (prioritized)

1. **Ensure installed binary always matches monorepo MCP** (version stamp in `bevy_env_check` / `rpc` banner).  
2. **`bevy_brp_query` sugar:** accept short aliases (`Name`, `Transform`) or return common FQNs from discover.  
3. **Launch status resource:** log path + PID + “warm binary used?” in launch response for agent forensics.  
4. **Scaffold force-into-cwd:** handle non-empty root (Crystal Drift empty-dir edge).  
5. **Capture tool copy:** “primary window” not “3D scene only.”  
6. **Optional input simulation** (key send already in extras discover): document a minimal “press W for 0.5s” agent test for movement — high demo value, currently underused.

---

## 7. Comparison: Specula MCP vs “just cargo + curl”

| | Shell only | Updated Specula MCP |
|--|------------|------------------------|
| Scaffold kit | Manual | Excellent |
| Env check | Ad hoc | Structured |
| Launch bookkeeping | DIY PID/logs | Named targets |
| Wait for BRP | Sleep/curl loops | First-class tool |
| Query/mutate ergonomics | Raw JSON-RPC | Typed MCP tools |
| Viewport into agent context | Manual path | Capture tool + image channel |
| Skill/workflow routing | None | `bevy_workflow` |

**Verdict:** For agents, the updated MCP is **worth the registration cost**. Shell remains mandatory for some cold-path and packaging work; MCP owns the **live iteration core**.

---

## 8. Bottom line

The **updated Specula MCP correctly fixes the worst agent failure mode** (blocking launch until BRP under a short host timeout) by making launch **non-blocking** and readiness **explicit** (`bevy_wait_brp`). Combined with already-strong scaffold, query, mutate, and capture, the harness is **production-usable for 2D/3D Bevy 0.19 agent loops** on a warm machine.

Remaining work is polish and ergonomics (FQP names, install freshness, scaffold edge cases, honest capture/docs wording)—not a rethink of architecture.

**Recommended one-line pitch for portfolio / AGENTS.md:**

> Specula MCP: scaffold Bevy 0.19 games, then **launch → wait BRP → query/mutate → screenshot** until the scene is true—without guessing from compile logs alone.

---

## 9. Appendix — portfolio-friendly call pattern

```text
bevy_env_check
bevy_launch_app(
  manifest_path = "<game>/Cargo.toml",
  name = "crystal-drift",
  features = "remote,capture",
  wait_secs = 0
)
bevy_wait_brp(target = "crystal-drift", timeout_secs = 30)  # warm

bevy_brp_query(
  components = [
    "bevy_ecs::name::Name",
    "bevy_transform::components::transform::Transform"
  ]
)
# Expect: Player, MainCamera, Asteroid_*, …

bevy_brp_mutate(
  entity = <Player entity id>,
  component = "bevy_transform::components::transform::Transform",
  path = "translation",
  value = [50.0, 30.0, 5.0]
)

bevy_capture_viewport(path = "captures/play.png")
```

Crystal Drift confirmed this pattern end-to-end (mutate re-query showed `[50, 30, 5]`; captures wrote real PNGs).
