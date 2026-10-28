# Design: particles, effects and decals (VFX)

Status: designed 2026-10-22 (M4 lane "Particle/VFX system; decals"), implemented in the same lane. Code:
`crates/specula-scintilla` (effects as data, the particle evaluator, decal specs, built-in textures, checks),
`crates/specula-gpu/src/effects.rs` and `shaders/particle.*`, `shaders/decal.*` (drawing), `crates/specula/src/vfx.rs`
and `vfx_tools.rs` (the engine: components, the effect library, frames, sight, tools). Proof: `scripts/vfx_check.py`.
Foundations **F3** (the Intel iGPU is the reference), **F2** (determinism: effects never touch the simulation) and
**F1** (agents author effects as data, with the same journal, claims and checks as everything else).

**Scintilla** (Latin: a spark) is the crate's name.

## What agents get

- **Effects are data.** An effect is a JSON object: up to 8 emitters (rate and bursts, lifetime, spawn shape,
  velocity, gravity, drag, curl-like noise, size, colour and alpha over life, rotation and spin, flipbooks, additive or
  alpha blending, soft edges, optional collision) and an optional light. Every field is validated with structured
  errors (`{error, field, value, allowed, message}`) that say what to change.
- **A library.** Built-in effects (fire, embers, motes, trail, dust puff, impact, smoke, sparks) plus the world's own:
  `vfx_set` writes an effect as a world entity under `/vfx/` (tag `vfx_effect`, `props.vfx_effect`; only `/vfx`'s
  children are effects, so a claim on `/vfx` covers the library), so it is journaled, claim-aware, undoable and saved
  with the world like every other edit. `vfx_get` reads one (or lists them). The library is **reserved data of the
  write policy** ([WRITE_POLICY.md](WRITE_POLICY.md)): `vfx_set` commits under its own `Rule::Vfx`, and every other
  checked writer (agents' world tools, what agents ask the game, a module or a live Lex run to do, modules' requests)
  is refused with `reserved` if it writes in `/vfx` (making, changing, moving in or out, deleting) or gives an entity
  the `vfx_effect` tag anywhere. Claims on `/vfx` hold for `vfx_set` too. An effect entity a loaded world holds outside
  `/vfx` isn't an effect (`vfx_info`'s library problems say so) and may be deleted by anyone.
- **A component.** `vfx: {effect, params}` on any entity plays the effect there. `params` are a fixed set of
  overrides every effect understands (rate, size, speed, tint, alpha, intensity, light on/off, seed). `decal: {...}`
  projects a texture onto the opaque surfaces inside the entity's box. Both go through `world_spawn` / `world_set` /
  `world_batch`, so claims, versions, the journal and undo apply unchanged.
- **Tools.** `vfx_preview` renders an effect over time as a filmstrip with facts per frame (live particles, estimated
  overdraw, world bounds, screen box); `vfx_check` runs budget and readability checks as data; `vfx_info` reports the
  library, live instances, budgets and the last frame's counts. Answers are concise by default.
  A preview is queued with the asset studio's renders and runs a step at a time (one frame evaluated, one tile
  rendered, then the effect's checks) within the studio's render budget, taking turns with studio renders; the
  finished sheet is PNG-encoded, written and base64-encoded on a thread of its own. A step starts while there is
  credit and is charged afterwards, so one step (about 1-6 ms on the Intel iGPU) may overrun a frame's share, never
  the whole sheet. At most 4 are queued or encoding, 2 per agent (`studio_busy` with `retry_after_s` past that). A
  playtest's preview outlives its starter's session, as the playtest does (its deadline still bounds it). (M4 audit:
  rendered in one go, 16 tiles of 320 px took Sepulcra's iGPU frame p95 from 8.5 to 43.5 ms; the review of the fix:
  encoding the sheet in one step still cost a 36 ms frame.)
- **Sight.** Capture packets list the effects and decals in view (entity, effect, live count, bounds, screen box).
  Particles and decals **never write the ID buffer** (see below), so `visible_px` and `occluded` stay facts about
  geometry. The `effects` overlay draws their boxes.

## Cosmetic, yet reproducible

