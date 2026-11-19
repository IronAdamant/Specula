# Design: the asset studio (M4 core)

Status: designed and built 2026-10-22 (M4 lane "asset studio core"). Covers the ROADMAP M4 items **Stage world**,
**Contact sheets, line-ups and before/after**, **Style bible and `style.toml`**, **Taste loop**, **Taste checks as
data** and **Shot sets**, plus the Fidelity track's "rendered in the M4 stage world from fixed cameras". Galleries,
the preference store, reference boards, the review queue and procedural modelling tiers 2-4 build on this and are
other lanes' work.

**Goal:** an agent that makes an asset can see it the way a reviewer would (fixed light, fixed cameras, scale
figures, beside its siblings), and gets back **numbers against the game's rules**, not opinions. Taste stays the
owner's; the studio makes it cheap to apply.

## Why it looks like this

The research ([AGENT_GAMEMAKING.md](../research/AGENT_GAMEMAKING.md) §1) is blunt: frontier models pick the best
and worst image of a set only 26.5% of the time (experts 68.9%), and even experts score inconsistently while they
compare consistently. So the studio follows principle 3, **measure, compare, then ask**:

1. **Measure** what can be measured: budgets, scale, palette distance, saturation, silhouette overlap, value
   contrast, flat lighting, pixel size from the game's camera. A broken rule is an **error**, like a budget error.
2. **Compare**: contact sheets, line-ups beside siblings and before/after, always from the same cameras under the
   same light, so differences are the asset's and not the setup's.
3. **Ask** a human where taste is the question. The studio's sheets are what the review queue will show.

It never asks a model "is this good?". The critique it returns is the bible's rule next to the measured number.

## The stage world

The stage is **isolated**: it renders builds from the model library with its own props and light, like
`anim_filmstrip`, and never reads or writes the game world. Agents call it up for any recipe (or glTF) at any
parameters and seed while a swarm edits the world; nothing needs a claim, nothing is journaled, and the world hash
is untouched.

- **Backdrop:** a ground plane and a uniform backdrop colour. The neutral rig uses 18% grey (`#767676`); the game rig
  uses the game's own ground and sky colours from `style.toml` `[stage]` (Sepulcra: `slab_top` under `sky`).
- **Lighting rigs** (fixed, named; the packet records every value):
  - `neutral`: white key light 40° up from the camera's front-left, white ambient, ACES, shadows. For judging form.
  - `game`: the game's light from `[stage.game]` (Sepulcra: the moon, `#c9d6ff`, 30° up, mist ambient).
  - `worst`: the game rig with the key behind the subject and ambient at the bible's minimum: the light under which
    an asset is hardest to read. Nothing is judged only under flattering light.
  - `silhouette`: black on white, no light, for masks. Every material (glass too) draws as opaque black.
- **Scale references:** a 1.8 m human figure (blocky, neutral grey, readable at thumbnail size) and a 1 m cube, set
  beside the subject on the ground. They are drawn in colour passes and line-ups, never in masks.

*Why a stage and not the world:* the world's light, fog and neighbours change all the time, so two renders of the
same asset an hour apart would differ for reasons that aren't the asset. The stage is reproducible: the same build
under the same rig and shot gives the same pixels on the same GPU.

## Shot sets

A shot is a camera rig plus a viewport plus a light rig. **Every review uses the whole set**, so nothing is judged
from one flattering angle.

| Shot | Camera | Viewport | Light | What it answers |
|---|---|---|---|---|
| `close_up` | three-quarter, 15° up, framed to fit | the tile | `neutral` | form, bevels, detail at full LOD |
| `gameplay` | the game's default camera (Sepulcra: 42° down, 56 m, 45° fov) | 1920×1080, crop at true pixel size | `game` | how big and how readable it is in play |
| `worst_light` | the gameplay camera | 1920×1080, crop at true pixel size | `worst` | does it still read when backlit in the dark |
| `phone` | the gameplay camera | 854×480 (a phone screen at the size it is seen) | `game` | readability on a small screen (F6) |
| `igpu` | the gameplay camera | 1280×720, low shadows, no bloom, LOD by screen size | `game` | the reference device's view (F3) |

