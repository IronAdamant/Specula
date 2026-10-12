# Specula — plan

**Specula** (Latin: *watchtower, lookout*; the root of "speculate"). Formerly **Grok-Bevy**.
Agent-native tooling for Bevy games: scaffold, live control over BRP, and **sight** — so any
MCP-capable agent (Claude, Grok, …) can see a running game or an isolated asset and judge it.

**North star:** the game *tells*, the tool *asks*, the model *judges*.
Today the tool guesses what is on screen from Name heuristics and hard-coded view extents;
Specula moves ground truth into an in-game plugin and keeps the MCP side thin.

Status: **Phase 0 done (uncommitted), Phase 1 mostly done.** GitHub repo renamed to `IronAdamant/Specula`;
`specula.exe` installed; Grok config repointed (backup `~/.grok/config.toml.bak-pre-specula`).
**Open decision (2026-10-12):** stay on Bevy vs. build a native zero-third-party-dependency agent-native engine — see chat.

**Windows build note:** default toolchain is `stable-x86_64-pc-windows-gnu`; LLVM-MinGW's `gcc` (first on PATH) lacks libgcc,
and MSVC build tools are not installed. Build with
`CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=<WinLibs>/mingw64/bin/x86_64-w64-mingw32-gcc.exe` (or reorder PATH / install MSVC).

---

## Phase 0 — Rename to Specula

Surface (excluding `target/`): ~78 files mention grok; ~370 `grok-bevy`/`Grok-Bevy` hits.

| Old | New |
|---|---|
| Repo `IronAdamant/Grok-Bevy` | `IronAdamant/Specula` (GitHub redirects old URLs) |
| Crate `grok-bevy` (CLI + MCP, binary `grok-bevy`) | `specula` (binary `specula`) |
| Crate `grok-bevy-brp` | `specula-brp` |
| Crate `grok-bevy-env` | `specula-env` |
| New crate (Phase 2) | `specula-sight` (Bevy plugin) |
| New crate/bin (Phase 3) | `specula studio` subcommand |
| `grok_bevy_sample` (sample-app package) | `specula_sample` |
| Schema `grok-bevy.eyesight/v1` | `specula.sight/v1` (accept old id on read for one release) |
| Env `GROK_BEVY_TEMPLATE_ROOT` | `SPECULA_TEMPLATE_ROOT` (fallback to old for one release) |
| `grok_bevy_root` / `grok_bevy_version` keys | `specula_root` / `specula_version` |
| `.grok/skills`, `.grok/workflows` | canonical `skills/` + generated `.claude/skills` and `.grok/skills` (Phase 1) |
| MCP tool names `bevy_*` | keep — they describe Bevy, not the product |

- [x] Rename crates, binary, package names, schema id, env vars (with old-name fallbacks)
- [~] README/docs: rebrand (intro + skills sections done; full rewrite in Phase 4); positioning = agent-agnostic (Claude Code, Grok Build, Cursor, …)
- [x] `cargo install --path crates/specula --force`; uninstall old `grok-bevy.exe` from `~/.cargo/bin`
- [x] Update `~/.grok/config.toml` `[mcp_servers.grok-bevy]` → `specula` (currently points at `grok-bevy.exe`)
- [x] Rename GitHub repo + `git remote set-url origin https://github.com/IronAdamant/Specula.git` (**confirm first**)
- [ ] Optional: reserve `specula` crate names on crates.io (all free as of 2026-10-12)
- [x] `cargo test` green after rename

## Phase 1 — Claude Code compatibility (first, so Claude can drive the rest)

- [x] `.mcp.json` (project scope): `claude mcp add --scope project specula -- specula mcp`
- [x] `CLAUDE.md` containing `@AGENTS.md` (single source of agent rules)
- [x] Skills: one canonical `skills/` folder; sync step writes `.claude/skills/` and `.grok/skills/`
      (no symlinks — Windows needs Developer Mode for those). Frontmatter already compatible.
