# Agent Eyesight Plan — long-horizon (Specula)

**Status:** implemented (V0–V6 shipped; dogfood Crystal Drift + Iron Feud, 2026-07-21)  
**Audience:** agents implementing Specula infrastructure; humans setting goals  
**Identity:** Specula is **agent assistant infrastructure**, not a Bevy editor.  
**This plan’s job:** give the **agent** reliable **eyesight** (baseline — eyes open).  
**Next (acuity 20/20):** [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md) — observation-grade focus/multi-view/subjects; **taste/design remain human-owned**.

Related:

- Live loop skill: `.grok/skills/bevy-agent-loop/`  
- Physics pins: [PHYSICS.md](PHYSICS.md)  
- Demo completeness: [GAME_DOD.md](GAME_DOD.md)  
- Art discipline: Grok `game-asset-core` (+ specialists)  
- Product roadmap: [ROADMAP.md](ROADMAP.md)  
- Progress tracking: [PROGRESS.md](../PROGRESS.md)  
- 20/20 acuity plan: [AGENT_EYESIGHT_20_20_PLAN.md](AGENT_EYESIGHT_20_20_PLAN.md)

---

## 1. North star

> **Eyesight = sensory evidence the agent must open and judge**, not optional telemetry.

When building or refining a scene — an entity, a landscape, water, a HUD, a jump, a slide — the agent should:

1. **See** the rendered result (pixels).  
2. **Know what** it is looking at (entity / region / state).  
3. **Compare** over time or before/after.  
4. **Judge** for human taste and physical plausibility.  
5. **Act** (assets, materials, lighting, scales, physics params).  
6. **Re-see** until a human glance test would pass.

**Not in scope:** scene graph UIs, gizmo sandboxes, designer property panels, “AI studio” editors.

**In scope:** optic nerve + fovea + short-term visual memory for the agent brain (the model).

---

## 2. Problem statement

### What works today (v0.x baseline)

| Capability | Tool / path | Role |
|------------|-------------|------|
| Full-window screenshot | `bevy_capture_viewport` → `brp_extras/screenshot` | Primary eye |
| Entity/component read | `bevy_brp_query` | Proprioception (names, transforms) |
| Field write | `bevy_brp_mutate` | Hands (pose, small knobs) |
| Input / extras | `bevy_brp_call` (`brp_extras/*`) | Stimulus (keys, etc.) |
| Optional rich BRP | `bevy_brp_mcp` | Complementary depth, not a second product |

Pins: Bevy **0.19**, BRP extras **0.22.1**, default port **15702**, features **`remote,capture`**.

### What is still blind

| Blind spot | Why it hurts taste / physics work |
|------------|-----------------------------------|
| No **subject fovea** (entity / region crop) | “Is this sprite blocky?” lost in full-frame noise |
| No **temporal vision** | Physics, water, animation judged from one lucky frame |
| Weak **what-am-I-looking-at** linkage | Pixels without named subject → wrong fix |
| No standard **before/after** packet | Refinement is guesswork, not comparison |
| Scene types under-served | Landscape, water, lighting need multi-view + motion |
| Skills treat capture as “verify BRP” | Should treat capture as **mandatory aesthetic evidence** |

---

## 3. Design principles (non-negotiable)

1. **Agent eyes, not user editor.** Every feature is justified by: “Does this improve the agent’s visual judgment?”  
2. **Pixels are primary; ECS is secondary.** Query without capture is not eyesight. Capture without subject context is weak eyesight.  
3. **Prefer skill↔tool contract over tool sprawl.** Few stable “see_*” tools; skills teach when and how to open images.  
4. **Evidence over vibes.** Claims like “looks good”, “water feels right”, “jump arc OK” require captures (and preferably diffs or strips).  
5. **Taste needs style intent.** Eyesight is measured against a style/DoD target (pixel scale, painted, low-poly, etc.), not abstract beauty.  
6. **2D and 3D both first-class.** Same eyesight contract; different recommended views.  
7. **Physics is seen, not only simulated.** Colliders and velocities matter; **motion in pixels** is the human test.  
8. **Dogfood in-repo.** Every eyesight phase must run on `games/demo-2d`, `games/demo-3d`, and at least one physics-enabled slice when relevant.