Effects are presentation, outside the simulation (DETERMINISM.md: rendering reads simulation state, never writes it).
Nothing the particle system computes is written back to the world: `world_hash` is identical with effects on or off
(`--vfx off`), which `vfx_check.py` proves on a scripted world and on Sepulcra. The *components* (`vfx`, `decal`) are
world data like `light`: hashed when present, saved, journaled. A game's own presentation effects go on its transient
entities (never hashed), so turning a game's effects on or off doesn't change its hash either.

They are still **reproducible**: the same world at the same time renders the same particles, so captures and sight
packets repeat bit for bit (`vfx_check.py` compares two fresh engines' captures). That comes from one decision:

### Particles are a pure function of time

A particle's state is computed from scratch every frame, in closed form, from:

- the effect (data), the emitter's **seed** (from the entity id, the component's start time and `params.seed`) and the
  particle's **index** `i`;
- the time `t` (world time: the clock agents and captures already use, which follows `time_control`);
- for world-space emitters, the emitter's **track**: where its entity was at each tick (see below).

Particle `i` of a rate emitter is born at `start + (i + jitter_i) / rate`; a burst's particles at the burst's time.
Its lifetime, spawn offset, velocity, size and spin come from a counter-based hash of `(seed, i)`. Its position is the
ballistic solution with linear drag:

```
v(a) = v0·e^(−k·a) + (g/k)·(1 − e^(−k·a))
p(a) = p0 + (v0 − g/k)·(1 − e^(−k·a))/k + (g/k)·a        (a = age; k → 0 gives p0 + v0·a + ½·g·a²)
```

plus a noise displacement (below) and optional collision. Size, colour, alpha and the flipbook frame are curves of
`a / lifetime`. The live particles at `t` are exactly the indices born in `(t − max lifetime, t]`, so the work is the
live count, never the history.

**Why not a stateful simulation.** Stepping particles per frame makes the image depend on frame timing (a 144 Hz
window and a capture see different smoke); stepping per tick at 30 Hz looks steppy and still needs sub-steps for
captures between ticks. The closed form gives:

- captures that repeat at any `time_s`, with no warm-up and no history to replay (`vfx_preview` jumps to any time);
- zero cost for emitters out of view (nothing to step while culled; they are exactly right when they come back);
- pause, slow motion and `time_control` for free (time stops, particles freeze);
- nothing to save, nothing to journal, nothing to desync;
- a **GPU seam**: the same function can run in a compute or vertex shader later (the state is the inputs).

**What it costs.** Forces must be integrable: gravity and linear drag are exact; noise is a displacement field, not an
integrated force; collision clamps instead of bouncing. Bounces, attraction to moving targets and particle-particle
effects need a stateful simulation (the later GPU simulation; the effect format leaves room for it).

### Curl-like noise

Our own deterministic hash noise (`specula_scintilla::noise`): 3D value noise on an integer lattice hashed with a
fixed 32-bit mix, quintic interpolation with analytic derivatives. Three such fields form a vector potential; its
curl is divergence-free, so smoke swirls without bunching. The displacement at age `a` is
`strength · a · curl(p_ballistic(a) · frequency, t · speed)`: sampled where the particle would be, it bends smoothly
along the path. Only `+ − × ÷` and `dmath::floor` are used, so it is identical on every platform.

### Tracks for world-space emitters

A world-space emitter (a wraith's trail) needs where its entity *was* when each live particle was born. The engine
records each emitter entity's world position once per **tick** (a ring of up to 128 samples, enough for the longest
lifetime at 30 Hz; longer lifetimes clamp to the oldest sample) and the current position as the live head. A tick is
deterministic, so headless runs and paused captures repeat exactly. In the window, a hosted game moves its transient
entities per frame (interpolated), so the window's trails follow what was shown, which is what a viewer expects; only
captures that need to repeat must be taken on a stepped or paused clock, as for animation. Local-space emitters (a
brazier's fire, a rune's motes) need no track.

### Seeds

`seed = hash(entity id, bits of the component's start time, params.seed)`; particle `i` hashes `(seed, i, field)`.
Hashes, not a stateful RNG: a particle's numbers don't depend on how many came before it, or which were culled.

## Drawing (F3)

### Particles: one draw call for all of them

