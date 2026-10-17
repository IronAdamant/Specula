# Design: streaming and HLOD

Status: designed and implemented 2026-10-18 (M3 lane "Async streaming with per-frame budgets, plus HLOD"). Code:
`crates/specula-gpu/src/stream.rs` (queued uploads, deferred frees), `crates/specula/src/streaming.rs` (cells,
sources, budgets, eviction), `crates/specula/src/hlod.rs` (merged proxies), `crates/specula/src/stream_tools.rs`
(tools). Proof: `scripts/stream_check.py`. Foundations **F3** (low-end performance: the Intel iGPU is the reference)
and **F2** (determinism: streaming never changes simulation results).

**Why:** traversal stutter is Unreal's other well-known problem ([ENGINE_SURVEY.md](../research/ENGINE_SURVEY.md)
§3): streaming in new areas does synchronous work on the game thread. Specula's answer is that nothing streams
synchronously, every frame's streaming work has a measured budget, and a miss is a structured error an agent sees.

## What loads where

| Data | Where it is made | Where it goes | Streamed? |
|---|---|---|---|
| **Model builds** (procedural recipes) | worker threads (`models::Library`, 2 workers) at spawn or when a recipe changes | CPU (`Built`, shared by content) | built once, kept while an entity uses them |
| **glTF imports** (meshes, textures, mip chains) | the same workers | CPU, then the GPU | GPU uploads streamed; textures stay once uploaded (their pixels leave the CPU) |
| **GPU meshes of builds** | engine thread, queued uploads | GPU | **yes**: per LOD, coarsest first, within the frame budget; evicted under `memory_mb` |
| **HLOD proxies** | the HLOD worker thread (`specula-hlod`) | GPU | **yes**: per cell, within the frame budget; evicted under `memory_mb` |
| **2D textures** (sprites, tilesets) | `two_d` loader thread | GPU | already asynchronous (decode off-thread, upload when ready); not budgeted yet |
| **World state** (entities, components) | `world_load` / `--world` | CPU | **no**: the simulation is always whole (see Determinism) |
| **Audio** | rendered on a worker at start (Sepulcra's procedural sounds) | CPU mixer | no (small); streamed music is the M5 audio item |
| **Pipelines** | at start and at window attach; variants on a worker | GPU | never mid-game (`pipelines_after_warm` budget) |

**World chunks.** Entity state is small (bytes per entity) and the simulation needs all of it every tick for
determinism, so worlds are never partially loaded. What streams is *presentation*: GPU meshes, textures and proxies.
A world bigger than memory (disk-backed regions) is a later step: regions would load as a whole into the simulation
on a worker and join at a tick boundary, journaled like any edit; nothing in this design blocks it.

## No synchronous loads in the frame

Before this lane, `GpuModels::sync` uploaded every newly landed build at once, each mesh with `upload_mesh`: a staging
buffer, a submit and a **fence wait** that also drained every frame already queued. A world whose builds landed
together stalled one frame for all of them, and deleting a mesh waited for both frames in flight.

Now, with streaming on (the default):

- **Queued uploads** (`Gpu::queue_mesh`, `Gpu::queue_texture_levels`): the data goes into a staging arena (8 MB,
  persistently mapped, one per batch slot) and the copy into a command buffer of copies, each followed by a barrier
  to vertex, index and fragment reads. The batch is submitted **before the next frame on the same queue** (`present`
  and every offscreen render flush it first), so that frame already draws the new mesh: a barrier's second scope
  reaches everything later in submission order. Nothing waits. Three slots cycle; a slot is reused once its fence has
  signalled (polled once a frame, at the flush, with a zero timeout: on the Intel driver even a zero-timeout poll can
  take milliseconds while the GPU is saturated). Only if all three are in flight (the GPU three batches behind) does
  queuing wait, for the oldest, and `upload_stats().slot_waits` counts it.
- **Mesh blocks:** queued meshes are ranges of 16 MB device-local blocks (first fit, freed ranges coalesce), not an
  allocation each. On the Intel iGPU a `vkAllocateMemory` costs about 0.3 ms and a free as much, which capped streaming
  at a few meshes a frame (and some drivers allow only 4096 allocations). One empty block is kept as a spare.
- **Deferred frees** (`Gpu::retire_mesh`, `retire_texture`): the resource is freed once the first batch submitted
  after the retirement has finished. A fence signal covers everything submitted earlier on the queue, so every frame
  that could have read the resource is done by then. When nothing else is queued, an empty batch carries the fence.
  Frees are spread out: at most 0.25 ms of them per poll.
- **CPU work in the budget.** Re-indexing a build's parts (`part_mesh`) and generated tangents run inside the upload
  budget; building models and generating proxies run on worker threads.
- Headless engines flush once per loop (and a minimized window once per serve window), so retired memory is freed
  without frames.

Synchronous `upload_mesh` and `destroy_mesh` stay for loading and tools; a synchronous destroy of a mesh whose queued
copy may still be pending finishes the batches first.

## Per-frame budgets

perf.toml `[streaming]`, per adapter in `[streaming.integrated]` etc., and live through `stream_settings` (role
`lead`, like the swarm frame floor):

| Key | Default | Meaning |
|---|---|---|
| `upload_ms` | 2.0 | CPU milliseconds of uploads per frame |
| `upload_mb` | 8 | bytes uploaded per frame |
| `memory_mb` | 512 | GPU memory for streamed builds and proxies |
| `cell_m` | 32 | cell size |
| `radius_m` | 384 | members load within this of a camera |
| `hlod`, `hlod_min_m`, `hlod_radius_m` | on, 48, 4096 | proxies, never nearer than 48 m, wanted within 4 km |
| `preload` | 1.25 | members load at this times a proxied cell's switch distance (at least 1.15: past the window's 10% hysteresis) |
| `settle_s` | 1.0 | quiet time before a changed cell's proxy is rebuilt |
| `capture_wait` | true | captures wait for what their view needs |
| `enabled` | true | off: the old path (everything uploaded at once, no proxies) |