The gameplay, phone and iGPU shots render **at true pixel size**: the tile is a crop of the full screen around the
subject, and the sheet magnifies it with nearest-neighbour scaling, so the reviewer sees the pixels a player gets.
The level of detail is chosen from the real screen size, as in the game. The game's camera and viewports come from
`style.toml` `[shots]`; the engine has defaults for a game without them.

## Sheets

All sheets come back like sight captures: a PNG and a **packet** (saved next to it in the captures folder) with one
record per tile: subject, view or shot, pass, tile box, the subject's screen box, its height in pixels, mean
lightness, contrast against its surround and the LOD drawn.

- **Contact sheet** (`studio_sheet`): one block per subject (up to 48), columns are angles (a turntable of N
  azimuths, top, or named shots) and the passes asked for: `colour`, `value` (greyscale lightness of the colour
  render), `silhouette`, and the four form views `clay`, `normals`, `curvature` and `stripes` (shape alone:
  [FORM.md](FORM.md); each a render of its own for every view). Blocks stack one above the other while they fit; a review batch wraps into columns
  of blocks (the fewest that give the largest tile within 2048 px), so one sheet shows a whole batch: the M4
  exit set's 33 props are 4 columns of 176 px tiles. Tiles (96) and renders (128) still bound a sheet.
- **Line-up** (`studio_lineup`): siblings side by side at true relative scale with the human and the cube, front and
  three-quarter, on one image, plus the silhouette distinctness matrix between them. With `form` (clay, normals,
  curvature, stripes) it is the same line-up as shape alone, the references with it ([FORM.md](FORM.md)).
- **Before/after** (`studio_compare`): either two subjects rendered now (two parameter sets, two recipe files), or a
  **mark**: `studio_sheet {mark: "v1"}` keeps that sheet (pixels, per-tile numbers, findings) in memory for the
  calling agent; after editing the recipe, `studio_compare {mark: "v1"}` renders the same sheet again and returns
  before over after, the changed-pixel share per tile and the **deltas of every metric and finding** (fixed, new,
  still failing). Marks are bounded like `sight_compare`'s (8 per session, a byte cap, least recently used first).

## `style.toml`: the measurable rules

The style bible ([STYLE.md](../../games/sepulcra/STYLE.md)) is prose for people. `style.toml`, next to the game's
`style.json`, holds the parts that can be measured. **A rule broken is an error** with the measured value, the
limit, the rule's path in the file and what to change. The palette itself stays in `style.json` (recipes refer to
its names); `style.toml` says how it may be used. The existing `[motion]` table (animation checks) is unchanged and
read by its own parser; the studio reads only its own tables and rejects unknown keys in them.

| Table | Rules |
|---|---|
| `[[budget]]` | per category or name glob, optionally `when` parameters match: `max_triangles` / `min_triangles` (LOD 0), `max_lod_triangles` per later level, `max_texture_kb`, `max_materials`. Later entries override earlier ones key by key. |
| `[[scale]]` | per category or name: `height_m = [lo, hi]`, optional `width_m`, `depth_m`, and `aspect = [lo, hi]` (height over the larger footprint side, for shape language like "upright monolith" or "wide and low"), `float_m = [lo, hi]` (how high its lowest point may be above the ground; default 0..0.05 m). Sizes are extents. |
| `[palette]` | `families` (named groups of palette names; everything else is neutral), `max_delta_e` (Oklab distance from the nearest palette blend), `max_off_palette` (share of surface area allowed beyond it), `neutral_max_chroma` (Oklch chroma), `exempt` (neutral names allowed above it, each with a reason), `forbid` (category → families it must not use: "never green on the player side"), `reserve` (family → the only assets, by name glob, that may use it: "fire is for braziers only"), `emissive` (presets allowed to glow: "emissive only for gameplay meaning"). |
| `[naming]` | `case = "snake"`, `max_len`, `file_matches_name`, `categories` (the allowed list), `parts_named` (every part has a name). |
| `[taste]` | thresholds for the taste checks below; `[taste.allow]` exempts deliberate style per subject glob, with the reason in a comment (like `[motion.allow]`). |
| `[form]` | limits on the form checks ([FORM.md](FORM.md)): `max_loose_pieces` (0 unless given), `max_razor_share`, `max_facet_deg`, `max_seam_share`, `max_smeared_share`, `max_small_relief`; `[form.allow]` exempts as `[taste.allow]` does. The measures are in every asset's facts (`form`) whether or not they are limited. |
| `[stage]`, `[shots]` | the game rig's light and backdrop, the game camera and the viewports. |
| `[bible]` | per category, the bible's rule text, quoted back in critiques. |