- [ ] Image sizing: full frames downscaled to ≤1568 px long edge for MCP image content; crops at native
      res; `abs_path` always points at full-res file (2560×1440 captures currently get auto-downscaled)
- [x] Skeptic workflow (`.grok/workflows/agent-sight-dogfood.rhai`) → Claude slash command +
      `.claude/agents/sight-skeptic.md` verifier subagent (fresh context, packet-vs-pixel check)
- [ ] Optional: package as a Claude Code plugin (skills + MCP config + agent) for one-step install

## Phase 2 — `specula-sight`: in-game sight plugin

Game-side crate, enabled under the game's `remote` feature. MCP-side crates stay Bevy-free.

### Subjects declared by the game

```rust
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct SightSubject {
    pub role: SightRole,   // Player, Enemy, Pickup, Hazard, Prop, Terrain, Water, Ui, Fx
    pub priority: i32,
}
```

Tag top-level entities only; children never become subjects. Untagged games fall back to a
generic Name+Transform path with **no game-specific names** in platform code.

### BRP methods

| Method | Returns | Replaces |
|---|---|---|
| `sight/subjects` | entity, name, role, world AABB (descendant `Aabb` × `GlobalTransform`), exact screen rect (8 corners via `Camera::world_to_viewport`), on-screen, active camera projection + viewport | `world_to_screen_ortho`, per-game visible-half extents, profiles |
| `sight/frame` | subjects **+** screenshot from the same frame | separate query-then-screenshot (drift for moving things) |
| `sight/orbit` | temp offscreen camera (`RenderTarget::Image`) orbiting a subject's AABB, N angles, then despawn | StrategyCamera nudge/restore chain, `views_similar` fallbacks |
| `sight/time` | pause / step N / resume (`Time<Virtual>`) | non-deterministic motion strips |
| `sight/debug_light` | one capture with lifted exposure + fill light, labelled not-game-look | luma-threshold tuning as primary signal |

Plus `SPECULA_SEED` env var so procedural worlds are identical across runs (diffs compare like with like).

### Deletes from `eyesight.rs` (est. ~half the file)

`DOGFOOD_NAME_STEMS`, `GAMEPLAY_NAME_HINTS`, `gameplay_subject_score`, structural/co-located demotion,
ChildOf merge, `TALL_ENTITY_PREFIXES`, `apply_game_profile` extents, camera nudge/restore.
**Keep:** PNG crop/montage/diff, packet schema, honest warnings (as metadata), packs (re-composed from new methods).

### Verified against Bevy 0.19 docs (2026-10-12)

- Custom methods: `RemotePlugin::default().with_method_main(name, handler)`; handler is a system
  `In<Option<Value>> -> BrpResult`. Watching variant: `with_watching_method_main` → `BrpResult<Option<Value>>`.
- `BrpExtrasPlugin` skips adding `RemotePlugin`/`RemoteHttpPlugin` if already present → add ours first.
- `Camera` has no `target` field; `RenderTarget` is a separate (required) component with an `Image` variant.
- `Camera::world_to_viewport(&self, &GlobalTransform, Vec3) -> Result<Vec2, ViewportConversionError>`.

### Open questions (verify before building)

- [ ] Screenshot completes frames after the BRP call returns → ticket + poll, or watching method?
      Mirror how `brp_extras/screenshot` does it.
- [ ] `Screenshot::image(handle)` on an offscreen `RenderTarget::Image` in 0.19 — confirm API.
- [ ] `Aabb` module path in 0.19 (`bevy::camera::primitives::Aabb`?).

### Tasks

- [ ] Crate skeleton + `SightSubject` + `sight/subjects`
- [ ] `sight/frame` (same-frame capture)
- [ ] `sight/orbit`, `sight/time`, `sight/debug_light`
- [ ] MCP `bevy_see_*` rewired to sight methods when present; generic fallback otherwise
- [ ] Delete heuristics listed above; unit tests for projection + AABB union
- [ ] Templates (`game-2d`, `game-3d`) + demos tag their subjects
- [ ] Live proof on one 2D and one 3D game (open PNGs; packet ↔ pixels)

