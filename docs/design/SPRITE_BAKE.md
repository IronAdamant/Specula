# Design: the 2D baker (`sprite_bake`)

The owner: "All of them by code and can be shown as in both 2D and 3D." A 2D game and a 3D game
take the same creature from the same source: its recipe (or `.glb`) and its clips. `sprite_bake`
turns a model into a sprite sheet the engine's own 2D path plays (EXEMPLARIA.md, section A.6 and
row D of the staged plan).

## What it makes

Two project files, written in **one journal entry** (claims on `/project/<path>`, versions,
`world_undo` takes both back):

- `assets/sprites/<name>.png`: the atlas, RGBA **with alpha**. A strip (one clip from one
  direction) begins a row and wraps onto the next, so its frames are consecutive in the grid.
- `assets/sprites/<name>.json`: the frame data, project file kind `sprite_sheet` (`*.json` in a
  folder named `sprites`; `sprites` is also a folder PNGs are mirrored in). Format
  `specula.sprite_sheet`, version 1: the atlas size, the cell, the columns, the **pivot** (the
  model's origin on its ground, in pixels from each cell's top left), the **metres per pixel**,
  the view and elevation, the light, the source (recipe, params, seed, the model's name and
  content hash); each strip's clip, direction (compass name and facing in degrees), first frame
  and count, fps and each frame's seconds, playback (loop or once), duration, the clip's travel
  per cycle (`distance_m`), its events by frame, and **`sprite`: the 2D path's own sprite
  object** that plays it; each cell's rectangle, clip, direction, frame, time, duration and its
  pivot in the atlas.

A game plays a strip with `world_spawn {shape: "sprite", sprite: <strips[i].sprite>}`: the
`sheet` + `anim {first, count, fps, playback}` reader that already existed (`two_d.rs`) reads
the frames as written, so no new reader was needed. The file's own reader
(`sprite_bake::sheet::SheetData::parse`) is the project's validator of the kind: it refuses a
file whose cells are not where the 2D path's grid puts them, or whose sprite objects do not
play their own strips.

## How

On the caller's connection thread, before the engine renders anything:

1. The arguments, each bounded in the schema.
2. The model, built as an entity would build it (`model_export`'s path: the build is lent to
   the caller's thread).
3. **The plan** (`sprite_bake::plan`): the clips' frames (a loop's frames split its cycle; a
   clip that plays once shows its first and last pose), the directions (1, 4, 8 or 16 round, or
   facings in degrees; 0 faces the viewer, 90 faces right; the model's front is +Z), and **one
   orthographic camera fitted to every posed vertex of every frame from every direction** (the
   vertices are skinned on the CPU exactly as the GPU skins them). So a swung limb is never cut
   off, a pixel is the same size in every cell, and the pivot is the same whole pixel in every
   cell: played back, the sprite's ground holds still. Two pixels are kept clear round the model
   in every cell.
4. **Every refusal**, before any render, with the numbers: an atlas past 4096 pixels a side or
   7 Mi pixels (`atlas_too_large`, with the width, height and bytes it would have been); past 512
   cells; a clip past 64 frames at the rate asked (`clip_too_long`); an unknown clip; clips on a
   model that has none; pixel art without a palette; a path that is not a sprite file.

On the engine thread, a **step job** (docs/design/WORKERS.md): one cell a step, one render with
the ID buffer and its readback, then the cell's resolve. It holds its build pinned on the GPU,
stops at its next step when its session closes or its owner calls `sprite_bake {cancel: true}`
(an agent's cancel stops that agent's bakes only), and answers within the call's deadline.

Back on the caller's thread: the PNG is encoded, checked as the project checks a PNG, the frame
data is read back by its own reader, and both are committed.

## Alpha

Alpha is the **ID buffer's**, never a colour key. Each cell renders at `supersample` samples a
side of each pixel (2 by default, 1 for pixel art, 4 at most, at most 512 render pixels a side).
A pixel's alpha is the share of its samples the model covers in the ID buffer: exactly 0 where
no sample is covered, exactly 255 where all are. Its colour is the mean of the **covered
samples' colours alone**, in linear light, so the background never reaches an edge pixel (no
halo). No post effect that spreads a pixel runs (no FXAA, no bloom, no sky). The colour of the
model's edge is then carried two pixels out into the transparent pixels round it (alpha stays 0
there), so a sprite drawn with linear filtering shows no dark fringe.

Tested: the resolve against hand-made samples (a lone sample keeps its colour exactly, a quarter
covered is alpha 64, black and white average in linear light without the magenta behind them);
on the GPU, for a prop and the shambler at eight directions, every pixel where the ID buffer has
the model differs from the background in the picture and every pixel where it does not is the
background exactly (the render's background is magenta for that reason).

**Pixel art** (`pixel_art: true`): one sample a pixel, alpha all or nothing (half covered or
more), every visible pixel snapped to the nearest colour of the project's palette (`style.json`,
Oklab), played with nearest filtering and pixel snapping.

## Light

`neutral` (the studio's neutral rig) or `project` (the `style.toml` `[stage]` game rig; neutral,
said, when there is none). The key light turns with the camera, so every direction is lit alike
on screen, as a 2D sheet expects.

## The same bytes

The same inputs on the same adapter and driver give the same bytes (a test bakes twice and
compares the atlases). Across adapters this is not promised: GPUs rasterise edges, shade and
round differently, so another adapter's sheet differs by a few levels in shading and at edges.
The plan (the scale, the pivot, the layout, the frame data) is computed on the CPU and is the
same wherever the same build of the engine runs.

## Not done

- Which feet are down in each frame (EXEMPLARIA.md lists it): not computed.
- An outline for pixel art: not drawn.
- A model's world anim assets (clips written with `anim_set`) are not baked: the build's own
  clips are.
- Root motion: clips are taken in place (the engine's clips carry their travel as `distance`,
  given per strip as `distance_m`); a clip whose root wanders sideways is named in the answer
  (`root_drift`).
- Vector 2D (`formae::draw` frame by frame) and a 2D skeleton: stage D2, not designed here.