**Palette distance, exactly:** every vertex colour (sRGB) is weighed by the area of the triangles around it. Its
distance is the Oklab distance to the nearest point on a **palette blend**: a segment between two palette colours, or
from a palette colour towards black (shading such as `ground_shade`). Gradients, bevel tints and noise tints of
palette colours therefore stay on the palette; a stray colour does not. A blend reads as the family of its nearer
end, and a neutral tinted towards a family colour reads as that family once it is colourful (a wraith's shroud fading
from mist to grave green is neutral at the mist end and the enemy's at the green end).

**Saturation, exactly:** Oklch chroma, with 0.05 as STYLE.md's "~25% saturation". HSV saturation was tried first and
rejected: it calls the near-black night sky (`#0b1020`) 66% saturated. Chroma only falls when a colour is shaded. The
bible's own moss (0.08-0.10) is above the rule; it is listed in `exempt` with that reason, and the question is in the
lane report for the owner.

## Taste checks as data

Each is a measured number against a `[taste]` threshold. Failures are errors in the same list as budget errors.

- **Silhouette distinctness between siblings:** masks at thumbnail size (`thumb_px`, 64) from three angles (front,
  side, three-quarter), each fitted to its own box; the intersection over union of two siblings averaged over the
  angles. Above `silhouette_max_iou` they read as the same thing.
- **Readability at thumbnail size:** the thumbnail silhouette's **solidity**: the share of it that survives a
  one-pixel morphological opening (erode, then dilate), so parts thinner than about three pixels at 64 px vanish and
  lower it (under `thumb_min_solidity` is an error); and the subject's height in pixels in the gameplay and phone
  shots (`min_px_gameplay`, `min_px_phone`).
- **Subject/background value contrast:** how the outline separates from what is around it: for every pixel of the
  silhouette's edge, the lightness just inside against the lightness just outside (within `ring_px`), and the median
  over the outline. In the gameplay shot (`value_contrast_min`) and under the worst light (`value_contrast_worst_min`).
  A mean over the whole subject was tried first and rejected: a backlit stone with a bright rim over a dark body
  averages to the ground's value while it plainly reads (the rune pillar measured 0.017 that way, 0.062 by its edge).
  The mean contrast is still reported (`mean_contrast`).
- **Shape language:** how round, square or pointed the thumbnail silhouettes' outline is
  ([FORM.md](FORM.md), "Shape language"): reported for every subject (`measures.thumbnails.shape`), and checked
  (`shape_language`) only when the style gives `[taste] shape_language`. Beside it, what the recipe is **built
  of** (`measures.thumbnails.built_of`): each shape the recipe names read by its own outlines and weighed by its own
  surface in the model, with the share of the surface that was read (`read_share`: what no record speaks for, an
  outline the measure cannot read and what is past the reading budget all count as not read) and the eight largest
  shapes by path ([FORM.md](FORM.md), "From what the recipe is built of"); a style answers the check by it with
  `shape_by = "recipe"`.
- **Palette distance and neutral saturation:** from the asset's surface colours, as above.
- **Scale against reference figures:** the build's extents (lowest to highest point) against `[[scale]]`, reported
  also as a ratio to the 1.8 m figure. How far the lowest point floats above the ground is checked on its own
  (`float`, default 0..0.05 m; a `[[scale]]` entry's `float_m` band allows a hovering flyer), so lifting something
  can't make it count as taller.
- **Flat-lighting detection:** the spread of lightness inside the silhouette (90th minus 10th percentile) under the
  game rig; under `flat_min_range` the form doesn't turn in the light (no bevels, no gradients, one colour).

## The taste loop

`studio_check` is the loop's one call: for a subject and its siblings (by default the other recipes of its category
in its folder) it renders the **silhouette, value and colour passes at thumbnail size**, the gameplay and worst-light
shots, runs every rule and taste check, and returns:

- the **findings** (errors), each with the measured value, the limit, the rule path and a **revise hint** tied to
  what failed ("silhouette 0.91 like rune_pillar's: change the top; the bible says each type has a distinct top");
- a **critique against the bible** as data: per pass (silhouette, value, colour, scale, budget), its measures, a
  verdict, and the bible's rule text for the category;
- one sheet with the passes (rows: subject then siblings; columns: three silhouettes, value, colour, gameplay,
  worst light), so a reviewer sees what the numbers say.

The agent revises the recipe and calls again (or marks and compares). After a bounded number of rounds it submits
anyway with the critique attached: that is the review queue's input.

## Tools

| Tool | Does | Writes |
|---|---|---|
| `studio_stage` | Renders subjects on the stage from one shot under one rig (or, with no subject, describes the stage: rigs, shots, references). | captures only |
| `studio_sheet` | Contact sheet: subjects × angles or shots × passes; `mark` keeps it for a later compare. | captures, a mark |
| `studio_lineup` | Siblings side by side at true scale with the scale figures, plus the distinctness matrix. | captures only |
| `studio_compare` | Before/after: two subjects now, or a mark against now, with metric and finding deltas. | captures only |
| `studio_check` | The taste loop: passes, shots, rules and taste checks, critique, one sheet. | captures only |
| `style_check` | The `style.toml` rules alone on recipes (no rendering): budgets, scale, palette, naming. Tiered recipes are checked at every value of small integer parameters. | nothing |

All are bounded (subjects, tiles, pixels and response bytes are capped, with errors that say what to reduce),
answer concisely by default (`response_format: "detailed"` gives every tile and measure; the full packet is always
saved), and wait for the builds they need. None of them changes the world, so none is journaled or claim-checked;
marks are per MCP session, in memory (a session's calls run on its own connection thread).

**Studio calls never stall the window (F1, F3).** A call runs on its **caller's connection thread**, as
`lex_simulate` and `balance_*` do. Staging, measures, rules, critiques, compositing and PNG encoding all happen
there, and the call stages its subjects once, so its answer describes exactly the builds it rendered. **Only renders
go to the engine thread:**

- **One render at a time.** A call sends a self-contained render request (builds, bind-pose skin, camera, light,
  size) with its own reply channel and waits for the pixels.
- **Round-robin, within a budget.** The engine queues requests per session and serves sessions in turn between
  frames, within a **render budget**: credit is earned at a share of wall time (25% in the window, about 4 ms of a
  60 Hz frame; 80% headless), and each render spends its measured time. In debt, nothing runs until the debt is paid
  back, so a slow render is followed by frames without studio work.
- **Pixel budget.** A single render can't be split between frames, so frames over 640×640 pixels (stage frames,
  line-ups) render smaller through the same camera and are scaled up on the caller's thread (`render_scale` in the
  packet).