The engine evaluates every visible emitter into one reused array of particles, sorted **globally far to near** (one
`sort_unstable` of `(depth bits, index)` keys; keys are unique so the order is deterministic). The GPU draws them with
**one instanced draw** of camera-facing quads: every particle names its material (texture, flipbook grid, softness,
lit) in a small table, textures are bindless, so different effects still batch. Blending is premultiplied alpha, where
**additive is alpha 0**: `src + dst·(1 − a)` with `a = 0` adds. So alpha smoke and additive fire share one pipeline,
one draw and one correct back-to-front order (per particle `additive` 0..1 even lets fire turn into smoke over its
life).

The pass runs **after the scene** (`particles`): it reads the scene's depth as a texture and does its own depth test
in the fragment shader, which also gives **soft particles** (a fade over `soft` metres where a particle meets
geometry) without a second depth read. Reading the depth as a texture needs no depth attachment in the pass, so the
graph only adds the depth → shader-read barrier (free of stores on the Intel iGPU, which isn't a tile GPU; on a tile
GPU the depth is stored once, as for SSAO). The cost is that there is no early-Z: a particle behind a wall still runs
its fragment shader (one depth fetch, then discard). Fill is bounded by the budgets below instead.

- **No vertex buffers:** the vertex shader reads the particle record (48 bytes) and its material through buffer
  device addresses, builds the quad (camera-facing, rotated, or stretched along a world vector for sparks), and passes
  everything as flat varyings (the sprite path's rule: reading records per fragment costs integrated GPUs dearly).
- **Lighting per particle, not per pixel:** lit particles (smoke, dust) get ambient, sun and point lights at their
  centre in the vertex shader; fog likewise.
- **HDR:** colours are linear and may exceed 1 (fire blooms); the pass writes the same values the scene does
  (exposed HDR, or tone mapped when the frame has no post chain).
- **Half resolution:** not built. The fill-rate cap keeps overdraw bounded on the iGPU; a half-resolution particle
  target (drawn at ¼ the pixels, upsampled with a depth-aware composite) is the next step if a game is fill-bound
  (measure with `perf_frame` `gpu_passes.particles` first).

### Budgets, LOD and culling (no per-frame allocations)

`perf.toml` `[vfx]` (overridable per adapter kind, like `[streaming]`):

| Key | Default | Meaning |
|---|---|---|
| `max_particles` | 6000 (integrated 4000) | particles drawn per frame, all effects together |
| `max_work` | 160000 (integrated 100000) | CPU work evaluating them per frame, in work units (below) |
| `max_emitters` | 512 | emitters evaluated per frame (nearest first) |
| `max_decals` | 256 | decals drawn per frame |
| `fill_cap` | 3.0 (integrated 1.5) | particle pixels per frame, in screens (estimated, then checked on what was evaluated) |
| `decal_fill_cap` | 3.0 (integrated 1.0) | screen area decal boxes may cover per frame, in screens, `lit` ones counting double (nearest first; about 2.7 ms per lit screen on the iGPU at 720p) |
| `max_per_effect` | 1500 | what `vfx_check` allows one effect instance at its peak |
| `lights` | 4 | point lights effects may add (the frame's 16 are shared with the world's) |

Per frame, deterministically:

1. **Cull** emitters whose bounds (track box grown by the effect's analytic reach) are outside the view or smaller
   than `cull_px` pixels on screen.
2. **LOD**: past an effect's `lod.start` metres, a growing fraction of its particles is skipped (down to `lod.min`
   at `lod.end`); the survivors grow slightly so the effect keeps its weight.
3. **Budget**: each emitter's expected live count is known analytically. If the total exceeds `max_particles`, emitters
   keep a fraction, highest `priority` and nearest first. A particle is kept when `hash(seed, i) < fraction`, so
   thinning is stable (the same particles stay) and needs no state. The same pass holds the CPU to `max_work`: a
   particle's cost is known from its emitter (`eval::work_per_particle`), so dear effects thin first.
4. **Fill cap**: each emitter's screen area (live × the mean over life of size² × alpha, so a spiking size curve counts
   by its square) is estimated; past `fill_cap` screens, the biggest contributors are thinned first. After evaluation
   the cap is checked again on the particles actually produced (a burst whose size spikes together escapes any mean):
   past it, every effect keeps the same stable share (`fill_thinned`).