---

## 4. Sensory model (what “seeing” means)

Think of stacked channels. Later phases add channels; earlier ones remain required.

```text
┌─────────────────────────────────────────────────────────┐
│  E0  Full frame capture          composition, lighting  │
│  E1  Subject / region crop       craft of one thing     │
│  E2  Temporal strip / short run  motion, physics, FX    │
│  E3  Before/after pair           refinement judgment    │
│  E4  Multi-view pack             3D / landscape / water │
│  E5  Optional diagnostic frames  unlit / depth-only    │
│  +   Thin context (names, pose,  ground the pixels      │
│      state, on-screen?)                                 │
└─────────────────────────────────────────────────────────┘
```

**Human analogy**

| Human | Agent channel |
|-------|----------------|
| Peripheral vision | E0 full frame |
| Fovea | E1 crop of entity / patch of water / terrain |
| Motion perception | E2 frame strip after stimulus |
| Memory of “before” | E3 baseline capture path |
| Walk around subject | E4 multi-camera / multi-angle |
| Squint / flat light | E5 unlit or simplified shading |
| Knowing object names | BRP `Name` + screen estimate |

---

## 5. Subject classes (what the agent must be able to inspect)

Eyesight is not only “the player sprite.” Long-term subjects:

| Class | Examples | What “looks right” means |
|-------|----------|---------------------------|
| **Entity (character / prop)** | Player, enemy, crate, pickup | Silhouette, scale vs world, blockiness, readability at game size |
| **Landscape / terrain** | Ground plane, tilemap, heightfield, cliffs | Horizon, tiling, scale of features, empty vs busy, camera fit |
| **Water / fluids / surfaces** | Lakes, rivers, oceans, wet ground, lava | Color depth, motion, edge vs land, reflection/spec fake, “cheap sheet” vs readable water |
| **FX / particles** | Splash, dust, muzzle, trail | Timing, opacity, overdraw mud |
| **Lighting / sky** | Directional light, ambient, fog, clear color | Mood, muddy midtones, default-engine look |
| **UI / HUD** | Menus, bars, prompts | Legibility, contrast, not crushing gameplay art |
| **Physics-driven motion** | Jump arc, slide, bounce, stack, projectile | Plausible path, tunneling, jitter, floaty vs snappy, ground stick |

Each class should eventually have:

- recommended **capture pack** (which E0–E5),  
- optional **stimulus** (keys / mutate velocity),  
- **failure signatures** (what bad looks like in pixels).

---

## 6. Physics-as-eyesight

Physics is not a separate product lane; it is a **temporal visual judgment** problem.

### Why BRP alone is insufficient

| Data | Proves | Does not prove |
|------|--------|----------------|
| `Transform` samples | Something moved | Looks snappy / floaty / wrong mass |
| Velocity fields | Numbers changed | Arc feels human-good |
| Collider exists | Setup present | Penetration, jitter, tunneling visible |

### Visual physics checks (agent must see)

1. **Ground contact** — feet/body sit on surface; no hover gap or sink.  
2. **Jump / launch arc** — strip of frames; peak and land readable.  
3. **Slide / friction** — stops or skids as intended.  
4. **Stack / push** — objects don’t explode or melt through.  
5. **Projectile / fall** — path continuous; no single-frame teleport.  
6. **Water interaction** (later) — enter/exit splash or buoyancy looks intentional.  
7. **Camera follow under motion** — motion sickness vs lag vs perfect stick.

### Stimulus contract (for E2)

Standard patterns (implement over phases):

