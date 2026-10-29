# Design: the terrain renderer and stylised water

Status: designed 2026-10-29 (M4 lane "Terrain renderer (LOD + splat materials), decals, stylised water"; decals were
done in the VFX lane), implemented in the same lane. Code: `crates/specula-gpu/src/terrain.rs` and `water.rs` (the
renderer's data, LOD selection and records), `shaders/terrain*.{vert,frag}`, `shaders/water.frag`,
`shaders/lit_common.glsl`; `crates/specula/src/terrain.rs` (the engine: which entities are terrain, GPU residency,
draws, sight) and `terrain_tools.rs` (`terrain_info`, `terrain_edit`); `crates/specula/src/water.rs` (the `water`
component and its presets). Proof: `scripts/terrain_check.py`. Foundations **F3** (the Intel iGPU is the reference),
**F1** (terrain and water are data agents edit through the journal), **F2** (nothing here touches the simulation
except colliders, which are world data written by tools) and **F4** (the collider is the drawn surface).

## What agents get

- **A terrain is a model entity whose recipe's terrain generator says `"render": "terrain"`.** Everything else about
  it is what procedural modelling already built (`procgen_terrain`, PROCEDURAL_MODELLING.md tier 4): noise layers,
  erosion, pads, up to four splat layers by slope, height and noise. The recipe is a project file, so edits are
  journaled, versioned, claim-checked and undoable (`project_*`), and the entity is an entity (claims, history,
  saves). No new world field: the `model` component already points at recipes.
- **Splat materials as data.** Each splat layer may also name a `texture` (a texture graph in the recipe's
  `textures`, or a library graph), a `uv_scale` (metres per repeat), `roughness`, `specular` and `triplanar`. The
  layer's `color` tints it. A layer without a texture is its colour with a little procedural grain.
- **Tiles.** A terrain bigger than one recipe's 257 samples a side is a grid of tile entities. The generator's
  `offset: [x, z]` (metres) moves the noise domain, so tiles of one recipe with `offset` set from `params` line up;
  `edge_lock` (samples) eases erosion out to the noise-only heights at the tile's border, so neighbouring tiles share
  their edge heights exactly (see *Seams*).
- **Sculpt and paint as data.** The generator's `edits`: an ordered list of strokes (`raise`, `lower`, `flatten`,
  `smooth`, `paint`) applied after erosion and splat, each a disc with a falloff. `terrain_edit` appends strokes to
  the recipe (through the project store: journaled, claim-checked, undoable with `world_undo`) and, by default,
  rewrites the entity's heightfield collider from the edited heights in the same call. Bounded: 512 strokes a
  terrain, 64 a call, radius at most a quarter of the terrain's size.