**Work units.** About 10-20 ns each on the reference laptop's CPU (`eval_cost_by_kind`, release): an index looked at
(1), a position (2), the curl noise on top of a position (15), finishing a drawn particle (8). A plain particle costs
11, one with noise 26, with `stick` (9 positions to find the touch) 29, with both 179. Measured: 16 instances of a
noise + stick effect (300 a second, 5 s lives) took 8-8.6 ms a frame before this budget; with it, 1-2 ms on that shared machine
(`vfx_check.py` part 3b, which also checks the work stays within twice the budget).

`vfx_info` and `perf_frame` report particles drawn, thinned and culled, emitters culled, the fill estimate and the
pass time. Every buffer is reused frame to frame (`frame_alloc` tests hold the line).

**Bounded work.** The evaluation follows what is drawn, not what would live: a thinned emitter visits only every
`⌊1/keep⌋`-th particle index (keeping each with probability `keep × stride`, still a stable subset), an emitter
with nothing kept isn't visited, an emitter stops once the output is full (a dropped particle is never positioned), a
frame stops evaluating at twice `max_work` (reported as a problem; checked inside an emitter, not only between
instances), `vfx_preview` frames are thinned and capped like window frames (each frame reports its work), a repeating
burst's cycles × count are
thinned as one index (so a fast burst of one is thinned like a rate), a repeating burst may fire at most 120 times a
second, and an effect whose analytic peak exceeds 50,000 particles is refused when parsed (a safety limit far above any
budget; `vfx_check`'s `max_per_effect` is the budget).

**Textures.** Built-in textures are made on first use. PNGs under the assets folder are decoded on a loader thread,
keyed by the file's canonical path (case-folded on Windows: `FX/a.png` and `fx/A.png` share one texture) and their
colour space (decal normal maps load linear, colour textures sRGB), reloaded when the file changes
(window frames look once a second; a capture looks first and waits, and `vfx_preview` loads what it needs on the spot,
so the same call gives the same images) and retried when a missing or broken file is fixed. A texture a world effect
or decal names stays loaded, in view or not (no reload hitch when the camera turns back); one nothing names (a
preview's, a removed effect's) is freed after about ten scans unused. At most 64 distinct asset textures and 256 MiB
(with mips) are held; past that, more are refused as data (`texture_budget` in the frame's problems).

Particles out of view are culled twice: emitters whose conservative bounds (the track box grown by the analytic reach,
inherited velocity and its stretch included) miss the view aren't evaluated, and an evaluated instance whose
particles' quads (half diagonal plus half the stretch) all miss the view isn't drawn (reported as `offscreen`).

### Decals: projected boxes after the opaque geometry

A decal is a box (the entity's transform × `size`); it paints its texture onto whatever opaque surface lies inside
the box, projected along the box's −Y axis (straight down for a decal lying on the ground). It respects reversed Z:
positions are reconstructed from the depth buffer with the inverse view-projection, whatever the depth convention.

**Where:** between the opaque part of the scene and the rest. When a frame has decals, the scene pass is split:
`scene` (opaque meshes and the sky) → `decals` (reads the depth, blends into the colour) → `scene_blend` (2D items and
blended meshes, depth test against the same depth) → `particles`. Decals land on stone and the slab, never on glass or
particles drawn after them. Frames without decals keep the single scene pass, bit for bit.

**How:** each decal draws its box's back faces (so it works with the camera inside the box), and the fragment shader
reconstructs the world position under the pixel, moves it into the box, discards it outside, and samples the decal's
textures at the box's XZ. The surface normal comes from the depth (nearer neighbours, as SSAO does), which also fades
decals on walls the box crosses (`angle_fade`). Two modes:

- `lit`: albedo (and optional tangent-space normal map, roughness, specular, emissive) **re-lit** in the decal shader
  with the frame's ambient, sun (with its shadow map) and point lights, blended over the surface by its alpha. Runes,
  chalk, moss.
- `multiply`: the surface's colour times the texture (no lighting: multiplying lit colour by an albedo ratio is exact
  for darkening). Scorch marks, soot, wet patches. Cheapest.

All decals are one or two instanced draws (consecutive decals of one mode merge; bindless textures), sorted by
`order`, then entity id. `max_decals` nearest win; decals smaller than 2 pixels are skipped.

**Why not forward decals.** Applying decals in the mesh shader (clustered forward decals) means a decal loop in every
opaque pixel's shader, a specialization of every mesh pipeline, and a per-tile decal list on the CPU. On the iGPU the
deferred-style pass costs only the pixels the decal boxes cover (a scorch mark is a few thousand pixels), plus one
depth → shader-read barrier the frame pays once. Its limits: the re-lit surface's own normal map and material aren't
known (the decal's own are used), and decals land on anything opaque inside the box, creatures standing in it
included; keep boxes thin (Sepulcra's are 0.5-0.6 m tall) and fade them with `angle_fade`. Receiver masks (stencil bits
per entity) are the later fix.

### The ID buffer

Neither particles nor decals write entity ids, and they never hide anything in the ID buffer: smoke in front of a
stone doesn't make sight report the stone `occluded`. That keeps sight's per-subject facts about geometry, which is
what agents check (is the tower there, is it hidden by the cliff). What the effects themselves cover is reported
separately: each entry in a packet's `effects` has its screen box and estimated covered pixels, so an agent can tell
"the stone is visible but behind fire" from the two lists.

### Lights from effects

An effect may carry a point light (`light: {color, intensity, range, offset, flicker}`), placed at the emitter,
flickering with deterministic noise of time. Effects add at most `[vfx] lights` lights (nearest to the camera first)
after the world's own, within the frame's 16.

### Photosensitivity

Player safety (WCAG 2.3.1: no more than three flashes in any one second, general and red). Two layers: per-effect
checks are the early warning agents see first; the **flash watch** on the final image covers what no effect can know.

**Per effect** (`specula_scintilla::flash`, refused when parsed with the `photosensitive` code, wherever the effect
comes from: `vfx_set`, props, a loaded world). At a close reference view (3 m, 60°, 720 lines) three signals are summed
over everything one instance plays at once, then sampled 120 times a second in windows around every onset (each
emitter's start, each burst's first and last cycle) and in the steady state, so late strobes are seen:

- **brightening:** area × alpha × luminance (capped at white), over a dark background;
- **darkening:** area × alpha × (1 − luminance) of the alpha-blended share, over a bright background;
- **red:** the area × alpha of saturated red (`R / (R + G + B) ≥ 0.8`, red value `(R − G − B) × 320` of 20 or more).

Area is the quad (`size² × E[random²] × params.size²`) times the texture's coverage (frame by frame for built-in
flipbooks); colour is the gradient × tint per channel × emissive × intensity, linear. Every burst cycle at its time,
every rate emitter's births at regular `i / rate` times (the worst phase), and the light (its range's area, at most
the view, × 15% reflected × its flicker level) go into the same timelines; random lifetimes are mixed (half the
shortest, half the longest). A transition is a change of half the significant area at full brightness (a quarter of
the 10° central field) that is also a tenth of the larger side; a flash is a pair; more than three in any second fails.
Each particle's own life (curves, built-in flipbook frames) and a flipbook's frame sequence at its frame rate are
checked too. Bounded: under a millisecond for any legal effect (`flash_cost`, a work-count test).

**Per instance, in the write policy** (`crate::vfx::write_rules`, run by every checked writer after the reserved-data
rules: agents' commits under every `Rule`, what agents ask the hosted game, a module or a live Lex run to do, and
modules' own requests): an instance made or changed must name an effect there is (`unknown_effect`) and, played with
its params, stay within three flashes a second (`photosensitive`); an effect `vfx_set` adds or changes must stay so
for every instance already playing it with params (`photosensitive`, naming them). One write asks for at most 64
distinct effect-and-params checks (`too_many_effect_params`). Whatever the policy exempts (a loaded world, an undo,
the hosted game's own calls, ticks) is caught as instances are evaluated: such an instance isn't drawn, and the frame
reports it (`flashing`, and a `photosensitive` problem), as data. So nothing that flashes is drawn, whichever path
wrote it, and writers that can be told are told when they write. The regression table
(`write_policy/tests.rs`) has a row for each path.

**Not covered per effect** (the flash watch covers them): several instances together, an entity's scale, a camera
closer than 3 m, flipbooks over asset PNGs (`flash_unchecked`, a warning), sizes and lifetimes beyond the mixes above,
what is behind an effect (backgrounds are taken as dark for brightening and bright for darkening), and everything
that isn't an effect (lights the world sets, the interface, game code).

**The flash watch** (`crates/specula/src/flashwatch.rs`, `shaders/flash_*.frag`) reads the final image whole,
interface included:

- **The window:** after the overlay, a half-resolution pass (`flash`) moves each texel's state on from the previous
  frame's. A texel stands for a 2x2 block of pixels, read with one bilinear tap at its shared corner (the exact mean
  of its linear colours; a partial block at an odd edge clamps onto its own pixels). It keeps its luminance and red
  value at their last extremes, the direction they were moving, this frame's means and whether it made a transition,
  in one of the window's two RGBA16F images (about 1.8 MB each at 720p; frames alternate between them, so nothing is
  copied). A second pass (`flash_cells`) reads only that state: each 8x8 block's mean (every pixel weighted once,
  partial blocks counting only their own pixels) and the share of its pixels whose texel made a transition. The
  blocks (8 bytes each, `flash_copy`) are read back a frame or two late and summed into 32-pixel cells, each block
  weighted by its pixels, so every pixel counts exactly once at any window size (a unit test checks the weights at
  1280x720, 1920x1080, 1366x768 and odd sizes). Means are kept to 1/255 (the thresholds are 0.1 and 20/320). The
  window needs SAMPLED, linearly filterable swapchain images; without them, or past 4096 x 2304,
  `flash.window_unsupported` says why.
