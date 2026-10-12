# Specula

*Specula* — Latin for *watchtower*. Formerly **Grok-Bevy**.

**Agent-native tooling for building production [Bevy](https://bevy.org) games in Rust.** Bevy has no GUI editor — the game *is* code — which makes it the natural engine for AI agents to build in. Specula gives any agent that speaks **MCP (Model Context Protocol)** — [Claude Code](https://claude.com/claude-code), [Grok Build](https://x.ai), Cursor, and others — what it needs to work like a game developer rather than a code generator: scaffolds for real games, **live eyes and hands** on the running game, and (planned) an asset studio for judging 3D assets in isolation.

> **Status: alpha, being reworked.** See [docs/SPECULA_PLAN.md](docs/SPECULA_PLAN.md) — in-game sight plugin, asset studio, Claude Code support. The eyesight sections below describe the current implementation.

### Portfolio showcase (2D vertical slice)

External dogfood: a standalone Bevy **0.19** mining-drone game (**Crystal Drift**) was scaffolded with `specula scaffold --kind 2d`, then iterated with the agent loop. Placeholder art is fine — the point is a **real playable loop** plus **live MCP control**.

![Crystal Drift gameplay — score, fuel, player, asteroids, mining laser](docs/images/crystal-drift-play.png)

*Live play capture via `brp_extras/screenshot` / `bevy_capture_viewport` (features `remote,capture`).*

![Agent MCP loop — BRP query, mutate player translation, re-query, screenshot](docs/images/crystal-drift-mcp-loop.png)

*Portfolio MCP sequence: **query → mutate → re-query confirm → screenshot** on a warm binary (BRP port **15702**).*

This slice launches **straight into Playing** (no main menu) by design. In-repo **`demo_2d` / `demo_3d`** still follow the full [GAME_DOD](docs/GAME_DOD.md) (menu, objective, win/lose, pause).

---

## Who this is for

| You might be… | Specula helps you… |
|---------------|----------------------|
| New to Bevy, using **Grok Build** or another AI coding assistant | Get a ready machine + a real starter game without memorizing every setup step |
| Building a game with an **LLM** (Grok, Claude, GPT, local models, etc.) | Give the agent tools so it can *see* and *control* the live game, not only edit code |
| Looking for an **MCP server for Bevy** | Plug into Grok Build, Cursor, Claude Code, Continue, Cline, Windsurf, or other MCP clients |
| Prototyping a **2D platformer / top-down game** or a small **3D** scene | Scaffold a playable vertical slice (menu → play → move) with remote control hooks |

You do **not** need to be a Rust expert to start. You will need a normal developer setup (Rust installed, ability to run terminal commands). The `doctor` command explains what’s missing in plain language.

---

## What you get in plain English

1. **“Is my PC ready for Bevy?”** — `specula doctor` checks Rust and your OS, then suggests fixes.  
2. **“Start a game project for me.”** — Scaffold a **2D** or **3D** starter (or a simple demo cube for testing).  
3. **“Let my AI play with the running game.”** — An **MCP server** so the agent can query the world, tweak values, and **see the viewport** (screenshots + structured eyesight packets).  
4. **“How should the AI build a real game?”** — Built-in **skills** (playbooks for Claude Code and Grok Build) for production structure, not one-file demos.

**Skills define how to build. Scaffold defines where files go. MCP verifies what is on screen.**

### Agent eyesight — what it is (and is not)

Specula gives agents (and any human using the same MCP tools) **live eyes on a running Bevy window**: full-scene capture, named subjects, true-fovea entity crops, motion strips, multi-view packs, baselines/diffs, and diagnostic bounds (`bevy_see_*` / `specula see`). **Observation-grade ~20/20 acuity** is implemented (state gate, world→screen fovea, subject filter, multi-view, temporal notes) — see [docs/AGENT_EYESIGHT_20_20_PLAN.md](docs/AGENT_EYESIGHT_20_20_PLAN.md) (baseline: [docs/AGENT_EYESIGHT_PLAN.md](docs/AGENT_EYESIGHT_PLAN.md)).

**Honest scope:**

| The tooling does | The tooling does **not** |
|------------------|---------------------------|
| Let the agent **see** what the player sees | Own **taste**, art direction, or “is this good design?” |
| Bind pixels to **named entities** and motion | Replace a human creative director |
| Help the agent **build to your requirements** with evidence | Auto-beautify or score beauty |
| Serve **human devs** who drive the same MCP | Turn Specula into a Bevy **editor** |

**You own the creative direction.** The planned `style.toml` captures it (palette, budgets, scale, silhouette rules): Specula checks the hard rules automatically, the agent judges the soft ones against it, and you have the final say.

---

## Why MCP + Bevy matters for AI game development

Most AI coding chats only see source files. Game work also needs:

- Does it **compile and run** on this machine?  
- What does the **player actually see**?  
- Can the agent **change a transform** and re-check without guessing?

Specula wires that loop using the **Bevy Remote Protocol (BRP)** and optional [bevy_brp_mcp](https://github.com/natepiano/bevy_brp) / [bevy_brp_extras](https://crates.io/crates/bevy_brp_extras). Your agent (Grok Build or another MCP-compatible LLM client) can treat a live Bevy app like a controllable environment: **launch → wait BRP → see (scene/entity/motion/pack) → mutate → re-see → fix → repeat**.

That makes this project useful if you are searching for phrases like:

- *Bevy MCP server* / *MCP for Bevy game engine*  
- *AI coding agent for game development*  
- *Grok Build Bevy* / *LLM-assisted Rust game dev*  
- *screenshot the Bevy window for an AI assistant*  
- *scaffold a 2D or 3D Bevy game with remote control*

---

## Quick start (about 10 minutes)

### Prerequisites

- A computer that can run games (a normal GPU + window is best for screenshots)  
- [Rust](https://rustup.rs) (`rustc` + `cargo`)  
- Optional but recommended: [Grok Build](https://x.ai) or any **MCP client** for AI agents  

### 1. Install the CLI

```bash
git clone https://github.com/IronAdamant/Specula.git
cd Specula
cargo install --path crates/specula
```

### 2. Check your machine

```bash
specula doctor
```

You want a **READY** report. If something is missing, follow the printed install tips for Windows, macOS, or Linux.

### 3. Connect the MCP server (so your AI can use it)

**Grok Build** — add to `~/.grok/config.toml` (or run `grok mcp add …`):

```toml
[mcp_servers.specula]
command = "specula"   # or the full path from `which specula`
args = ["mcp"]
enabled = true
startup_timeout_sec = 30
```

Optional richer Bevy control (hierarchy, watches, input injection):

```bash
cargo install bevy_brp_mcp --locked
```

```toml
[mcp_servers.bevy-brp]
command = "bevy_brp_mcp"
args = []
enabled = true
```

Print ready-made config:

```bash
specula mcp-config
```

**Other MCP clients** (Cursor, Claude Code, Continue, Cline, etc.): register a stdio MCP server that runs `specula mcp` the same way you register any local MCP tool.

Restart or reload MCP so the agent sees tools like `bevy_env_check`, `bevy_launch_app`, `bevy_wait_brp`, `bevy_capture_viewport`, and `bevy_workflow`.

### 4. Play the in-repo short demos (dogfood)

These are **real short games** (objective, hazard, win/lose) — the default dogfood path:

```bash
# 2D: collect 3 orbs, avoid the red hazard
cargo run -p demo_2d --features remote,capture

# 3D: collect 3 cubes, avoid hazard / don't fall off
cargo run -p demo_3d --features remote,capture
```

Package a playable folder/zip (binary + `assets/`):

```bash
./scripts/package-demo.sh demo_2d games/demo-2d
./scripts/package-demo.sh demo_3d games/demo-3d
```

See [docs/GAME_DOD.md](docs/GAME_DOD.md), [docs/PACKAGING.md](docs/PACKAGING.md), [docs/ROADMAP.md](docs/ROADMAP.md).

### 5. Or scaffold your own game

```bash
specula scaffold --kind 2d --path ./my-first-game
cd my-first-game
cargo run --features remote,capture
```

- **2D / 3D** — GAME_DOD short demos (menu, collect, hazard, victory/game over).  
- **`demo`** — static cube for BRP plumbing only (not a product game).

Controls and objectives are in each game’s README.
### 6. Let the agent look at the game

With the game running (`remote,capture` features) and MCP connected, ask your assistant something like:

> Check Bevy readiness, wait for the game on port 15702, query the scene, mutate the player if needed, and capture a screenshot of the window.

**Cold first compile:** prefer shell `cargo run --features remote,capture` (or `cargo build` then run). **MCP** `bevy_launch_app` is non-blocking by default — follow with **`bevy_wait_brp`** (e.g. `timeout_secs` 180 cold / 30 warm). Warm binary + query → mutate → re-query → capture is the portfolio-proven loop above.

Or: use the MCP prompt **`start_2d_game`** / **`start_3d_game`**, or the **`bevy_workflow`** tool with goal `new_2d`, `new_3d`, `verify_scene`, or `ship`.

More detail: [docs/FAST_START.md](docs/FAST_START.md) · [docs/PRODUCTION_GAMES.md](docs/PRODUCTION_GAMES.md)

---

## What the AI can do (MCP tools, human labels)

| Tool (name) | Everyday meaning |
|-------------|------------------|
| `bevy_env_check` | Is this computer ready to build Bevy games? |
| `bevy_launch_app` | Start my game (non-blocking; prefer after a warm build) |
| `bevy_wait_brp` | Wait until BRP answers on the port |
| `bevy_brp_query` | What entities exist? (aliases `Name`/`Transform` or FQNs) |
| `bevy_brp_mutate` | Change a live field (e.g. Transform translation) |
| `bevy_capture_viewport` | **Screenshot the game window** for the agent to “see” |
| `bevy_workflow` | Step-by-step plan: which skills + tools for a goal (router, not autopilot) |
| `bevy_brp_mcp_status` | Is the optional full Bevy BRP MCP installed? |

**MCP prompts** (shortcuts for agents): `start_2d_game`, `start_3d_game`, `iterate_scene`, `prepare_ship`.

---

## Skills (playbooks for building real games)

Canonical source is `skills/`; `python scripts/sync_skills.py` mirrors them to `.claude/skills/` (Claude Code) and `.grok/skills/` (Grok Build) so both load them automatically:

| Skill | When it helps |
|-------|----------------|
| `bevy-production` | Project layout, states, shipping mindset |
| `bevy-2d-game` | 2D camera, sprites, movement |
| `bevy-3d-game` | 3D camera, lights, meshes |
| `bevy-agent-loop` | Live run → screenshot → fix loop |

For **game art**, pair with Grok’s image skills (e.g. `game-asset-core`) and drop files under `assets/sprites`, `assets/models`, `assets/ui`, or `assets/audio`. See [docs/ASSET_CONVENTIONS.md](docs/ASSET_CONVENTIONS.md) and [docs/SHIPPING.md](docs/SHIPPING.md).

---

## Compatibility

| Piece | Version |
|-------|---------|
| Bevy game engine | **0.19** |
| bevy_brp_mcp / bevy_brp_extras | **0.22.1** |
| Specula | **0.2.x** (game factory alpha continues on `main`) |

Bevy is a **Cargo** dependency of your game project—not a separate installer. “Ready” means this machine can **compile and run** Bevy apps.

---

## Project map (for the curious)

```
Your AI (Grok Build / Claude / Cursor / …)
        │  MCP (stdio)
        ▼
   specula mcp  (+ optional bevy_brp_mcp)
        │  Bevy Remote Protocol (HTTP)
        ▼
   Your Bevy game (features: remote, capture)
```

| Piece | Role |
|-------|------|
| `specula` CLI | doctor, scaffold, BRP helpers, MCP entry |
| MCP server | agent tools + prompts + workflow router |
| `templates/game-2d`, `game-3d` | playable starters |
| `templates/sample-app` | remote-control test fixture (cube) |
| `skills/` | production playbooks (mirrored to `.claude/skills/`, `.grok/skills/`) |

---

## CLI cheat sheet

```text
specula doctor [--compile-probe] [--json]
specula mcp
specula scaffold --kind 2d|3d|demo --path DIR [--name NAME]
specula brp discover|query|mutate|call|screenshot|wait
specula mcp-config
specula compat
```

---

## Troubleshooting (short list)

Full guide: [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)

- **Doctor not READY** — install Rust via rustup; follow OS tips (Xcode CLT / MSVC / Linux packages).  
- **Agent can’t connect** — game must run with `--features remote,capture`; default port **15702**.  
- **Launch “timed out”** — cold Bevy compile can exceed MCP tool budgets; use shell for the first build, then `bevy_wait_brp`.  
- **Black screenshot** — keep the game window visible (not minimized).  
- **Query conflict (Bevy B0001)** — overlapping `Query<&mut T>`; use `Without`, markers, or `ParamSet` when growing systems.  
- **First compile is slow** — normal for Bevy; later builds are faster.  

---

## License

**MIT OR Apache-2.0** (same dual license style as Bevy).

- [LICENSE-MIT](LICENSE-MIT)  
- [LICENSE-APACHE](LICENSE-APACHE)  

## Credits

- [Bevy](https://bevy.org) — game engine and Remote Protocol  
- [natepiano/bevy_brp](https://github.com/natepiano/bevy_brp) — `bevy_brp_mcp` and `bevy_brp_extras`  
- [Grok Build](https://x.ai) / xAI — agent-first coding environment this project targets first  

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). This public demo favors a **small, tested surface**: readiness, scaffold, MCP/BRP control, and clear agent docs.

---

### Keywords for discovery (humans and agents)

Specula · Grok Build · MCP server · Model Context Protocol · Bevy game engine · Bevy Remote Protocol · BRP · AI game development · LLM coding agent · Rust game engine · 2D game scaffold · 3D game scaffold · agent screenshots · viewport capture · open source Bevy tooling  

If you found this while looking for an **AI-friendly Bevy workflow**, start with `doctor` → register `specula mcp` → `scaffold --kind 2d` → ask your agent to capture the window.