- **Waits and drops.** A render waits while its skinned or textured pipeline variant compiles. A build not on the
  GPU yet (or not down to the level the render draws) is uploaded a level per step, coarsest first, through
  streaming's queued path and within the frame's upload budget; the first step brings its textures. Renders of
  closed sessions, and past their deadline, are dropped.
- **Pinned builds.** A call's builds stay pinned on the GPU while it runs, and for 10 s after. Neither the model
  sync (streaming's included) nor streaming's eviction drops a pinned upload; a textured glTF's texture pixels
  leave the CPU when uploaded, so dropping it would force a re-import. Pins are pruned about once a second, busy
  or not.

**Admission** happens before any work:

- at most 2 calls per agent and 16 in all;
- a call is refused when the renders queued ahead of it, at the engine's share and recent render time, would not
  fit in 60% of the 40 s call limit;
- refusals are `studio_busy` errors with `retry_after_s`.

**Limits:** at most 128 renders per call (each shot and each silhouette is one).

**The studio's own offscreen targets** are kept apart from the sight tools':

- keyed by size and light kind, so a target's render graph isn't rebuilt between lit and silhouette frames;
- at most 16 targets and 96 MB (estimated), each freed after 10 s unused;
- one shadow map shape for every studio render: the world's when its shadows are on (the window and the studio then
  share one map), else 1024 with one cascade. Render graphs share a map per shape and the frame descriptors point at
  one map, so a render with another shape than the last frame's rebinds it, which waits for the GPU to go idle: that
  was the 12-15 ms `record_ms` seen in round 2, not GPU work.