| Stimulus | How | Then |
|----------|-----|------|
| Nudge pose | `bevy_brp_mutate` translation | E0/E1 stills |
| Play input | `brp_extras/send_keys` (discover-first) | E2 strip |
| Force velocity | mutate physics velocity if Reflect+registered | E2 strip |
| Drop test | spawn/reset above ground | E2 strip + land frame |

**Pin reminder:** optional `physics` feature → avian2d/avian3d **0.7** on Bevy 0.19 ([PHYSICS.md](PHYSICS.md)). Kits default transform-based; physics dogfood is explicit.

---

## 7. Agent judgment loop (taste + physics)

Canonical loop for any visual/physics refinement goal:

```text
1. Intent
   - style anchor (pixel / painted / low-poly / …)
   - subject class (entity | landscape | water | physics-motion | …)
   - acceptance language (“readable at play scale”, “water not a flat blue quad”, “jump lands clean”)

2. Baseline eyesight packet
   - launch with remote,capture (+ physics if needed)
   - wait BRP
   - query named subjects
   - E0 full frame (+ E1 crop if subject known)
   - OPEN and read every image path returned

3. Diagnose in human terms
   - blocky / muddy / off-scale / empty / floaty / jitter / sink / tiling / “default cube”
   - separate art fault vs lighting vs camera vs physics

4. Act (smallest change that could fix the look)
   - asset regen / scale / filter / material / light / clear color / collider / damping

5. Compare
   - E3 before/after (same camera, same state if possible)
   - E2 if motion-related

6. Stop when glance test passes
   - would a non-author human say “that’s fine / that reads as water / that jump feels OK”?
```

**Hard skill rule (target state):**  
Any agent claim that something “looks good/bad/right/wrong” **must** reference a capture path (and preferably what was seen in it). No aesthetic pass from code inspection alone.

---

## 8. Target tool surface (stable names)

Prefer a **small** MCP surface. Names are contracts for skills and `/goal` steps.

### Existing (keep, strengthen)

| Tool | Evolution |
|------|-----------|
| `bevy_capture_viewport` | Always return **abs_path + byte size**; agent must read file if image blob truncates |
| `bevy_brp_query` / `mutate` / `call` | Context + stimulus only |
| `bevy_wait_brp` / `bevy_launch_app` | Reliable path to open eyes |

### Proposed (add in phases; do not rename lightly)

| Tool | Purpose | Channels |
|------|---------|----------|
| **`bevy_see_scene`** | Default eyesight packet: full frame + thin context (state if known, named entities + rough screen presence when computable) | E0 + context |
| **`bevy_see_entity`** | Fovea: crop (or tight zoom capture) around a named entity / entity id | E1 + context |
| **`bevy_see_region`** | Fovea for non-entity subjects: screen rect or world AABB (landscape patch, water surface) | E1 |
| **`bevy_see_motion`** | Temporal: apply optional stimulus, capture N frames or short strip over T ms | E2 |
| **`bevy_see_diff`** | Register baseline path; new capture; return both paths (optional simple pixel-diff later) | E3 |
| **`bevy_see_pack`** | Multi-view / multi-preset pack for 3D landscape/water (game cam + top + side; or unlit pair) | E4/E5 |

Implementation may start as **skill-orchestrated sequences** of existing tools, then collapse into real MCP tools once the packet shape stabilizes. Prefer **skill first, tool second** when uncertain.

### Explicit non-goals for tools

- Hierarchy browser as primary UX (use `bevy_brp_mcp` if needed).  
- Live always-on video stream as v1.  
- Click-to-select editor.  
- Auto “beautify” without agent judgment.

---

## 9. Eyesight packet schema (contract)

Stable JSON (or MCP structured content) for `/goal` automation and skill checks.