- **Captures:** the same state on the CPU from their own pixels (every second pixel across and down), at simulation
  time, kept per view (up to 8, least recently used out first), so the same captures give the same report.

A texel *transitions* when it moves back from an extreme: relative luminance by 0.1 with the darker side under 0.8
(general), or the red value `(R − G − B) × 320` by more than 20 while saturated red. Each cell is watched two ways: its
mean luminance and red value (a uniform strobe), and its **pattern**: a frame where at least a quarter of its pixels
transitioned counts one (a checkerboard, stripes or dots inverting, a red/green checker: their means don't move). A
flash is a pair of transitions; a cell fails with more than three in the last second; the frame fails when failing
cells cover 2.8% of the view (a quarter of 341 × 256 on the 1024 × 768 screen WCAG assumes, scaled to the viewport).
Reported in `perf_frame` (`flash`, and an `errors` entry), `vfx_info` and sight packets (`flash`, once a view has a
history), with an event recorded when a failure starts; a minimised window's report goes `stale` and clears. **Warn
only for now;** a *protect* mode that dims what flashes in the failing cells for shipped games is reserved
(`Mode::Protect`) and is an M6 shipping item.

**The watch's limits, honestly:**

- The area is summed over the whole view, not any one 10° field: stricter than WCAG (two small strobes at opposite
  corners can fail together), so expect false positives there, never the reverse.
- Patterns finer than 2 pixels average out in the half-resolution state; captures read every second pixel. The red
  value is taken from each 2x2 block's mean colour, so red and another colour mixed inside one block count as less
  red.
- Red changes are counted as changes of the red value within saturated red; a change of hue that never reaches
  saturated red isn't a red flash.
- State changes that agents or game code make every tick (a `world_set` loop toggling a light or a material) are seen
  only by the watch, on the final image; no per-effect check can know them.

## Effect format

```json
{
  "duration": null,            // seconds of emission; null = forever (loops)
  "emitters": [{
    "name": "flames",
    "rate": 40,                // particles per second (0 with bursts only)
    "bursts": [{"time": 0, "count": 30, "cycles": 1, "interval": 0}],
    "lifetime": [0.5, 0.9],    // seconds, min..max
    "shape": {"kind": "cone", "radius": 0.15, "angle_deg": 15},   // point, sphere, box, disc, cone, line
    "speed": [0.6, 1.2], "direction": [0, 1, 0], "spread_deg": 10,
    "gravity": [0, 0.5, 0], "drag": 1.5,
    "noise": {"strength": 0.4, "frequency": 1.2, "speed": 0.8},
    "size": {"keys": [[0, 0.25], [0.3, 0.35], [1, 0.05]]},          // metres over life
    "size_random": [0.8, 1.2],
    "color": {"keys": [[0, "#ffd27a"], [0.4, "#ff7a1f"], [1, "#c2410c"]]},
    "alpha": {"keys": [[0, 0], [0.1, 1], [1, 0]]},
    "emissive": 3.0,           // HDR multiplier (blooms above 1 after exposure)
    "rotation_deg": [0, 360], "spin_deg": [-90, 90],
    "texture": "flame",        // built-in name, or a PNG under the assets folder
    "flipbook": {"cols": 4, "rows": 4, "fps": 16},  // or "over_life": true
    "blend": "additive",       // additive | alpha; "additive": 0..1 mixes
    "soft": 0.3,               // metres of depth fade (0 = hard)
    "lit": false,
    "stretch": 0,              // sparks: quad stretched along velocity × this (seconds)
    "space": "local",          // local | world (world: particles stay where they were born)
    "inherit_velocity": 0,
    "collide": {"plane": 0, "mode": "kill"},  // or {"ground": true, ...}: the world's ground grid
    "lod": {"start": 25, "end": 70, "min": 0.25}, "priority": 1, "cull_px": 1.5
  }],
  "light": {"color": "#ff7a1f", "intensity": 2, "range": 4.5, "offset": [0, 0.6, 0], "flicker": 0.25}
}
```

Limits (each a structured error): 8 emitters, 16 keys per curve, rates up to 2000/s, lifetimes up to 10 s, bursts up
to 1000 particles, analytic peak per instance ≤ `[vfx] max_per_effect` (a `vfx_check` failure, not a parse error, so
a budget change never makes a saved world unreadable).

## Measured (2026-10-22, Intel RaptorLake-S iGPU, the reference device)

- **Offscreen, 1280x720 HDR frame** (shadows, SSAO, bloom, FXAA; 400 boxes, 600 particles in two materials, 12
  decals; `cargo test --release -p specula-gpu --test effects effects_cost -- --ignored --nocapture`): the
  `particles` pass 0.10-0.14 ms, `decals` 0.14-0.16 ms; on an RTX 4090 Laptop 0.05 ms each. The scene split adds
  nothing measurable to `scene` on the iGPU (its time swings more between runs than with or without decals); on the
  discrete GPU it costs about 0.03 ms.
- **Sepulcra in the window, wave 5 with 12+ enemies** (30-80 particles, 7-10 decals): `particles` 0.02-0.07 ms,
  `decals` 0.04-0.14 ms; the effects' CPU time (evaluation, budget, sort; `vfx_info` `cpu_ms`) 0.1-1.3 ms on that
  shared machine with 9 instances, 150-220 particles. Frame pacing with `--vfx on` and `off` overlaps
  run to run on a shared machine; the order of the runs moved it more than the effects did.
- **Flash watch** (Sepulcra mid-wave, 1280x720 window): on the iGPU `flash` (the half-resolution state) 0.13-0.14 ms,
  `flash_cells` 0.05 ms and `flash_copy` 0.005 ms, about 0.2 ms in all (the first exact version read the final image
  twice and copied its state: 0.83 ms); on the RTX 4090 Laptop 0.02, 0.01-0.02 and 0.01-0.03 ms. The window's
  detector takes 0.15-0.3 ms of CPU a frame; a capture's 2.5-3 ms at 1280x720 (every second pixel), 0.13 ms at
  320x180. A calm Sepulcra wave never trips it (at most 2 flashes in any cell, no pattern flashes); a two-instance
  strobe, an 8-pixel checkerboard inverting and grey bars in the rows the old halving blits missed are all reported
  from the window (`vfx_check.py` part 3c).
- **Determinism:** `scripts/vfx_check.py` (both adapters) shows `world_hash` identical with `--vfx on` and `off`,
  captures byte-identical across fresh engines, and Sepulcra's hashes equal on and off over 40 checkpoints.

## Seams for later

- **Flash protect mode (M6, shipping):** the flash watch knows the failing cells; protect mode would dim the
  contributions there (particles and effect lights whose screen boxes overlap them, then the whole image's change if
  that isn't enough) instead of only warning. The seam: `flashwatch::Mode::Protect` and the report's `region`.

- **GPU simulation:** the evaluator's inputs (effect, seed, track, time) are a few hundred bytes per emitter; a
  compute shader can evaluate the same closed form, and a stateful GPU simulation (bounces, attractors) can replace it
  per emitter behind the same `ParticleMaterial` drawing path.
- **Half-resolution particles** (above), **ribbons** (trails as strips instead of particles), **decal receiver
  masks**, decals on the 2D path, and texture atlases for effect textures.