Real GPU memory, these included, counts against `perf.toml`'s `gpu_memory_mb`. Game-shot crops come in eight size
classes (32 to 512 px) and stage frames are rounded up to multiples of 64, so a few targets are reused. A subject
as big as the game camera's view renders the whole viewport, scaled down; pixel heights are reported in the
viewport either way.

The profile zones are `studio_render`, `studio_upload`, `studio_first_frame` (a new target's graph),
`studio_target_create` and `studio_readback`; inside a render, `specula-gpu`'s `gpu.prepare` (with
`gpu.flush_uploads`, `gpu.prepare_frame`, `gpu.pipelines` and `gpu.prepare_graph`) and `vk.wait_fence`. `python scripts/studio_check.py --pacing` captures the window's
frames over the whole of heavy calls (a cold-upload case included, and a window scene with shadows on) and fails
on a frame over 50 ms whose time went to the studio. Each packet's `renders` says what the call's renders cost:
`count`, `max_engine_ms` (engine-thread wall time), and of it `max_record_ms` (preparing and recording the frame),
`max_submit_ms` and `max_fence_wait_ms`, with `max_gpu_ms` (GPU timestamps). With the shadow map shared, a studio
render costs a few milliseconds of CPU (recording, submit, and the readback copy out of mapped memory) and well
under a millisecond to 3 ms of GPU. Wall times far above those come from other processes keeping the CPU or the
GPU busy (preemption), which asynchronous submits would not remove.

**The palette search is bounded:** a palette has at most 64 colours (more is an `invalid_style_json` error,
`palette_too_big`), answers are cached per colour and named set for the call, and the search stops at the call's
deadline (the call answers `timed_out`).

**Reported as well:** taste checks (rendered measures) that pass within 15% of their limit are flagged `marginal` (adapters differ by a few
hundredths in lightness, and a small edit can tip them). Palette checks on a glTF with a base-colour texture are
listed as `not_measured`, since its vertex colours don't say what the texture paints.

## Code

- **`specula-studio`** (new crate, pure, `#![forbid(unsafe_code)]`, depends on `specula-core` and `specula-model`):
  colour maths (sRGB, linear, Oklab, HSV), `style.toml` rules (parse, validate, match), asset facts from a built
  `Model`, rule checks, image measures (masks, IoU, rings, percentiles), taste checks and the critique. Unit tests,
  plus a test that builds every Sepulcra recipe and checks it against Sepulcra's `style.toml` in CI (no GPU).
- **`crates/specula/src/studio.rs`** (engine; calls on the caller's thread, renders on the engine's): the stage (backdrop, rigs, references), shots, rendering tiles through
  the GPU, sheets and packets, marks.
- **`crates/specula/src/studio_tools.rs`**: parsing and schemas; it hands each call to `studio::run` on the
  caller's connection thread.

## Fidelity

The benchmark set (runestone, barrel, lantern, tree, skeleton warrior) has T0 and T1 recipes in
`docs/fidelity/assets/recipes/` (`<subject>_t0.json`, `<subject>_t1.json`), with the set's own small `style.json` and
`style.toml` (neutral daylight, a third-person camera, per-tier budgets). `scripts/studio_check.py --fidelity` checks
them and renders one contact sheet from the stage's fixed cameras (front, three-quarter, top, gameplay) to
`docs/fidelity/benchmark_t0_t1_studio.png`. The skeleton warrior stands in its bind pose: rigging and animation
come with a later tier. Later tiers (`_t2`, ...; procedural modelling tiers 2-4 belong to that lane) add rows.

**Why a raw colour is caught (the draft in the live check):** palette blends across groups only count when the
recipe names both colours. Allowing every pair covered so much of colour space that a raw brown passed as "a fire
tint"; it now measures as the fire family, which `reserve` keeps for braziers.

## Later

- The **review queue** takes `studio_check` sheets and critiques as submissions; galleries and the preference store
  turn picks into `style.toml` rules or priors.
- **Reference boards** measure dropped-in images with the same measures (value histogram, saturation, palette) and
  write targets into `[taste]`.
- Showing the stage in the human's window (today humans see the PNGs).
