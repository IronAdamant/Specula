# Design: baked global illumination (irradiance probe volumes)

Status: designed and implemented 2026-10-29 (M4 lane "Baked global illumination"). Code: `crates/specula-lux` (the bake:
probe lattices, L1 spherical harmonics, a BVH ray caster, visibility, relocation, the file format), `crates/specula-gpu`
(`gi.rs`, `shaders/gi.glsl`: the probe buffer and the sampling in the lit, decal and particle shaders, terrain and
water through `lit_common.glsl`),
`crates/specula/src/gi.rs` and `gi_tools.rs` (the engine: scene gathering, bake jobs, the store, residency, staleness,
tools). Proof: `scripts/gi_check.py` (Sepulcra, iGPU). Foundations **F3** (the Intel iGPU is the reference), **F2**
(the same scene bakes to the same bytes) and **F1** (agents bake, check and read GI as data).

**Lux** (Latin: light) is the crate's name.

## The choice: irradiance probe volumes, baked on the CPU

ROADMAP M4 asks for "lightmaps or probe volumes, baked by agents ... the low-end-friendly answer to Lumen". Probe volumes
win for Specula on every axis that matters here:

| | Lightmaps | Irradiance probe volumes (chosen) |
|---|---|---|
| UVs | need a second, non-overlapping UV set per mesh (UV2 unwrap, padding, texel density) | none |
| Procedural and scatter content (`specula-model`, `procgen_scatter`) | every generated mesh needs an unwrap; instanced scatter can't share a lightmap | works as is: probes light whatever is drawn |
| Dynamic objects (Sepulcra's creatures, projectiles, ragdolls) | not lit by the bake (a second system is needed anyway) | lit by the same probes as the static world |
| Memory | texels per surface area, grows with detail | probes per volume: 48 bytes each on the host, 64 on the GPU (with tier low's lean copy), ~0.8 MB per 32 m cell at 1 m spacing |
| Bake cost | per texel, many more samples | per probe: thousands to tens of thousands of probes |
| Streaming | per mesh or per atlas | per cell: probe chunks follow `streaming.rs`'s 32 m cells |
| Detail | sharp contact shading | smooth; small-scale occlusion comes from SSAO (already in the engine) and the direct shadow map |

Lightmaps would only win for sharp baked shadows of static lights; Specula already draws the moon's and the sun's shadows
directly (cascaded shadow maps) and point lights directly, so the bake only has to carry **indirect** light, which is
smooth by nature. That is exactly what probes are good at.

**CPU, not GPU.** The bake runs on worker threads with IEEE `+ - * / sqrt` and `specula_core::dmath`, so the same scene
and settings give **the same bytes on every machine and adapter** (`specula-lux` is a determinism-linted crate). A GPU
bake would be faster but differ between vendors and drivers, and Specula has no compute or ray-tracing path to build it
on. Sepulcra's top face bakes in seconds on two threads (see Measurements).

## What a probe holds

Each probe stores, in 48 bytes (12 × u32, IEEE halves; the GPU reads the same bytes the CPU wrote):

- **Irradiance as L1 spherical harmonics, pre-convolved:** `E(n) = c0 + c1 · n` per colour channel (12 halves). `c0`
  is the mean radiance the probe sees; `c1` is the cosine-lobe L1 term (the standard `2π/3` band factor folded in, and
  the `1/π` of the diffuse BRDF, matching the renderer's convention that a surface's diffuse light is `albedo × E`).
  A uniform sky of radiance `L` gives `E = L`, so in the open the GI matches the old flat ambient. The bake clamps
  `|c1| ≤ c0` per channel, so `E` is never negative (no ringing).
- **An offset and a weight** (4 halves): probes the bake finds inside geometry are moved out (up to 0.6 spacings per
  axis, "relocation"); those still inside afterwards get weight 0 and are never used.
- **Its cell's visibility mask** (64 bits): the probe is the lowest corner of one lattice cell, and the bake records,
  for each of that cell's eight octants (half-cells), which of the cell's eight corner probes the octant's centre can
  see (the segment from the centre to the probe, where it was moved to, is clear: nothing crosses it and it doesn't
  run inside a solid). A wall through the cell hides the probes on its far side from the octants on its near side.
  An octant whose centre hides every valid corner (it sits inside a solid: a noise-displaced mesh's crease, a wall)
  looks again from a 3 x 3 x 3 grid of points in it. Its open points are grouped into pockets (joined through grid
  neighbours they see clearly, or through a corner probe they both see); a single lone point that sees nothing is
  ignored. One pocket: the octant takes the corners any of its points sees. Two pockets (the two sides of a wall or a
  roof, which share no probe) keep nothing, so walls still hold. Before this, closed noise-displaced SDF meshes baked
  at 0.25 to 0.5 m left whole cells black beside them (the T3 fidelity lane: the runestone's footing, the barrel's
  base and lid rim, the tree's root flares; `specula-lux` `points_beside_a_noisy_closed_mesh_are_never_black`).
  `BAKE_VERSION` 5 rebakes stored files.
- **Its reaches** (14 nibbles, 7 bytes): how far the probe sees before a surface along the six axes and the eight cube
  diagonals, in eighths of the spacing (15: nothing within 15/8). A point further than that along the direction
  nearest to it (and along the second nearest when it is nearly as near) is behind a wall from the probe.

**Why masks, not distances.** The design pass started with six free distances per probe (a cheap cousin of DDGI's
octahedral depth maps, read as an ambient cube). The bake's own tests found the flaw: a probe relocated just above a
roof sees open sky along ±X, so a point under the roof diagonally from it looked visible and the hut leaked (0.04 of the
outside light). Six directions can't resolve that, and DDGI's fix (16×16 octahedral depth per probe, 1 KB) costs 20
times the memory and a texture path. The masks are exact at the octant's centre and cost 8 bytes per probe and one extra
16-byte load per pixel (the cell's). **They are not enough alone** (the independent review, 2026-10-29): a wall
through an octant puts points of both sides in it, and its centre (in the wall, or on one side) speaks for one side
only; with the hut shifted against the lattice, strips of the closed room read 54 to 126% of the outside. Its probes
found three more holes, each fixed and pinned by a test: segments slipping through the shared edges of two box faces
(Moller-Trumbore isn't watertight: edges are now widened by 1e-4 barycentric and node boxes padded); segments
running *inside* a solid between two of its faces (an octant centre on a wall's inner edge, a probe on its outer
edge: no face between them, so a midpoint inside test now hides them); and a far probe left alone at a sliver of
trilinear weight, normalized to full brightness. The **reaches** add a per-probe test that holds for any point, not
just octant centres (the old six distances leaked diagonally; fourteen directions, judged by the nearest two, don't
in the sweeps), and a point whose visible probes carry under a quarter of its valid trilinear weight is dimmed in
proportion (`VISIBLE_SHARE`: a point beside a wall on the open side always keeps at least that share, because the
wall sits at least the normal bias behind it). `closed_huts_anywhere_on_the_lattice_stay_dark` bakes the closed hut
moved 0 to 0.95 m in 5 cm steps against 1 m and 0.5 m lattices: no inside point (floor, ceiling and walls, from 5 cm
off the corners) reads above 8% of the open ground.

The ambient cube was considered instead of L1 SH: 18 values instead of 12 for the same smooth signal; L1 is the
standard (Unity APV, DDGI's irradiance) and evaluates in three multiply-adds.

## Sampling (the lit shaders)

`shaders/gi.glsl`, `gi_ambient(p, n, v)`, replaces the flat ambient term in `gather_light` (mesh; terrain and water
through `lit_common.glsl`'s `lit_ambient`, which also dims water's mirrored sky and terrain's glossy reflections by the
same specular occlusion as meshes), the decal and
the lit particles (per particle, in the vertex stage, with no normal: the mean `c0`), and is specialized away when the
frame has no GI (`FEAT_GI`, so frames without it run exactly the old code):

1. **Bias.** The sample point moves off the surface, `p' = p + n · 0.25 s + v · 0.05 s` (`s` = the spacing), so a
   surface is lit by the probes on its own side.
2. **Volume.** The first resident volume (a streamed chunk) whose box holds `p'`; outside every volume the flat
   ambient is used, faded in over one spacing at a volume's edge, so a volume never shows a hard border.
3. **Eight probes**, trilinear weights, each times:
   - a **backface** weight `((dot(dir, n) + 1) / 2)² + 0.2` (DDGI's smooth backface), `dir` from the point to the probe;
   - **visibility**: 0 if the cell's mask hides it from the octant `p'` falls in, or its reaches put `p'` behind a
     surface;
   - its validity (0 for probes left inside geometry).
   Weights are normalized; a floor of 1e-4 of the trilinear weight keeps visible probes in play (none at a trilinear
   weight of 0). Every valid probe hidden (a pocket no probe sees into: a crevice, ground under a barrel's bulge, a
   closed room's corner) gets a twenty-fourth of their mean irradiance (`HIDDEN_FLOOR`): dim, never black (the T3
   lane saw whole black cells), and under the closed-hut test's 8%, so walls hold; every probe invalid (inside rock)
   falls back to the flat ambient. Visible probes carrying under a quarter of the valid trilinear weight dim the
   result in proportion.
4. **Result:** `Σ w (c0 + c1 · n) / Σ w`, times the volume's `intensity`, clamped non-negative. Glossy reflections of
   the surroundings are scaled by the ratio of that irradiance's luminance to the flat ambient's (a cheap specular
   occlusion: rough metal inside the crypt no longer mirrors the bright night sky).

**Tiers** (`gi_settings quality`, the machine's choice, not world data; every tier keeps the cells' visibility
masks):

| Tier | Meshes | Terrain and water | Pipelines | Default on |
|---|---|---|---|---|
| `high` | every lit pixel samples its eight probes (masks, reaches, backface weights) | the same | the full sampler (`FEAT_GI_PIXEL`) | discrete GPUs |
| `low` | draws up to two spacings wide: an ambient cube the CPU samples once (three loads per pixel); wider draws: the lean probes per pixel | the lean probes per pixel | the lean sampler only | integrated GPUs |
| `off` | the flat ambient | the flat ambient | the old ones | |

**Tier low.** The full sampler's registers cost the iGPU's fragment shaders their occupancy on every draw a pipeline
shades, whether or not a pixel runs it (about 0.8 ms of the bench's scene pass), so tier low has its own pipelines
without it:

- **Small draws** (bounding radius up to two spacings: props, walls, creatures) get an **ambient cube**: the CPU
  samples the irradiance each face of the draw's box sees, half a spacing out from the face's centre, facing out
  (the twin of the shader's weights, masks and reaches, no normal). The shader weighs the six by the normal squared.
  A wall's inner face reads the room and its outer face the yard. (Its first form, one SH at the draw's centre, sat
  inside a wall and blended both sides: the round-4 review's sealed hut was lit by the sky at low.) A face whose point
  is inside a solid takes the valid probes there unmasked (every probe hidden), or the flat ambient (none valid).
- **Wide draws** (ground slabs, the slab's rock, a hollow crypt) and **terrain and water** read the probes per pixel
  through their **lean copy**: one 16-byte record a probe (the mean irradiance and the direction's luminance as
  halves, the validity), appended to the GI buffer (64 bytes a resident probe in all). The eight probes around, with
  the cell's mask for the pixel's octant and the volume-edge fade, but no reaches and no backface weights. (Its first
  form, a grid of SH the CPU sampled over each wide draw's box, blended samples across walls, so the sky crept onto a
  sealed hut's floor, and a moving wide draw re-sampled its grid every frame.)
- The records are cached across frames by mesh, model matrix and bounds (cleared when the GI set, the flat ambient or
  the tier changes). A draw that is exactly what that draw index was last frame takes last frame's record without a
  lookup, so a static scene samples and looks up nothing. Entries unused for two frames are swept, and the oldest go
  first past 4 MB; nothing is ever flushed wholesale (the first cache emptied itself past 8,192 entries, and moving
  draws, a new key every frame, filled it).

A middle tier (the full sampler for wide draws only) was built and dropped: it paid the full sampler's registers on
every draw, so on the iGPU it cost as much as `high`. No preset picks `high` on an integrated GPU; a lead who sets it
there gets a `gi_tier_over_budget` warning with the measured cost. Each tier's pipelines (and terrain's and water's
with them) are built only when a frame at that tier asks.

SSAO keeps working: the ambient part it may darken is now the GI term. The CPU has the same sampler
(`specula_lux::Field::sample`), used by the bake's own bounces, `gi_check`, and the tests that pin the shader's
behaviour (`crates/specula-gpu/tests/gi.rs` renders it).

**Leaks** (Sepulcra's crypt and fence): a closed hut with 0.2 m walls at 1 m spacing stays dark inside (below 8% of
the outside on its floor, every inner wall and its ceiling: `crates/specula-lux` `a_thin_wall_does_not_leak`, which
also shows that without the masks the same walls leak). A fence's bars block the few rays that hit them, which is
right for a fence. Probes inside walls are relocated or dropped, so the slab's solid rock never contributes.

## The bake (`specula-lux`)

Input, gathered by the engine on its own thread (bounded, O(entities in the box)), then handed to workers:

- **Geometry:** every shown, persistent (non-transient), non-`ui` 3D entity whose box touches the volume plus a margin
  (default 8 m): cubes, quads, spheres, lathes and models (each model's LOD 1 when it has one: fewer triangles, the
  same light), unless it moves (an `anim`, a kinematic or dynamic body) or is under an `exclude` path. Albedo is the
  vertex colour times the entity's tint (textured parts: their material colour; textures aren't sampled), in linear
  light through `dmath`. Emissive is `albedo × emissive` as the shader draws it, except on entities that carry a point
  light: there the light stands for the glow (a brazier's bowl and its fire light would count twice, and the bowl is
  too small for the probes' rays to find evenly: the first yard bake showed orange blotches all over the ground).
  Glass, blob shadows and lava lamps neither block nor emit (documented limitation: emissive glass doesn't glow into
  the bake). Spheres and lathes are tessellated with `dmath` (the renderer's meshes use the platform's `sin`).
- **Lights:** the sun (direction, colour × intensity) with ray-traced shadows, every point light in range (the
  shader's falloff, shadowed), and the **sky**: rays that escape see the world's ambient colour × intensity
  (`sky_below` scales the lower hemisphere, default 1). Direct sun and point light are drawn by the renderer itself;
  the probes carry only what bounces and the sky.
- **Fireflies:** each ray's radiance is clamped to a luminance of 16 times the sky's (at least 0.25). Indirect light
  seldom needs more, and a small bright surface hit by one ray in 256 can't make a blotch.

Steps (each bounded, each deterministic, each charged to the job's work count):

1. **Lattice.** Probes at `(n + ½) · s` on a world-aligned grid (any spacing 0.25..8 m): floors and walls at whole
   metres never pass through a probe, and two volumes' probes coincide where they overlap. At most 65,536 probes and
   1,048,576 triangles per volume.
2. **BVH** over the triangles: binned SAH, built in a fixed order (stable partitions).
3. **Validity and relocation:** 64 rays per probe; more than a quarter hitting back faces means inside geometry: move
   past the nearest back face (at most 0.6 s per axis) and test again, up to three times.
4. **Visibility:** each cell's mask, 64 short segments per cell (each with a midpoint inside test when clear), and
   each probe's 14 reaches.
5. **Bounces.** `rays` (default 256, at most 1,024) directions per probe on a spherical Fibonacci set, rotated per
   probe by a rotation drawn from its index. Pass 0 traces them once and caches each hit (20 bytes: distance, normal,
   albedo, direct light plus emission as halves); each later pass lights the cached hits with the previous pass's
   probes (the same sampler as the shader) and projects radiance into SH. `bounces` (default 2, at most 4) passes
   after the first.
6. **Denoise:** each probe averaged with its valid axis neighbours it can see (the segment between them clear), its
   own value counted twice: Monte Carlo noise made probe-sized blotches on lit walls (the sight-skeptic's finding on
   the hut front), and light never crosses a wall this way.
7. **Pack** into the GPU format and a file: `LUX2` header (origin, spacing, dims, scene and settings keys, the
   counts: valid, relocated, invalid, triangles, lights), the probes, and a StableHasher hash of the bytes. Reading a
   file checks everything the sampler trusts (at least two probes per axis, the bake's spacing range, finite values,
   weights in 0..1); a fuzz test damages files at random and nothing panics.

**Work and memory are capped from the request** before anything starts (`probes × rays × (bounces + 2)` ray casts
plus a shadow ray per point light at every first hit, at most 2³⁰; the hit cache at most 256 MB; the triangles
counted while the engine gathers them, at most 2²⁰, so an over-limit box is refused before its list is built) and
refused with a structured error that names what to change (`spacing`, `rays`, the box, the lights). A cancel stops
the bake anywhere: between chunks of probes, within a probe's rays, in the BVH build. Workers take probe ranges in index order and write results by index, so thread count and timing
never change the bytes.

## Agents' tools

- `gi_bake {volume, min, max | around, spacing, rays, bounces, intensity, sky_below, margin, exclude, threads, cancel,
  remove}`:
  writes the volume's settings (`/gi/<volume>`, a group at the box's centre holding `props.gi_volume`), then bakes on
  worker threads off the frame thread. Returns at once with the estimate (probes, rays, work); progress is
  `gi_info`'s. When it finishes, the result (hash, scene key, counts, timings) is committed into `props.gi_volume.baked`
  **as the agent who asked**, so claims, the journal and undo apply. An unchanged scene with unchanged settings
  comes from the store (read on the bake's worker) within a frame or two. Each call commits at most once (the
  settings); the result is its own commit when it lands.
- `gi_info`: every volume's settings, status (`fresh`, `stale` with reasons `geometry` / `lights` / `environment` /
  `settings`, `missing` (no file in the store), `loading`, `baking` with progress, `failed` with why), what is resident
  on the GPU (chunks, bytes), the runtime tier, `gpu_error` when the GPU couldn't take the set, and the last bake's
  statistics (probes valid / relocated / invalid, rays per second, ms per step).
- `gi_check {volume?, points?}`: checks as data: share of invalid and relocated probes, cells with walls through
  them, octants that would leak without the masks (what quality low gives up), mean and peak luminance, staleness;
  with `points: [{p, n}]` the sampled irradiance at each point with the contributing probes and weights (what the
  shader computes).
- `gi_settings {quality, radius_m, memory_mb}` (lead only, like `stream_settings`): the tier (`off`, `low`, `high`;
  see Sampling), answered with the tier's description, the adapter's default (`low` on integrated GPUs, `high`
  elsewhere) and, for `high` on an integrated GPU, a `gi_tier_over_budget` warning.
- `gi_bake cancel: true` stops the running bake: only its starter, or an agent who may write `/gi/<volume>`, naming
  that volume; once it has finished computing the answer is `too_late` (it lands this frame). A start with unchanged
  settings writes nothing but still checks the volume's claims. `threads` past the default (all cores but one, at
  most 4) needs the lead.
- **Sight:** `sight_frame {gi: "off" | "irradiance" | "probes"}`: `off` renders that capture without GI (before/after
  pairs from one call), `irradiance` shows only the indirect light, `probes` draws the probes as small spheres shaded
  by their own SH (invalid ones left out, counted in the packet). Packets carry `gi: [{volume, status, chunks}]` for the volumes in view, so a stale
  bake is never silently wrong.

## Reserved data and the write policy

Volume settings are world data: `props.gi_volume` on a bare group, normally `/gi/<name>`. The record of a bake in
it (`baked`: hash, scene keys, settings key, counts) holds only what the bake determines, so it enters `world_hash`
the same on every machine and whether the bake ran or landed from the store; timings are `gi_info`'s `last_job`.
`gi_bake` is refused inside playtests (it lands by wall clock, not at a tick). They are **reserved**
(`Rule::Gi`, [WRITE_POLICY.md](WRITE_POLICY.md)): only `gi_bake` writes them or anything under `/gi`; every other
checked writer gets `reserved` with the tool to use. The group sits at its box's centre, so region claims over the
area hold for baking there too. The baked probes are **derived data** in a content-addressed store
(`.specula-cache/gi/<hash>.lux`, `--cache`): the world keeps only the hash, so journals, saves and replays stay small,
and a missing file (a fresh clone) is reported as `missing` and rebaked to the same hash.

## Streaming

A baked volume is cut into **chunks** of 64 m columns in X and Z (two streaming cells; chunks share their border probes,
so neighbours interpolate seamlessly). 64 m, not one cell, because each resident chunk is one more box the shader
tests per pixel: Sepulcra's top face is a single chunk. Chunks within `radius_m` (default 96 m) of the window's camera
or a capture's (for 5 s; a capture keeps the window's chunks too) are resident, nearest first, at most 16 at once,
at most 524,288 probes (what the GPU side holds) and within `memory_mb` (default 16 MB, at most 24); the rest falls
back to the flat ambient. **GI never fails a frame:** a set the GPU can't take (limits, memory, pipelines) draws the
flat ambient and `gi_info` reports `gpu_error`. Baked files load on a worker (never on the frame thread) within 256 MB
of host memory; captures and `gi_check` wait for a load that is running. The store keeps at most 512 MB of files,
oldest first (a volume whose file went reports `missing` and bakes again); every write and removal goes through the
disk budget (`disk::book()`, the cache category: admitted, accounted, refused below the free-space floor). At most 64 volumes: a 65th is refused. Offscreen renders without GI (studio, previews, filmstrips) leave the
window's buffer alone. The set is worked out again only when the window's camera moved a metre, a
capture came, or a volume changed. The GPU holds the resident chunks in one device-local buffer (copied through a
staging buffer; host-visible memory made every probe read slower on the iGPU) read through its device address
(`gi.rs`), rebuilt only when the set changes; the old one is retired after the frames that read it.

## Night, emissives and presets

- **Readable moonlight** (STYLE.md): the sky term is the world's ambient, so open ground keeps the night's tuned fill;
  bounce adds the moonlit slab's light to the cliffs facing away, and enclosed spaces (the crypt) fall dark except
  where the door lets light in. Emissive braziers and runes add their glow to the bounce (`emissive` × albedo, as
  drawn), on top of their point lights. **How much a fire bounces** (gi_check's corner: a brazier, range 3 m, in the
  corner of two pale walls at night, baked with its light and again without): the ambient term on the floor in front
  of it rises by 0.047 (0.157 against 0.110), beside it by 0.031, on the walls by 0.012 to 0.015. That reads clearly
  in the irradiance view and only subtly in the lit image, where the fire's direct light dominates at that range; in
  the open yard (the first round's claim) the brazier's bounce is too small to see.
- **Quality presets:** spacing (4 → 0.5 m) is the bake's preset; `gi_settings.quality` is the runtime's tier (see
  Sampling): `high` samples per pixel, `low` an ambient cube per draw and the lean probes per pixel on wide draws
  and terrain, `off` restores the flat ambient. The
  iGPU defaults to `low`, which holds the iGPU's budget (Measurements).

**The irradiance view** (`sight_frame gi: "irradiance"`) shows the ambient term alone, stretched over its own range:
the resident probes' 5th to 95th irradiance-luminance percentiles map to 0.03..0.45 before the tone curve (hue kept,
the frame's exposure undone), so a pale, evenly lit stage shows gradation instead of near-white. The packet's
`gi.irradiance_scale` and `gi.irradiance_start` give the stretch (shown = 0.03 + (luminance - start) x scale). Particles don't draw in it (their glow isn't GI); terrain shows its ambient term too.

**Engine thread:** `gi_bake` gathers the scene in one pass over the world (the instances, lights and the scene keys
together). A scene baked before is read from the store on the bake's worker, not the engine thread (decoding the file
there stalled frames 7 to 11 ms, the T3 lane's `engine.stall`); its result lands like a fresh bake's, as its own commit,
within a frame or two (`gi_info`'s `last_job.result.cached`).

## Measurements

From `scripts/gi_check.py` (Intel RaptorLake-S iGPU, Windows, 2026-10-29) and the GPU bench (`--with gi`). The machine
was shared with other sessions; the GPU numbers are the cleanest of several runs.

**Bakes** (CPU, 4 worker threads):

| Scene | Probes | Triangles | Time | Note |
|---|---|---|---|---|
| Test yard (hut, shed, brazier), 0.5 m, 256 rays, 2 bounces | 8,840 | 380 | 0.85 to 1.2 s | the same hash (`916baa496d49ead3`) on the discrete adapter with one thread (3.2 s) |
| Sepulcra's top face, 46 x 10 x 46 at 1 m, 256 rays, 2 bounces | 21,160 | 10,532 | 4.5 s | validity 1.0 s, trace 2.7 s, bounces 0.7 s; 1.0 MB on the GPU |

An unchanged scene bakes again in no time (from the store). The GI pipelines build on a worker from the bake's start:
ready 4.6 to 5.7 s after the bake finished on a quiet iGPU (validation on, no pipeline cache); the window draws the flat
ambient meanwhile, and captures wait for them.

**Leaks** (gi_check): the closed shed's floor, walls and ceiling read 0 (the open ground 0.033); the hut's floor by its
door 0.011 and at the back 0.005 (light comes in through the door and falls off).

**Shader cost** (the iGPU, 1280 x 720). First round, per pixel everywhere (before the tiers and the review's
visibility fixes):

| | Scene pass, GI off | GI on (per pixel) | GPU frame, off | on |
|---|---|---|---|---|
| GPU bench `game` (2,000 props on a slab: heavy overdraw) | 3.21 ms | 7.91 ms | 5.55 ms | 10.33 ms |
| Sepulcra, a wave in real time (perf_capture, 180 frames; means, p95 in brackets) | 3.15 (3.73) ms | 5.79 (6.96) ms | 7.53 (9.70) ms | 9.02 (10.50) ms |

What was tried then: unrolling the probe loop (slower: register pressure took the iGPU's SIMD width), sharing each 2x2
pixel quad's probe reads through subgroup quad operations (slower), sampling per vertex for small draws (slower: the
props' vertices outnumber their pixels), device-local probes (no change), a 16-byte probe record (0.5 ms saved, not
worth a format change).

**The tiers** (the owner's rule: the low preset holds the iGPU's budget; the coordinator's target was about 0.5 ms
of scene time, and the measured +1.0 to 1.3 ms in play was accepted on 2026-10-30: Sepulcra keeps about 8 ms of
headroom on the iGPU). The GPU bench's `game` scene, offscreen, the lowest of three medians of 150 frames each (scene pass):

| Bench `game` | Scene pass | Added |
|---|---|---|
| GI off | 3.21 ms | |
| low, the first try (per draw, the per-pixel sampler still in the shader for wide draws) | 4.31 ms | 1.10 ms |
| low, every draw per draw, the per-pixel code compiled out (the floor) | 3.58 to 3.61 ms | 0.37 to 0.40 ms |
| low, round 2 (lean pipelines; wide draws by a two-layer grid, four loads issued together) | 3.80 ms | 0.59 ms |
| low, round 3 (CPU grids up to 8 layers; they leaked across walls) | 3.97 ms | 0.72 ms |
| low as built (ambient cubes; the lean probes per pixel on wide draws, unrolled) | 3.97 to 3.98 ms | 0.77 ms |
| medium (per pixel past one spacing) | 7.42 ms | 4.2 ms |
| high (per pixel) | 9.20 ms | 6.0 ms |

The bench has no other load; its slab is one wide draw and its props overdraw heavily. Sepulcra, release, windowed
1280 x 720 on the iGPU, `--rules lex`, a real-time wave from wave 6 (18 enemies) on, the top face baked at 1 m: GI
off, low, medium and high in turn, eight times each (120 frames a capture), four runs. The machine was shared with
other sessions throughout: one or two other engines on the same iGPU in most captures and 10 to 19 compilers running,
so single captures swing tenfold; the lowest capture per tier is the closest to the real cost:

| Sepulcra scene pass (ms) | off | low | medium | high |
|---|---|---|---|---|
| run 1 (sorted): lowest / median | 2.63 / 3.00 | 3.67 / 5.06 | 8.09 / 10.0 | 8.07 / 13.3 |
| run 2 (unsorted) | 3.07 / 3.42 | 2.92 / 4.57 | 8.09 / 10.2 | 8.38 / 9.37 |
| run 3 (sorted) | 1.89 / 3.16 | 2.42 / 4.20 | 8.79 / 12.0 | 7.90 / 10.7 |
| run 4 (unsorted) | 2.41 / 4.88 | 2.60 / 6.99 | 7.92 / 18.1 | 8.16 / 30.4 |

Tier low adds about 0.2 to 1.0 ms by the lowest captures (0.4 on average), in line with the bench's 0.6: about the
budget, not clearly under it. The cleanest run came later, with the machine quieter (`gi_check.py`'s wave, 180 frames
a capture, every tier twice): scene pass off 2.60 and 2.79 ms, low 2.89 and 3.57, medium 7.60 and 7.91, high 7.27 and
7.70; GPU p95 off 7.2 and 7.7 ms, low 7.0 and 9.2, high 11.2 and 14.0. Low adds 0.3 to 0.8 ms there. Its GPU p95 (lowest per run 7.2 to 9.2 ms) stays inside the 14 ms frame budget.
`scripts/m3_exit.py --gi` (the M3 exit run with every measured wave captured off, low and high, twice each) agrees:
lowest scene means off 2.25 to 2.32 ms, low 3.00 to 3.79, high 8.37 to 9.14; its frame-level budgets missed under the
same contention in captures of every tier (frame pacing; one capture of 1.4 s GPU p95 with two other engines running).
**High costs 5 to 6 ms** on the iGPU: the review's visibility fixes (a reach test per corner: one more 16-byte load and
its arithmetic) roughly doubled the per-pixel sampler. It is the discrete GPUs' tier; the per-pixel variants carry no
grid or per-draw code (compiled out). The middle tier measured here (`medium`) was dropped (see Sampling).

Round 5 (2026-10-30, tier low as built: ambient cubes, the lean probes per pixel on wide draws, terrain per vertex;
release Sepulcra, `--rules lex`, a quiet machine: no compilers, at most one other engine). `m3_exit.py --gi` (waits
for each tier's pipelines; 300 frames a capture, alternating off, low, high twice per wave), scene pass means:

| Sepulcra, iGPU | off | low | high |
|---|---|---|---|
| wave 6 (18 enemies), round 1 / 2 | 2.09 / 1.98 ms | 3.27 / 3.02 ms | 7.38 / 6.81 ms |
| wave 10 (boss), round 1 / 2 | 2.16 / 2.17 ms | 3.36 / 3.43 ms | 7.36 / 7.35 ms |

So low costs **+1.0 to 1.3 ms on the iGPU in play** and high +4.8 to 5.3 ms. That is over the 0.5 ms target and was
accepted: the low preset still holds the budget (about 8 ms of headroom in Sepulcra). The leak-free wide-draw path is
the price: the CPU grids it replaced cost 0.7 to 0.9 ms in the review's measurement, and leaked. A static wave (`gitier`, eight rounds): off 1.65, low 2.70, high 5.86 ms (medians). On the
discrete RTX 4090 Laptop GPU: off 0.11, low 0.08, high 0.23 ms (high costs about 0.1 ms there). Terrain (the GPU
crate's `terrain_cost_report`, 1600 x 900, a 2 m volume over the whole terrain, iGPU): scene pass 3.49 ms without GI,
3.56 ms at low (per vertex) and 11.35 ms at high (per pixel at low it had cost 6.08 ms); the lake's water pass 2.52 ms
without GI, 2.75 ms at low (its draw's cube; the lean probes per pixel had cost 3.63 ms) and 7.15 ms at high. Building a tier's GI pipelines
took 3 s after a bake in m3_exit (30 to 112 s with an empty pipeline cache on a busy machine).

**Front-to-back batches** (opaque batches ordered near to far by their nearest draw within each pipeline variant;
instancing kept): the bench's `game` scene has two batches, so it can't show it. In Sepulcra the GI-off scene pass was
lower in the sorted builds in three of four comparisons (lowest 1.89 and 2.63 ms sorted, 2.41 and 3.07 ms unsorted),
within the noise above: no clear gain and no loss. The CPU cost is one pass over the sorted keys and a sort of the
batches (tens to hundreds), not measurable against the draw list. A depth pre-pass was not tried.

**Pipelines.** The engine has a `VkPipelineCache` (`<cache>/pipelines-<adapter>.bin`, loaded at start, saved at
shutdown). With both GI sets built at once, the GI pipelines were ready 52 and 132 s after the first bake with a cold
cache under the contention above (14 s cold on the quieter machine), and still 40 and 48 s with the warm cache from
the previous run: the Intel driver's cache hit saves little here, and the GI request waited behind the skinned and
textured background builds. Now only the tier's set is built, a world loaded with volumes asks for it at start (before
any model upload asks for the skinned and textured ones), and a bake asks at its start.

**Leaks** (`crates/specula-lux` tests, after the review): the closed hut moved 0 to 0.95 m in 5 cm steps at 1 m and 0.5
m spacing reads under 8% of the open ground everywhere from 5 cm off its inner corners (the review measured 54 to
126%). Within 3 cm of an inner edge, bounces can still carry up to 14% in (a scratch sweep: 82 points of 20,172).

## Limits and later

- **A point light inside a solid lights nothing:** fully shadowed, it adds nothing to the bake (the T3 lantern's
  flame inside its own cone). The bake reports such lights; the bake's result (`gi_info`'s `last_job.warnings`) and
  `gi_check` warn `light_inside_geometry`, naming the light's entity. Move the light out of the mesh, or `exclude`
  the mesh around it.

- Indirect light only; probes are static (a rebake after big changes: `gi_info` says when). Real-time relighting
  (DDGI-style ray-traced updates) needs a GPU ray path first.
- Textures aren't sampled for albedo; emissive glass doesn't emit into the bake.
- No specular probes (reflection captures): glossy surfaces reflect the procedural sky, occluded by the GI's ratio.
- **Tier low is an approximation:** small draws get one ambient cube (no variation across a draw: a brazier's sphere
  reads flatter than at high in the irradiance view), wide draws and terrain the probes without reaches or backface
  weights (terrain per vertex). In the round-5 captures the lit views of the crypt, the hut, the yard and Sepulcra look
  the same at both tiers (the sight-skeptic), and the sealed huts of `crates/specula-gpu/tests/gi_low.rs` stay as dark
  at low as at high (no pixel 16/255 apart); lit rooms differ in up to 1.6 to 4.6% of their pixels, near the fire.
  Water at low takes its draw's cube (one value per pond). Low costs +1.0 to 1.3 ms on the iGPU in play (accepted:
  the low preset holds the budget).
- Masks are exact at each octant's centre and reaches along fourteen directions: within a few centimetres of a
  closed room's inner edges a little light can still get in (see Leaks above). Bake such areas at a finer spacing.