## Phase 3 — Asset studio (`specula studio <asset.glb> [--style art/style.toml]`)

Minimal Bevy app, no game code: load glTF, frame from AABB, render, write outputs, exit.

**Stage (fixed, so comparisons mean something):** neutral mid-grey backdrop, shadow-catcher floor,
3-point light + environment map, fixed exposure, **scale refs** (1.8 m human silhouette, 1 m grid).

**Contact sheet (one PNG):**

| Panel | Catches |
|---|---|
| 8-angle orbit @ 30° + top + front | form, unmodelled backs |
| Close-up | surface detail, texel density |
| Silhouette (black on white) | readability |
| **Game-distance thumbnail** (on-screen size, e.g. 64 px) | wasted detail; the key taste check |
| Wireframe (`WireframePlugin`) | density, bad topology |
| UV checker | stretching, seams |

**Stats JSON:** tris, verts, materials, texture sizes, dimensions (m), pivot-at-base, missing textures.

**Modes:** single · before/after · **lineup** (whole prop set together — style-drift check).

**`style.toml`** — hard rules checked by the studio, soft rules judged by the model; human owns final say:

```toml
[palette]  colors = ["#2b2d42", "#8d99ae", "#ef233c"]  max_deviation = 0.15
[budget]   prop = 3000   character = 12000   hero = 25000   # triangles
[scale]    unit = "m"    player_height = 1.8
[shading]  roughness = [0.4, 0.9]   metallic = "rare"
[notes]    silhouette = "chunky, readable at 64px; no thin spikes"
```

**Pipeline:** Blender `bpy` script → `.glb` → studio → model reads contact sheet → edit script → repeat.

- [ ] Install Blender (not on PATH as of 2026-10-12); headless runner (`blender -b --python …`)
- [ ] Studio stage + contact sheet + stats JSON
- [ ] `style.toml` hard checks (budget, scale, palette drift)
- [ ] Before/after + lineup modes
- [ ] MCP tool `specula_studio` returning sheet + stats
- [ ] Proof: 3 generated props through the full loop

## Phase 4 — Docs and process cleanup (lands with Phase 2)

- [ ] Move the 20 `AGENT_SIGHT_*` / `AGENT_EYESIGHT_*` plan+findings docs to `docs/archive/`
- [ ] `docs/HISTORY.md` — one line per old phase (V, A, S, D, R, H, F, T, E, SS) with what it established
- [ ] `docs/SIGHT.md` — architecture, current guarantees, known limits (single source)
- [ ] Agent-loop skill: 26 rules → ~8 (drop rules that only patch deleted heuristics)
- [ ] Replace absolute `/Users/aron/...` paths with `specula.toml` (dogfood game paths, ports)
- [ ] Split `eyesight.rs` (4148 lines) → `image`, `packet`, `subjects`, `packs`, `see` modules
- [ ] Track future work as GitHub issues or one living plan, not letter-coded micro-phases

## Phase 5 — Original game

- [ ] Write `style.toml` **before** any assets
- [ ] Scaffold with `specula scaffold --kind 3d`; tag subjects from day one
- [ ] Asset loop through the studio; in-game verification through sight

---

## Order

Phase 0 (rename) → Phase 1 (Claude Code) → Phase 2 (sight) + Phase 4 (cleanup) → Phase 3 (studio) → Phase 5 (game).

| Date | Change |
|---|---|
| 2026-10-12 | Plan created; repo pulled at `2efad7b`; local folder renamed Grok-Bevy → Specula |
| 2026-10-12 | Phase 0 rename executed; Phase 1 config (.mcp.json, CLAUDE.md, skills sync, sight-skeptic agent); 111 tests green |