The budget covers all of a frame's streaming work: filing changed entities and recomputing the wanted set count against
it before any upload, and captures' own pumps (an agent's sight call between frames) share what is left of the frame's
budget instead of getting one each. The pump estimates each item's cost from a learned fixed cost per item plus a
learned rate per MB (samples past three times the estimate are clipped: a driver stall must not starve later frames)
and stops **before** an item that would overrun the time or byte budget (the first item of a frame always goes, so
huge items still make progress; a proxy uploads one material group per item, a build one LOD). Misses are structured
`perf_frame` errors with `{error, metric, value, budget, unit, adapter}`, and `stream_state.recent_misses` names the
slowest item of each:

- `stream_upload_over_budget`: the p95 of upload ms over frames that uploaded is above `upload_ms`;
- `stream_upload_spike`: one frame took over twice `upload_ms` (one item bigger than the budget, typically);
- `stream_memory_over_budget`: what the cameras want doesn't fit `memory_mb` (the farthest stays out).

The 2 ms default is an eighth of a 60 Hz frame on the reference iGPU, measured with the game running (see
Measurements). The `frame/stream` CPU zone also takes a `[budgets.zones]` budget.

## Priority by distance and view

**Cells.** Persistent model entities are filed into `cell_m` squares (XZ) by their origin; distances are measured to
the box of the members' world boxes (their builds' bounds once built), so a big model near the camera loads whichever
cell its origin is in. The index is patched from the world's draw changes (as the window's draw cache is: an entity
that stays in its cell is updated in place), rebuilt only when the world is replaced.

**Sources.** The window's camera every frame, and each capture's camera for 10 s after it (at most four), so an agent's
next capture from about the same place finds its view resident.