```json
{
  "schema": "specula.sight/v1",
  "subject_class": "entity|landscape|water|fx|lighting|ui|physics_motion|scene",
  "app_state": "Playing|MainMenu|null",
  "intent": "short human acceptance string",
  "captures": [
    {
      "role": "full|crop|frame|baseline|after|unlit|top|side",
      "abs_path": "/absolute/path.png",
      "bytes": 0,
      "note": "optional"
    }
  ],
  "subjects": [
    {
      "name": "Player",
      "entity": 42,
      "transform": { "translation": [0, 0, 0] },
      "on_screen_estimate": true
    }
  ],
  "stimulus": { "kind": "none|keys|mutate", "detail": {} },
  "agent_must": ["open_and_read_each_capture_image"]
}
```

Rules:

- `abs_path` is required even when an image is embedded in MCP.  
- Agent **must** use image-read capability on each path (or embedded image).  
- Packets live under project `captures/` by convention (`captures/eyesight/…`).

---

## 10. Phased delivery (for `/goal`)

Each phase is **independently shippable**. A `/goal` run should target **one phase** (or one phase slice), dogfood, update this doc’s checklist, and link PROGRESS.

### Phase V0 — Eyesight discipline (docs + skills only)

**Outcome:** Agents treat capture as mandatory eyes, without new MCP tools.

- [x] Extend `bevy-agent-loop` with **Eyesight rules**: open PNG, full-frame before aesthetic claims, before/after for refine loops.  
- [x] Document capture packs for subject classes (entity, landscape, water, physics_motion) in skill references.  
- [x] GAME_DOD / production skills: visual claims need capture evidence.  
- [x] Dogfood: re-run demo-2d + demo-3d verify_scene with explicit “read image” steps.  
- [x] Add PROGRESS section **v0.x Agent Eyesight** pointing here.

**Exit criteria:** A fresh agent session can run verify_scene and *describe what it saw* from captures, not only “PNG written.”

---

### Phase V1 — Reliable see_scene packet

**Outcome:** One-shot evidence pack for “open your eyes on this game.”

- [x] Implement `bevy_see_scene` (or skill macro → existing tools) returning full capture + named entity list + abs_path.  
- [x] Capture path conventions under `captures/eyesight/`.  
- [x] State-aware optional wait (Playing vs menu) documented.  
- [x] Tests: fixture or live dogfood on sample-app / demo-2d.  
- [x] Workflow goal alias: `verify_scene` uses see_scene language.

**Exit criteria:** Single tool/skill path produces a packet an agent always opens.

---

### Phase V2 — Fovea: entity + region crops

**Outcome:** Agent can inspect **one thing** (sprite, mesh, water patch) at craft resolution.

- [x] `bevy_see_entity` — by `Name` or entity id; crop around projected screen bounds or temporary camera zoom.  
- [x] `bevy_see_region` — screen rect and/or world AABB for landscape/water without a single entity.  
- [x] Skill guidance: use E1 for “blocky / muddy / wrong scale” judgments.  
- [x] Dogfood: player sprite crop (2D), ground/prop crop (3D).

**Exit criteria:** Agent can improve a single asset based on a crop, not only full-window guess.

**Implementation notes (choose simplest that works):**

1. Full screenshot + client-side crop using projected AABB (preferred when projection available).  
2. Temporary camera zoom/orbit + full capture (fallback).  
3. Avoid building an in-engine editor overlay as the primary path; tiny name labels optional later (V4).

---

### Phase V3 — Temporal vision + physics eyesight

**Outcome:** Agent can **see motion** and judge physics/feel.

- [x] `bevy_see_motion`: params `frames` or `duration_ms`, optional stimulus (`keys`, `mutate`).  
- [x] Store ordered frame paths or a short strip montage PNG.  
- [x] Physics dogfood recipe: `remote,capture,physics` jump or drop test on a small scene.  
- [x] Failure signatures doc: hover, sink, jitter, explode, teleport.  
- [x] Skill: physics claims require E2 evidence.

**Exit criteria:** Agent can reject “jump feels wrong” with frame evidence and fix damping/impulse/collider, then show after-strip.