- **Water is a component.** `water: {preset, size, ...}` on any entity draws a water surface on its local XZ plane
  (at its origin, `size` × its scale), like `decal: {...}` projects a decal. Presets: `lake`, `murk` (Sepulcra's
  graveyard pools: dark, still, moonlit), `frozen` (map 2's frozen lake), `lava` (map 3), `swamp`. Every look
  parameter is data and overrides the preset: `deep`, `shallow`, `foam` colours, `clarity` (metres to full colour),
  `opacity`, `foam_width`, `waves {scale, speed, strength}`, `roughness`, `reflect`, `emissive`, `pattern`
  (`waves`, `ice`, `lava`). World data like a decal: journaled, claimed, saved, undoable, through `world_spawn` /
  `world_set` / `world_batch`.
- **Tools.** `terrain_info {entity}` (samples, cell, height range, LOD levels and patches drawn, residency, GPU
  bytes, layers, and whether the entity's collider is exactly the drawn surface), `terrain_edit {recipe, strokes,
  collider?}`: the strokes and the rewritten collider are **one journal entry** (the collider is built from the
  edited text before anything is committed; claims are checked on both; one `world_undo` takes both back). A bad
  collider target refuses with a structured error (`recipe_committed: false`) and changes nothing. `terrain_edit`
  writes its recipe through the project store although it is in the `terrain` tool family, not `project`: editing
  that recipe is the tool's purpose, and claims on the file, its versions and the journal apply exactly as for
  `project_patch`. `world_*` handles
  water. `perf_frame` reports `terrain_patches`, `terrain_triangles`, `water_draws`
  and the `water` pass's GPU time; the `scene` and `shadow` passes include terrain.
- **Sight.** Terrain and water are entity draws: sight packets list them as subjects with their entity ids and
  `kind: "terrain"` / `"water"`, with `visible_px` and `occluded` from the ID buffer (terrain is opaque there; water
  counts like glass: it covers, it never hides).

## The terrain renderer

### Why CDLOD

Three families fit a heightfield renderer on an iGPU:

| | CDLOD (Strugar 2009) | Geometry clipmaps (Losasso & Hoppe 2004) | Chunked LOD / geomipmapping with skirts |
|---|---|---|---|
| Popping | none: vertices morph to the coarser grid over each level's last 30% | none: transition regions blend | pops at each switch |
| Cracks | none by construction (a node's border is fully morphed where it meets a coarser one) | none (rings, fix-up strips) | hidden by skirts or stitched indices |
| CPU per frame | a quadtree walk with box-sphere tests: a few hundred nodes | toroidal texture updates as the camera moves | pick a level per chunk |
| Data | the heightmap as is (any size, many terrains) | one camera-centred clipmap stack per world: a single big terrain | per-chunk meshes per level |
| Fits Specula | **yes**: many small terrains and tiles, each a texture; culling and shadows reuse the draw plan | built for one huge continuous heightmap; awkward with tiles that are entities | memory per level and popping |

So: **CDLOD**. One shared grid mesh (16×16 quads, plus a skirt ring) is instanced for every selected quadtree node;
the vertex shader reads the node's heights from the terrain's height texture and morphs. Patches are ordinary
`DrawItem`s (`Material::Terrain`): the draw plan culls them, batches every patch of every terrain into one instanced
draw, sorts them after the opaque meshes (early depth rejection under props), and the shadow plan and the ID buffer
take them like any opaque draw. That is why there is no separate terrain pass.

### Data on the GPU

Per terrain build (`specula_gpu::terrain::TerrainTextures::from_heights`), three textures, uploaded through the
queued (streaming) path:

- **Heights**: `Rgba8Unorm`, one texel per sample holding the f32's four bytes; the shader reassembles the bits
  (`texelFetch`, no filtering): the drawn heights are **bit-for-bit the collider's**. Shader-side bilinear only
  between samples while a vertex morphs.
- **Normals**: `Rgba8Unorm` with mips, central differences of the heights: smooth per-pixel lighting even on
  coarse patches far away.
- **Splat**: procgen's RGBA8 weights with mips, filtered.

Per frame, a **terrain material record** (256 bytes, `FrameDesc::terrains`) per build and set drawn (shared by every
entity of that build: 300 tiles of one recipe use one record; at most 256 a frame, decided before an entity's stand-in
mesh is hidden, so a terrain past them keeps its mesh and never vanishes): its grid, the three
texture slots, and four layers (tint, roughness, specular, uv scale, triplanar sharpness, albedo and normal texture
slots). Patch records (one `DrawRecord` per patch) hold the terrain's world matrix, the node (first sample, size,
level) and its morph range; the batch's fifth push-constant field (the bones address for skinned meshes) points at
the material records.

### Selection, morphing and seams

The engine calls `terrain::select` for every terrain in view (CPU, pure, unit-tested):

- Leaf nodes are 16 cells; a level-`k` node covers `16·2^k` cells with a vertex every `2^k` cells. A per-terrain
  **min/max tree** (built once per build, 2 floats a node) gives every node's height range, so node boxes are
  tight on flat ground and tall on cliffs.
- LOD ranges: `range[k] = r0 · 2^k` with `r0` = a leaf node's diagonal (16 cells, at the terrain's full
  cell and largest scale): for 1 m cells, 22.6 m. That is the ring around the camera drawn at the data's full
  resolution. Tiles of one terrain share the cell and scale, so they share the ranges.
- Selection (Strugar): a node is drawn at level `k` where its box meets the sphere `range[k]` but not
  `range[k-1]`; otherwise its children are selected, and any child whose box misses `range[k-1]` is drawn as a
  **quarter patch** (8×8 quads, a second shared grid) at the parent's level. Selection never culls by the frustum
  (the draw plan does, per view), so the camera's patches are exactly the shadow casters and the ID buffer's
  patches: one selection, three views, no mismatch. At most 4,096 patches a terrain (past it nodes stop
  subdividing: coarser, never missing).
- **Morph, over every level:** a vertex collapses onto each coarser level's grid in turn, by `m_k = clamp((d_k -
  start_k) / (range[k] - start_k))` with `start_k` at 70% of the band (85% above level 0), for every level `k` from
  its patch's own, where `d_k` is the camera's distance to the vertex *as it stands on level `k`'s grid* (a vertex
  the coarser patch has too). Odd coordinates slide to the even neighbour below (an odd-odd vertex to the cell
  corner), so at `m_k = 1` the patch is level `k+1`'s grid, cells split v10-v01 like the physics heightfield at
  every level. A vertex's position is therefore a function of its sample and the camera alone, whichever patch draws
  it.
- **No cracks, between any two levels:** the coarser side of every border lies outside the finer side's range (box
  tests), so the finer side's extra border vertices are fully collapsed there, onto the coarser side's vertices,
  and from then on both sides run the same steps from the same vertex. The first version morphed one level only
  (classic CDLOD), and the seam test found 4 m cracks where a tall cliff node reached into the next level's morph
  band; the multi-level morph closed them. `specula_gpu::terrain::tests` checks every shared border over 72 cameras,
  three transforms (rotated, scaled) and a deliberately spiky terrain: the largest gap is under 0.1 mm.
  `patch_vertex` is the CPU mirror of the shader's vertex (keep them in step).
- **Tiles** of one terrain (equal cell size, power-of-two sizes such as 128 cells) select with the same world-space
  ranges, so they behave as subtrees of one quadtree. Their border heights are bit-equal when the noise domain lines
  up (`offset`, computed in f64 from whole sample indices; tested) and erosion is eased out at the border
  (`edge_lock`). Normals at a tile's border use the neighbouring tile's samples just past it (the engine finds
  unrotated tiles of one shape side by side on a grid; a set is made again when a neighbour arrives), so lighting
  is seamless too. Splat weights at the shared border row follow a one-sided slope, so a 1-sample splat seam can
  show where a layer's slope band is crossed exactly there. A **skirt** (the grid's ring, dropped by
  `skirt` metres, default 0.5) hangs from the terrain's outer border, as insurance against neighbours that don't share
  heights (different recipes side by side, rounding between two tiles' transforms); it is below the surface, so it
  never shows where the surfaces match.
- **What the collider sees:** within `range[0]` the drawn triangles are the heightfield's, bit for bit (heights
  read as their f32 bits). Farther out the surface is coarser, as every LOD scheme's is. The engine's heightfield
  colliders hold at most 129 samples a side, so tiles of 129 samples match their colliders exactly; a 257-sample
  terrain's collider is resampled (`terrain_info` reports `same_grid` and `max_error_m`).

### Shading

`shaders/terrain.frag` (sharing `lit_common.glsl` with water: the scene's sun, shadow, point lights, fog, SSAO alpha
and display code, copied from `mesh_frag.glsl` so the mesh pipelines stay bit for bit; fold them together after
the GI lane merges):

- Splat weights (filtered), renormalised; layers under 1/256 skip their samples (most pixels sample one or two
  layers). Layer textures are sampled with explicit gradients (`textureGrad`), since the layer loop branches.
- **Triplanar on steep slopes:** layer textures project from above (`uv = world xz / uv_scale`, world space so
  tiles line up) until the slope passes the layer's `triplanar` threshold (default 35°), then blend in the X and Z
  projections over the next 0.15 in cosine by the normal's sharpened weights; projections under 2% are skipped.
  Flat ground costs one sample a layer.
- Normal: the terrain's normal map (smooth lighting on coarse patches too), bent by the layers' normal textures from
  above (UDN) when they have them.
- Untextured layers get a little world-space grain (`grain`, 0..1). Near the camera, where a pixel covers a few
  centimetres, a finer octave fades in and breaks the splat's bilinear blends into patches (magnified 1 m blends
  read as blur).
- **Decals** need nothing: they reconstruct the surface from depth after the opaque pass, so they land on terrain.
- **GI seam:** `lit_ambient(p, n)` in `shaders/lit_common.glsl` is the ambient light at a surface (terrain and water
  both read it through `gather_light`; water's sky reflection goes through `water_reflection(p, dir)` in
  `water.frag`). They return the frame's ambient and sky today; the baked-GI lane replaces their bodies with probe
  lookups.

### Residency and streaming

`crates/specula/src/terrain.rs` keeps each terrain build's GPU textures (heights, normals, splat), nearest first.

- **Finding terrains costs nothing per entity.** The terrain entities are an index made again only when the GPU
  models' sync key moves (a model entity came, went or changed, a build landed, the world was replaced); water is a
  world index (`World::water_entities`, kept like the effects index). A frame visits those and nothing else, and takes
  the library's lock only to remake the index (10,000 plain cubes: no entity visited).
- **Texels are made off the engine thread, for captures too.** A texel worker makes a set's heights, normals (with
  the neighbours' borders) and mipped splat; an upload on the engine thread is then a copy. The window's uploads stay
  within streaming's per-frame budget (`upload_ms`, `upload_mb`, shared with model and 2D uploads through
  `Streamer::allowance` / `charge`). A capture whose terrain sets (or the terrain pipelines) aren't up parks, like one
  waiting for a model build (`CAPTURE_BUILD_WAIT`, 10 s, and never past the call's deadline), until the worker has
  made them; past the wait it renders with those terrains' stand-in meshes and says so in its warnings.
- **Per build, from its nearest entity.** Residency (what to make, what to free) is decided once per build from
  its nearest entity, before any material record is handed out; nothing is freed after that in the frame, so every
  record's set is resident (two entities of one build, one near and one 1 km away, used to free the near one's set
  and take the engine down). A build drawn at one grid place reads its neighbours' borders; one drawn at several
  places reads none (each place's neighbours would ask for a different full set, re-made every frame).
- **Only the window evicts, and not what captures just used.** Distance frees sets only in the window's frames, and
  not a build a capture used in the last 30 s (`CAPTURE_GRACE_S`): a capture never takes what the window draws, and
  repeated captures from elsewhere don't wait for their sets again each call. Idle sets (no entity for 5 s) go
  either way.
- **Memory:** resident sets count against streaming's `memory_mb` with the models and proxies (`terrain_mb` in
  `stream_state`). When a set the frame wants doesn't fit, the least recently wanted sets not wanted this frame are
  freed first (`evicted_for_memory`); only if that isn't enough is the set left out (`blocked_bytes`; its terrain
  keeps its coarse copy or its stand-in mesh). Worker texels nothing uploads within 120 frames are dropped
  (`texels_dropped`, `texels_waiting_bytes`).

What is kept where:

- **coarse** (every 8th sample: 17² texels for a 129-sample tile) within `hlod_radius_m` of the camera: far tiles stay
  drawn after their meshes stream out;
- **full** within `radius_m`; terrains of at most 65² samples, or whose size doesn't divide by 8, have no coarse
  copy and keep the full set out to `hlod_radius_m` (past it, their stand-in mesh draws);
- freed 25% past `radius_m` (full) and 10% past `hlod_radius_m` (coarse), and a build no entity drew for 5 s
  (replaced by an edit, deleted) is freed whole.

A coarse terrain draws as a terrain with an 8× cell and level offset 3, so its level 0 is the full terrain's level
3 and its morph ranges are the full terrain's. Until a terrain's textures and the terrain pipelines (built in the
background like the skinned and textured variants) are ready, its entity draws its recipe's mesh as before (the
recipe's terrain mesh defaults to every 4th sample when it renders as terrain), so nothing pops out. Terrain entities
are never merged into HLOD proxies (they have their own LOD). Entity ids past the HLOD cover cap (transient
entities) keep drawing their mesh.

## Stylised water

### Where it draws

Water draws only once its pipeline (the terrain variant, built in the background) exists, and until then the ID
buffer doesn't count it either: a sight packet never reports water that isn't on screen.

Water needs the opaque scene's depth (shore foam, depth colour, the floor's distance through the water), so it can't
be drawn in the scene pass. Like decals, it gets a pass of its own after the opaque part of the scene:
`scene` (opaque, sky) → `decals` → **`water`** (reads depth, blends into colour, no depth attachment: the shader
tests depth itself) → `scene_blend` → `particles`. Frames without water declare no pass and are unchanged bit for
bit. Water entities are `DrawItem`s with `Material::Water` on a quad, so the plan culls them, the shadow plan leaves
them out (blended), and the ID buffer counts them like glass.

**Limits (v1):** blended meshes and particles drawn after the water draw over it even when they are below its
surface (glass behind a lake shows in front of it); water doesn't write depth, so fog and SSAO see the floor under
it; the camera under the surface sees a plain tinted ceiling. Volumes (an aquarium box) and planar reflections are
later work.

### Shading

All a function of the frame's time (`FrameDesc::time_s`), so captures are deterministic, like particles:

- **Waves:** four octaves of value noise's gradient, each rotated (no axis lines), drifting its own way (the surface
  evolves instead of sliding) and about twice the last's frequency by an irrational step: nothing repeats, so no grid
  or parallel bands at any distance (sinusoid trains, even five at golden-angle spacing or a faint swell, read as
  bands at grazing angles). Summed as normals (no vertex displacement: the quad stays flat, so the ID buffer and depth
  test agree with the image); octaves under about 8 pixels fade out. `waves.scale` (metres), `speed` (m/s) and `strength`.
- **Depth:** the floor point under each pixel from the depth buffer; `thickness` along the view ray and `depth`
  straight down. Colour `mix(shallow, deep, 1 - exp(-depth / clarity))`; opacity `opacity · (1 - exp(-thickness /
  clarity))`, premultiplied over the scene behind: **refraction without distortion, for free** (the scene's own
  colour shows through). Distorted refraction would need a copy of the scene colour (about 0.3 ms at 1080p on the
  iGPU): not by default, left for a quality preset.
- **Foam** where `depth < foam_width`: sparse clumps (about 1.5 m) where a drifting two-octave noise beats the depth
  (never an outline: at most 65% cover), none on the first few centimetres of the waterline and none where the
  foam band would be under a few pixels wide (a steep bank, where it could only be a line). It fades out between 15 and 40 m from the camera: past the terrain's fine
  rings the floor under the water is coarse triangles, and depth foam would trace them (straight runs, plates over
  low ground).
- **Waterline and shallows.** The water fades in over about two pixels of the floor depth's change (its screen
  derivative, worked out before the shader's depth-test discard; not across a jump in the floor, such as a crest's
  silhouette, where widening it opened the surface in 2 x 2 blocks), at least 5 cm: an anti-aliased line, not a band.
  It starts 12 cm above the surface, at every distance: floor barely out of the water (a crest grazing it) is faded
  over rather than cutting the surface into a dotted line, and nothing standing in the water (any mesh: the shader
  can't tell terrain from a post) loses more than its lowest 12 cm. (A distance-scaled rise, up to 1.5 m, could hide
  a thin post at the waterline far away.)
  Shallow water takes up to 45% of its own colour within its first 50 cm, so a bright floor just under the surface
  doesn't shine through clear water. (A fixed 60 cm fade left holes in the reflection over every submerged crest,
  pale lines at a distance; with no shallow colour, steep banks showed the bright floor as a pale line.)
- **Reflection:** Schlick Fresnel (F0 0.02). With the sky drawn, the water mirrors the sky's own gradient (zenith to
  horizon, as the sky pass draws it, never brighter than the ambient term) plus the sun's or moon's glint; without a
  sky, the ambient term (`env_radiance`). (Mirroring the ambient fill at night turned dark water pale lavender,
  brighter than the night sky.) Tinted 60% by the water's own colours, and the body leans (80%) to the shallow colour at
  grazing angles, so a preset still reads at the horizon; the sun's GGX highlight through its shadow. Cheap enough
  for the iGPU (no planar re-render). A GI probe can replace the environment term (`water_reflection`).
- **Patterns:** `ice` has body: frosted and nearly opaque where thick (0.92), thinning to clear at the shore; the
  Worley crack network is part of that body (paler, never more opaque than the ice) and fades where it is thin.
  `lava` is opaque and emissive: a slow-flowing crust (dark, cooled) over glowing veins, a band of solid cooled crust
  towards the shore (at any distance), and an edge that fades over its first 30 cm of depth instead of a hard rim
  along the terrain's triangles.

### Presets (STYLE.md)

| Preset | deep | shallow | foam | clarity | waves | notes |
|---|---|---|---|---|---|---|
| `lake` | `#13303f` | `#2f6a73` | `#d8e4e8` | 3 m | 3 m, 0.6 m/s, strength 0.4 | a daytime lake |
| `murk` | `#0b1119` | `#1d2633` | `#5d6878` | 1.2 m | 6 m, 0.15 m/s, faint | map 1: still and dark, the moon in it (reflect 0.65: never brighter than the night sky); neutral (< 25% saturation) |
| `frozen` | `#3c5566` | `#a9c2cf` | `#eef4f8` | 6 m | still (`ice`) | map 2: frosted shore, cool neutrals |
| `lava` | `#2a0d06` | `#ff7a1f` | `#3a2a24` | — | 8 m, 0.1 m/s (`lava`) | map 3: fire colours, emissive 2.5 |
| `swamp` | `#1c1309` | `#4a3618` | `#6b7034` | 0.6 m | 5 m, 0.1 m/s | peat-brown, tea-coloured shallows, a weed-green scum at the shore; dull |

## Cost on the iGPU (measured 2026-10-29, Intel RaptorLake-S iGPU, other sessions sharing it)

`cargo test --release -p specula-gpu --test terrain terrain_cost -- --ignored --nocapture`: a screen-filling view at
1600x900 (HDR, shadows, bloom, FXAA; median of 40 frames). The `scene` pass:

| Case | iGPU | RTX 4090 Laptop |
|---|---|---|
| a lit ground quad (the baseline) | 1.04 ms (0.59 without shadows) | 0.27 ms |
| terrain, untextured layers, no grain | 2.84 ms (1.95 without shadows) | 0.39 ms |
| terrain with grain | 3.45 ms | 0.39 ms |
| terrain, two layers textured (triplanar on slopes) | 4.13 ms | 0.46 ms |
| terrain and a lake | 3.78 ms + the `water` pass 2.05 ms | 0.35 + 0.23 ms |

Of the terrain's fragment cost (bisected by cutting the shader short): rasterising the patches 0.47 ms, the normal
and splat textures 0.4 ms, the layer loop 0.75 ms, lighting the rest; hills put far more of the screen into shadow
penumbrae (the 12-tap filter) than a flat quad. Fixes made on the way, each measured: one octave of grain per pixel on
a float hash instead of two per layer on the integer hash (1.7 ms down to 0.6), an unrolled layer loop (a
dynamically indexed array spilled), layer fields read one by one instead of whole 256-byte records, no anisotropic
filtering on the data maps, `r0` one leaf diagonal instead of 2.5 (0.4 ms), the water's ripple as two cosine trains
instead of a noise gradient and its foam noise only near the shore (2.9 ms down to 2.0). The remaining gap to a lit
mesh is most likely register pressure (the Intel compiler drops to SIMD8 for big shaders); specialising the terrain
shader per feature (textured layers, triplanar, layer normals) is the next step if a game needs it.

Live (`scripts/terrain_check.py --adapter integrated`, 1600x900 captures of the 1 km² terrain of 64 tiles with a
lake): 42-79 patches and 19-44 k triangles drawn; GPU totals 8.8 ms (overview) to 12.9 ms (low over the slopes), of
which `scene` 3.5-6.8 ms, `water` 0.9-2.2 ms, `shadow` 0.1 ms and the capture's own `id_buffer` 2-2.9 ms (sight only).
All 64 tiles drawn as terrain 14.6 s after spawning (builds on two workers); a sculpt stroke's rebuild of all 64 tiles
2.9 s; 17.5 MB of terrain textures resident.

In the window (`terrain_check.py --window`, 1600x900, 240 frames each, the iGPU shared with two other sessions'
Sepulcra engines and five compilers): GPU p95 10.6 ms over the whole terrain, 15.1 ms low over the slopes, 18.3 ms
over the lake shore (`scene` 8-10 ms, `water` 1.4-1.9 ms); the same views captured offscreen alone took 9-13 ms.

**Sepulcra with a pool** (`m3_exit.py --adapter integrated --water murk`: an agent spawns a 70 m `murk` pool round
the slab's foot through `world_spawn`; 1280x720, 600 frames, Discord, Teams, Edge and Steam open, one other engine
and five compilers running): busy wave GPU p95 11.5 ms (8.99 ms without the pool in the run just before), boss wave
9.6 ms (8.57); the `water` pass 1.58 ms mean. Both runs won at wave 10 with the replay verified and Lex parity: the
water is cosmetic. The budget is 14 ms. Frames without water or terrain declare neither pass: Sepulcra's own frames
are unchanged.

## Known limits (polish)

The owner ruled these polish, not correctness, on 2026-10-30. Found by the sight-skeptic's last round (check16, Intel iGPU, 1600x900) and left for Sepulcra's map 2 (the frozen
wasteland, part of the v1.0 maps work), when the frozen and swamp presets are refined:

- **Shallow-shelf hairlines.** Thin pale lines on clear water (`lake`) along shallow shelves and submerged crests at
  mid and far distance (100 m and more): the bright floor shows through thin, clear water. Each round reduced them
  (shallow water takes 45% of its colour within 50 cm; the waterline fade no longer opens across crest silhouettes);
  they remain faint. Fix direction: tint the floor seen through water by thickness (a colour multiply needs the
  scene colour as a texture: the distorted-refraction copy below), or raise shallow opacity with distance.
- **Ice crack tiling.** `frozen` reads as a frosted body, but its crack network is one Worley scale of even width: an
  even polygon tiling over a flat body. Fix direction: jitter the cells (a domain warp) and add a second, finer
  crack scale with varying width; vary the frost with low-frequency noise.
- **Swamp horizon.** Near the camera `swamp` is peat-brown (mean sRGB 53, 47, 40); toward the horizon distance fog
  greys it (134, 141, 138) while `murk` (129, 153, 169) and `lake` (134, 172, 183) stay blue from the sky they
  mirror. Fix direction: carry the preset's hue through the fog (fog the water toward a mix of the fog colour and
  the preset's shallow colour).

## Not done (yet)

- Heightmap assets (16-bit PNG) as a terrain source (procgen generators only today).
- Distorted refraction, planar reflections, underwater views, water volumes.
- Virtual texturing of layer textures (four layers a terrain, from texture graphs).
- Shore foam fades out with distance instead of following the heights (beyond about 80 m the shoreline is the
  coarse triangles' shallow colour): foam from the terrain's height texture instead of the depth buffer would follow
  the true shore at every distance.
- Splat seams between tiles of *different* shapes or rotated tiles (only unrotated tiles of one shape on a grid
  share their borders' normals and weights).
- Specialising the terrain shader per feature for the iGPU (see *Cost*), and a cheaper water lighting path.