**Wanted** (recomputed when a new capture view or a jump of the window's camera asks for it at once; otherwise at most
every 100 ms while the window's camera moves or the world or the builds change, and at least once a second): a cell's
members are wanted when a source is within `radius_m`, unless the cell's proxy is current and the source is beyond
`preload` × its switch distance (the proxy draws them). A cell whose proxy is stale or still coming (an edit, a build
landing) has its mergeable members stand in for it as far as proxies are wanted (`hlod_radius_m`), so it never vanishes
while its proxy is rebuilt. Models that can't be merged (glass, textured, animated, moving) are drawn only within
`radius_m`: raise it for long views of such models. Priority is the distance to the cell's box, halved inside the
window's view frustum and quartered for captures (an agent is waiting); transient model entities (a game's
presentation) are always wanted first. Proxies are wanted within `hlod_radius_m`, at half the priority of their cell
(they're cheap and cover a lot); built proxies of cells no longer wanted are dropped.

**Progressive levels.** A build uploads one LOD per item, **coarsest first**. An entity whose chosen level isn't
resident yet draws the finest resident one (`LodStats::coarser`); it is never drawn from a level finer than the data.
Entities with nothing resident aren't drawn (`not_resident`). So a new area appears coarse within a frame or two and
sharpens, and nothing waits.

## Residency and eviction

Uploaded builds and proxies count against `memory_mb` (`GpuModel::bytes`, proxy bytes). Before an upload that would go
over, what **no source wants** is evicted, least recently wanted first (builds and proxies by the same clock) through
`retire_*`: never a wait. A textured glTF build's pixels left the CPU at upload, so when an evicted one is wanted again
the library imports it again (its entities show as building meanwhile). If the wanted set alone exceeds the budget, the
lower-priority remainder stays out and is reported (`stream_memory_over_budget`); once nothing more can be evicted in a
frame the rest are skipped at once, and captures don't wait for them. Nothing is evicted eagerly: a small world never
evicts. Mesh-block slack (fragmentation, the spare block) is outside `memory_mb` and reported in
`perf_frame.streaming.gpu` (`mesh_block_mb` vs `mesh_block_used_mb`).

## HLOD

Per cell, a **proxy** merges the cell's static models into one mesh per material (usually 1-3 draws per cell):

- **Members:** persistent, shown, non-`ui` model entities without `anim`, without a kinematic or dynamic body, with a
  ready build whose coarsest level is opaque (`standard` presets) and untextured. Everything else in the cell draws on
  its own (within `radius_m`).
- **Geometry:** each member's **coarsest reachable LOD** (`min(lods - 1, 3)`), transformed into the proxy's frame (the
  members' centre, rounded to a metre, for precision far from the origin), normals by the inverse transpose, the part
  and entity colours baked into the vertex colours in linear light exactly as the shader multiplies them (a white tint
  leaves them bit for bit). Generated on the HLOD worker; a pure function of its input sorted by entity id, with `dmath`
  colour conversions: **the same members give the same bytes** on every run (`ProxyData::hash`, unit-tested with
  shuffled input).
- **Switch distance (no popping):** a member at distance `d` draws its last level once its screen size is below that
  level's threshold with the window's hysteresis, `T = LOD_PIXELS[L-1] × (1 - LOD_HYSTERESIS)`, i.e. beyond
  `d_i = r_i × viewport_h / (T × tan(fov/2))`. A cell's switch distance is the largest `d_i` (at least `hlod_min_m`),
  measured to the box of the members' centres, never farther than any member. Past it **the proxy is the members' own
  geometry**, so the swap is invisible. The window enters the proxy 10% past the switch distance, leaves it inside the
  switch distance (hysteresis), and leaves only once every member is resident (their coarsest level, which is what the
  proxy showed). Captures decide afresh, without hysteresis (reproducible).
- **Staleness:** a cell's content key hashes its members' ids, content hashes and builds. A change makes the proxy
  stale at once (no longer drawn; the members stand in, see Wanted), and a new one is built once the cell has been quiet
  for `settle_s`, so an agent editing a far cell never sees a ghost of the old content. An edit of a mergeable entity in
  a cell the window draws as a proxy updates the cover at once, not at the next periodic update.
- **Drawing:** covered members are skipped in `GpuModels::draw_model` by a bitset indexed by entity id
  (`hlod::covers`, one branch per model draw); proxy draws are appended after the entity draws. The draw-list build
  itself (sight.rs, the draw cache) is untouched apart from owner id 0 for proxy draws in captures.

## What agents see

- `perf_frame.streaming`: settings and who set them, this frame's uploads (count, KB, ms, deferred, memory-blocked,
  evicted), the upload-ms series, totals, resident and budget MB, live / uploaded / partial / wanted / pending builds,
  sources, cells, the learned ms/MB, HLOD (cells current, stale, building, window cells and covered entities, proxy
  draws, build ms), the GPU's queued-upload counters; budget misses in `errors`.
- `models` (perf_frame and every packet): `not_resident`, `coarser_lod`, `hlod` counts beside the LOD histogram.
- **Sight packets:** `streaming {hlod_cells, hlod_entities, not_resident, coarser_lod, note}`; subjects get
  `drawn_as: "hlod"` (drawn merged: `visible_px` is null, the pixels belong to the proxy) or `"not_resident"` (built,
  not on the GPU, not drawn), and `resident_lods_from` while finer levels stream. A warning names not-resident models.
  Captures park until what their view needs (within `radius_m`, not proxied) is resident, like they wait for builds,
  so a capture shows the world as a player standing there would see it.
- Tools: `stream_state` (summary, nearest cells with residency and proxy state and switch distance, entities not
  resident there), `stream_settings`, `stream_fly` (flies the window camera along a path and reports frame p50/p95/
  p99/max, CPU frame, upload ms, frames over budget **that were uploading**, draws, HLOD cells, not-resident peaks).

## Determinism

Streaming only reads the `World` and writes GPU resources and its own bookkeeping: entity state, resolved models
(handles and build states come from the model library, which streaming doesn't touch) and the world hash are
identical whatever is resident. HLOD generation reads copies (`MemberInput`); proxies are presentation. Where frames
land in time never reaches the fixed-tick simulation. Checked: `streaming::tests` (filing the world never changes its
hash), `stream_check.py` (the hash after building the proof world with streaming on equals a second run's with
streaming off, and a flight doesn't change it), and the Sepulcra run in Measurements.

## Measurements

See the lane report and `scripts/stream_check.py`'s `report.json` (numbers on the Intel iGPU, 1280x720).

## Later

- Disk-backed world regions (above).
- 2D textures through the same budget; texture eviction with CPU copies or re-import.
- Dithered cross-fades for the few cells whose members aren't all static (today they never merge).
- A worker-side `part_mesh` (re-indexing off the engine thread) if profiles show it matters.
- Sub-allocated device memory (one `vkAllocateMemory` per mesh today; `maxMemoryAllocationCount` is 4096 on some
  drivers, which the memory budget and proxies keep well clear of in the proof world).