---

### Phase V4 — Diff + multi-view packs (landscape / water / 3D)

**Outcome:** Refinement and environment-scale subjects are first-class.

- [x] `bevy_see_diff` — baseline + after paths (optional crude diff image later).  
- [x] `bevy_see_pack` presets:  
  - `entity_craft` — full + crop  
  - `landscape` — game cam + top-down + horizon  
  - `water` — wide + surface crop + short motion strip  
  - `physics_jump` — pre / apex / land or N-frame strip  
  - `lighting` — lit + unlit (when available)  
- [x] 3D secondary camera helpers **only as agent-spawned debug cameras** (code in game under feature flag), not a human editor.  
- [x] Dogfood: landscape + water (or water-like surface) slice when available; else synthetic dogfood scene.

**Exit criteria:** Agent can refine terrain/water readability using a multi-view + motion pack and before/after.

---

### Phase V5 — Style-aware eyesight + art loop integration

**Outcome:** Eyesight feeds asset generation deliberately.

- [x] Packet field `style_intent` + optional reference image path.  
- [x] Skill chain: `see_*` → judgment → `game-asset-core` regen → `see_diff`.  
- [x] Catalog of taste failure modes with example capture notes (blocky, muddy, tiling, toy lighting, illegible HUD).  
- [x] Optional: side-by-side asset thumbnail + in-engine crop (source vs runtime).

**Exit criteria:** “Too blocky → regenerate → looks better” is a closed, evidence-based loop.

---

### Phase V6 — Hardening & multi-instance (later)

**Outcome:** Eyesight works under real agent stress.

- [x] Multi-target: register ports; see_* accept `target`.  
- [x] Black-frame detection heuristics (all-near-black → warn: minimized window / no lights / wrong camera).  
- [x] Cold-compile launch policy remains: shell cargo for cold; MCP launch for warm.  
- [x] Performance: don’t capture 60 FPS streams; default short strips (e.g. 4–12 frames).  
- [x] CI: fixture tests for packet schema; optional live capture job where GPU available.

---

## 11. Recommended capture packs (cheat sheet)

### Entity (character / prop)

| Step | Channel |
|------|---------|
| Full play frame | E0 |
| Tight crop of entity | E1 |
| Idle + move if animated | E2 |
| After scale/art change | E3 |

### Landscape / terrain

| Step | Channel |
|------|---------|
| Game camera composition | E0 |
| Top-down or elevated | E4 |
| Horizon / skyline crop | E1 |
| Optional unlit | E5 |

### Water / large surfaces

| Step | Channel |
|------|---------|
| Wide establishing shot | E0 |
| Surface crop (horizon line + land edge) | E1 |
| 1–2 s motion or N frames | E2 |
| Before/after material/color | E3 |

### Physics motion

| Step | Channel |
|------|---------|
| Resting contact frame | E0/E1 |
| Stimulus (jump/drop/push) | — |
| Frame strip through event | E2 |
| Land / settle frame | E0 |
| After param tweak | E3 |

---

## 12. Failure signatures (agent diagnostic language)

Use these phrases when reading images; map to likely fixes.

| Seen in capture | Likely cause class | Typical acts |
|-----------------|--------------------|--------------|
| Black / empty frame | window, camera, load state, clear color | wait state, lights, camera transform, un-minimize |
| Subject tiny / lost | scale, camera distance, FOV | scale entity, move camera, crop to verify |
| Blocky / Lego | art resolution, filter, upscale | regen art, nearest/linear choice, higher res sheet |
| Muddy brown soup | lighting, albedo, fog, exposure | lights, materials, ambient, clear color |
| Tiling obvious | texture scale, seam, UV | retile, blend, larger unique regions |
| Flat blue water slab | missing motion/normal/edge | shader/FX, foam edge, E2 to verify motion |
| Hover above ground | collider vs mesh mismatch, origin | align collider, mesh pivot, physics margin |
| Sink into floor | penetration, wrong collider | thickness, continuous CD, positions |
| Jitter / vibrate | solver fight, dual control of transform | one authority (physics **or** transform) |
| Teleport frames | tunneling, huge dt, missed spawn | CCD, speed clamp, spawn timing |
| Floaty jump | gravity/damping/impulse | tune and **re-see strip** |
| HUD crushing art | anchors, font size, full-screen panels | layout, opacity, safe areas |

---

## 13. `/goal` usage guide

When the user runs goals against this plan:

| Goal phrasing | Target phase |
|---------------|--------------|
| “eyesight discipline / agents must open captures” | **V0** |
| “see_scene packet / verify_scene eyes” | **V1** |
| “see entity / crop / inspect sprite” | **V2** |
| “see physics / motion strip / jump looks wrong” | **V3** |
| “landscape / water multi-view / before-after” | **V4** |
| “art regen closed loop from captures” | **V5** |
| “harden multi-target / black frame / CI” | **V6** |

**Goal template (paste into `/goal`):**

```text
Execute docs/AGENT_EYESIGHT_PLAN.md Phase V{N} only.
Respect: agent eyesight, not editor; skill↔tool contract; Bevy 0.19 pins.
Dogfood on games/demo-2d and/or demo-3d (and physics feature if V3+).
Update checkboxes in this plan + PROGRESS.md when exit criteria met.
Do not implement later phases in the same goal unless blocked otherwise.
```

---

## 14. Mapping to existing product surface

| Layer | Responsibility |
|-------|----------------|
| **Skills** | HOW to open eyes, judge, and when to recapture (`bevy-agent-loop`, production, 2d/3d, demo, asset skills) |
| **MCP tools** | WHAT evidence to fetch (`see_*`, capture, BRP) |
| **Templates / kits** | WHERE remote+capture (and optional physics) are wired; Name on important entities |
| **Dogfood games** | Proof the eyes work on real short games |
| **Docs** | This plan + PHYSICS + GAME_DOD + ASSET_CONVENTIONS |

Roadmap line (agentic loop) becomes:

```text
Skills → kits → build → EYESIGHT packet (see/judge) → fix → re-see → package
```

---

## 15. Success metrics (long-term)

A phase is not done when code merges; it is done when:

1. **Agent behavior:** aesthetic and physics claims cite capture paths.  
2. **Loop time:** agent can baseline → fix → re-see without human screenshot pasting.  
3. **Subject coverage:** entity, landscape-like ground, and motion each dogfooded once.  
4. **Water readiness:** when a water/surface exists, E0+E1+E2 pack is documented and runnable.  
5. **No editor creep:** no PR that primarily adds human scene-editing UX under this plan’s name.

---

## 16. Open decisions (resolve during phases, don’t block V0)

| Decision | Options | Lean |
|----------|---------|------|
| Crop implementation | client crop vs camera zoom | client crop when projection available |
| Motion storage | N PNGs vs sprite-strip montage vs short video | N PNGs first; montage optional |
| Unlit/diagnostic | Bevy debug modes vs second material pass | only if dogfood needs it (V4/V5) |
| Debug name labels in frame | optional feature for agent readability | optional, default off for “beauty” captures |
| Physics Reflect surface | how much avian state is BRP-visible | prefer visual strips; mutate only stable fields |

---

## 17. Immediate next step

**Start at Phase V0** on the next `/goal`: skill and doc discipline so every visual claim requires opened captures. That unlocks all later engineering without waiting on new tools.

Then V1 (`see_scene`) makes the packet automatic.

---

## Document history

| Date | Change |
|------|--------|
| 2026-07-21 | Initial long-horizon agent eyesight plan (V0–V6), entity/landscape/water/physics subjects, `/goal` templates |
| 2026-07-21 | V0–V6 implemented: see_* library/MCP/CLI, skills, dogfood 2D/3D, findings |
