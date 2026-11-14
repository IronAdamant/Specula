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
| **2D textures** (sprites, tilesets) | `two_d` loader thread | GPU | **yes**: decoded off-thread, queued uploads within the frame budget (first, before meshes); a texture that doesn't fit waits a frame, still "loading" so captures wait for it |
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
  persistently mapped, one per batch slot) and the copy into a command buffer of copies, ended by one memory barrier
  to vertex and index reads (not one per copy: on the Intel driver every recording call can stall). Each slot has its
  own transient command pool, made with the device and reset whole when the slot is reused (vendors' advice: no
  per-buffer resets). The batch is submitted **before the next frame on the same queue** (`present`
  and every offscreen render flush it first), so that frame already draws the new mesh: a barrier's second scope
  reaches everything later in submission order. Nothing waits. Three slots cycle; a slot is reused once its fence has
  signalled (polled once a frame, at the flush, with a zero timeout: on the Intel driver even a zero-timeout poll can
  take milliseconds while the GPU is saturated). Only if all three are in flight (the GPU three batches behind) does
  queuing wait, for the oldest, and `upload_stats().slot_waits` counts it.
- **Mesh blocks:** queued meshes are ranges of device-local blocks (first fit, freed ranges coalesce), not an
  allocation each. On the Intel iGPU a `vkAllocateMemory` costs about 0.3 ms and a free as much, which capped streaming
  at a few meshes a frame (and some drivers allow only 4096 allocations). Blocks grow 16, 32, then 64 MB, so a big world
  loading makes a few allocations, a small world one small one. One empty 16 MB block is kept as a spare.
- **Blocks grow ahead, on a worker thread.** A block's `vkAllocateMemory` takes 2-160 ms on the busy iGPU. Allocating
  ahead in a frame that does nothing else was tried first and doesn't hide it: that frame takes the whole allocation
  (125 ms measured). So the pump starts the next block early and its memory is allocated off the frame:
  - **How:** the block's buffer is made on the engine thread, without memory (`Device::create_buffer_unbound`). A
    thread named `specula-mesh-alloc` allocates the memory through a `RawAllocator` (the device handle and the two
    function pointers). The next flush binds it and starts using the block (`Device::adopt_buffer`); if binding fails, the
    buffer and the memory are both freed. `Uploads::destroy` (always run by the GPU's drop, with the device idle and still
    alive) joins the thread and frees memory it allocated that was never adopted.
  - **Why the worker is safe:** Vulkan lets any thread use a `VkDevice`. `vkAllocateMemory` has no externally
    synchronized parameters. `vkFreeMemory` has only the memory it frees, and only the thread that got that memory
    frees it. The validation callback the worker may trigger writes through a mutex. That is the `SAFETY` reasoning
    behind `RawAllocator`'s `Send`.
  - **When:** a block grows when uploads are queued and under 60% of the next block's size is free. At 8 MB of uploads a
    frame, that leaves a few frames for the allocation to finish before anything needs the new block. Blocks grow only
    while they hold less than `memory_mb`, so they never pass the budget by more than one block, and a world filling its
    budget still gets its last block ahead. The **first block** is made synchronously by the first upload, since there
    is nothing yet to hide it behind. A failed allocation, or a failed bind, stops growth for 1 s.
  - **Fit rule:** while a block grows, the pump puts off an item that doesn't fit the free space already there, so the
    frame never waits for the worker. A build must fit **twice** its estimate in one free range: the estimate counts 28
    bytes a vertex, and parts may also carry skin (8) and UV + tangent (16) streams. A proxy group is one mesh with an
    exact estimate, so it must fit that estimate.
- **Driver stalls.** On the Intel iGPU's Windows driver, recording calls (`vkBeginCommandBuffer`, `vkCmdCopyBuffer`)
  sometimes take 10-200 ms while a big world loads (measured 2026-10-22, zoned per call): never on the RTX, never in
  frames that don't upload, and the same with a fresh pool per batch reset whole. They are constant-time calls on our
  side; `stream_check.py` reports frames over budget only inside them apart (`over_budget_in_driver_recording`).
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
and stops **before** an item that would overrun the time or byte budget (the first item of a frame always goes, even
when the frame's filing and wanted-set work alone used the budget, so huge items and busy frames still make progress;
a proxy uploads one material group per item, a build one LOD; nothing goes while every upload batch is in flight).
Captures with no frame started for `FRAME_STALE` (100 ms: a test, nothing driving frames) start a budget of their
own: a budget is per frame, and spent work never blocks uploads for good. Misses are structured
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

**Wanted.** A *pass* walks every cell and works out what its members and its proxy are wanted for. It is spread over
frames (each frame spends half the upload budget on it, at least one cell), so a big world's update never costs a frame
more than that; a capture from a new view or a jump of the window's camera runs a whole pass at once (an agent is
waiting). Passes start when the window's camera moves (a quarter cell, or a tenth of the way to the nearest cell, a turn
or a zoom: at most every 100 ms), when the world or the builds change, and at least once a second. Changed entities
are filed the same way, a slice at a time (an agent's 1,000-spawn batch costs milliseconds to file). When more changed
than the world's change record holds (1,024 changes), every entity is filed again, also a slice at a time, while the
old cells serve, then the changes since; refilings in a row that can't catch up get twice the slice each (it used to be
a rebuild in one frame: 3-6 ms for 8,000 entities). A pass's end is sliced too, in two steps that may run in different
frames. The **pending** step makes the list of what to upload, nearest first. The **tidy** step drops cells and proxies
nothing wants and schedules the next pass. Together they used to take 1.6-3.5 ms in one frame on the busy iGPU; now
each step takes at most 1.3 ms. Captures file everything at once, and a capture redoes the changed cells in its view at once, out of turn (it never sees the world
as it was there), without a whole pass.

- Models that can't be merged (glass, textured, animated, moving) are wanted within `radius_m`, and beyond it wherever
  they are at least `RESIDENT_MIN_PX` (4) pixels across: a glass tower 500 m away stays drawn. The same holds for every
  model with `hlod` off, and for mergeable ones past `hlod_radius_m`.
- **Orthographic** views (2D, top-down maps) measure sizes from the view's height, not the eye's distance: everything in
  the view is wanted at its size there. They never draw proxies (their switch distances are perspective ones).
- Mergeable members are wanted wherever a camera wouldn't draw the cell's proxy: within `preload` × its switch
  distance when the proxy is current, whatever `radius_m` says (so there is no band where neither draws).
- A cell whose proxy is stale or still coming (an edit, a build landing) has its mergeable members **stand in** for it
  as far as proxies are wanted (`hlod_radius_m`); beyond `radius_m` (and the switch distance) they are wanted *softly*:
  only their coarsest level, and they give way to anything nearer or hard-wanted under the memory budget.
- **Wide** models (wider than a cell: a terrain, a cliff range) are wanted by their own box and never merged, so one
  big model never stretches its cell over the world.
- Priority is the distance to the cell's box (or the wide model's), halved inside the window's view frustum and
  quartered for captures (an agent is waiting). Transient model entities (a game's presentation: creatures, effects)
  aren't in cells: every frame, those whose builds aren't fully on the GPU go first, without waiting for a pass, and a
  capture waits for the ones in its view. Proxies are wanted within `hlod_radius_m` at half their cell's priority;
  built proxies of cells no longer wanted are dropped.

**Progressive levels.** A build uploads one LOD per item, **coarsest first**, and only down to the finest level a
camera needs (its size on screen at that distance, with the window's LOD hysteresis as margin): a model 300 m away
never has its LOD 0 uploaded. An entity whose chosen level isn't resident yet draws the finest resident one
(`LodStats::coarser`). Entities with nothing resident aren't drawn (`not_resident`). So a new area appears coarse
within a frame or two and sharpens, and nothing waits.

**A capture that forces a level** (`sight_frame {lod}`, an orbit's or a mark's) wants that level for what its
views see, whatever the size on screen, and waits for it like any level its view wants (`capture_wait`). Before
2026-11-15 the level was chosen by size alone: `lod: 0` from a far camera drew the finest level on the GPU,
counted in `coarser_lod`, in that capture and in every one after it. Members merged in a proxy stay merged
(`hlod`), and a build the memory budget keeps out is still drawn coarser and counted.

## Residency and eviction

Uploaded builds and proxies count against `memory_mb` (`GpuModel::bytes`, proxy bytes). Before an upload that would go
over, what **no source wants** is evicted, least recently wanted first (builds and proxies by the same clock), then
soft stand-ins and wanted proxies the window isn't drawing, farthest first, for anything nearer (a hard-wanted item may
evict any of those), through `retire_*`: never a wait. The eviction queue (two heaps, no full sort) is scanned a slice
a frame after each pass while the budget is full; the previous queue serves meanwhile, each entry checked again against
the latest wanted set (nothing wanted hard, or soft and no farther, is taken). The queue used to be rebuilt in the frame
after each pass: 0.6 ms on average, up to 2-3.5 ms on a busy iGPU. A textured glTF build's pixels left the CPU at upload, so when an evicted one is wanted again
the library imports it again (its entities show as building meanwhile). If the wanted set alone exceeds the budget, the
lower-priority remainder stays out and is reported (`stream_memory_over_budget`); once nothing more can be evicted in a
frame the rest are skipped at once, and captures don't wait for them. Nothing is evicted eagerly: a small world never
evicts. Mesh-block slack (fragmentation, the spare block) is outside `memory_mb` and reported in
`perf_frame.streaming.gpu` (`mesh_block_mb` vs `mesh_block_used_mb`).

## HLOD

Per cell, a **proxy** merges the cell's static models into one mesh per material (usually 1-3 draws per cell):

- **Members:** persistent, shown, non-`ui` model entities without `anim`, without a kinematic or dynamic body, with a
  ready build whose coarsest level is opaque (`standard` presets) and untextured, and that fewer than `INSTANCED_MIN`
  (8) shown entities use: a build many entities share is already drawn instanced (one draw for all of them, at any distance),
  so merging it would only add draws (the M2 10k field stays instanced). Everything else in the cell draws on its own
  (within `radius_m`, or beyond where big enough on screen).
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
  for `settle_s`, so an agent editing a far cell never sees a ghost of the old content. A changed cell (an edit, or a
  member's build rebuilt by `model_reload`) is marked dirty at once: the window redoes the ones it draws as proxies out
  of turn in the running pass, a capture redoes those in its view the same way, and no capture draws a dirty cell's
  proxy. A capture waits for a coming proxy only when its members aren't on the GPU to draw meanwhile.
- **Drawing:** covered members are skipped in `GpuModels::draw_model` by a bitset indexed by entity id
  (`hlod::covers`, one branch per model draw); proxy draws are appended after the entity draws. The draw-list build
  itself (sight.rs, the draw cache) is untouched apart from owner id 0 for proxy draws in captures.

## What agents see

- `perf_frame.streaming`: settings and who set them, 2D texture uploads (`textures_2d`: count, MB, held back), wanted / standing-in / blocked builds, blocked proxies, this frame's uploads (count, KB, ms, deferred, memory-blocked,
  evicted), the upload-ms series, totals, resident and budget MB, live / uploaded / partial / wanted / pending builds,
  sources, cells, the learned ms/MB, HLOD (cells current, stale, building, window cells and covered entities, proxy
  draws, build ms), the GPU's queued-upload counters; budget misses in `errors`.
- `models` (perf_frame and every packet): `not_resident`, `coarser_lod`, `hlod` counts beside the LOD histogram.
- **Sight packets:** `streaming {hlod_cells, hlod_entities, not_resident, coarser_lod, note}`; subjects get
  `drawn_as: "hlod"` (drawn merged: `visible_px` is null, the pixels belong to the proxy) or `"not_resident"` (built,
  not on the GPU, not drawn), and `resident_lods_from` while finer levels stream. A warning names not-resident models.
  Concise answers keep `drawn_as`, `resident_lods_from`, and the packet's `streaming` and `models` blocks whenever
  something is merged or not resident; `sight_query` answers `drawn: false` with a reason for a not-resident entity and
  `drawn_as: "hlod"` for a merged one.
  `sight_compare` doesn't diff the visibility of subjects drawn in a proxy on either side (no pixel count of their
  own); it counts them in `visibility_not_compared_hlod`.
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

`python scripts/stream_check.py` (Intel iGPU, 1280x720, 2026-10-18; a 24 x 24-cell world of 8,316 procedural
entities, 6,021 builds, 128 MB budget, 2 ms / 8 MB upload budget per frame; `--baseline-exe` for the old path).
Other sessions shared the machine, so frame times are noisy (most slow frames are `frame/present`, the GPU itself);
the checks attribute each slow frame to its zones.

| | before (main) | streaming |
|---|---|---|
| loading, slowest frame | 6,093 ms (`sync_meshes` 5,820) | 52 ms; `frame/stream` p99 2.0 ms, max 4.2 |
| loading, frame p99 | 991 ms | 39 ms |
| flight at 25 m/s, frame p99 | 237 ms (streaming off, same binary: 69) | 28 ms; `frame/stream` max 2.0 ms, 0 frames over from streaming |
| upload ms per frame | - | p99 1.0, max 1.6 |
| far vantage: draw calls / submitted | 7,350 / 9,738 | 2,573 / 3,733 with HLOD (instanced builds stay out), 0 pixels different |
| far vantage: draw-list CPU, mean / p95 | 4.9 / 11.1 ms | 0.42 / 0.53 ms |
| switching streaming off (upload everything) | - | the engine is busy for 10-90 s |

The world hash is identical with streaming on and off, the flight doesn't change it, and Sepulcra plays tick-identically
both ways. Loading had 1 frame of 2,400 with `frame/stream` over twice the budget (within the check's 0.1 %
allowance for driver stalls). Earlier runs on a busier machine had up to 5, from a slow `vkBeginCommandBuffer` and
descheduled pumps.

**Loading, 2026-10-22: growing blocks ahead on the worker vs main.**
- **Method:** `stream_check.py --skip off,sepulcra --cells 24`, run alternately with main's build and the lane's. The
  load is the number of cargo and rustc processes and the CPU % at the start of each run. The counts are loading
  frames over budget because of streaming.
- **First series** (the worker and its fit rule, before blocks could grow up to the full budget):

  | Pair | Machine load | main | lane |
  |---|---|---|---|
  | 1 | 1 cargo, 1 rustc, 17-22% CPU (the only quiet pair) | 1 | 0 |
  | 2 | main 4 / 7 at 62%; lane 1 / 4 at 32% | 0 | 1 |
  | 3 | 2-3 / 5-6 at 42% | 0 | 0 |
  | 4 | 2 / 1-5 at 28-40% | 5 (fails) | 1 |

  Totals: main 6, lane 2.
- **Second series** (the final version), on a much busier machine (frame p99 128-488 ms in both builds):

  | Pair | Machine load | main | lane |
  |---|---|---|---|
  | 1 | 1-2 cargo, 0-6 rustc, 42-50% CPU | 9 (fails) | 3 (fails) |
  | 2 | 2 / 1-4 at 44-65% | 1, and 2 flight frames (fails) | 2 (fails) |
  | 3 | 3 / 6 at 57-66% | 2, and 2 flight frames (fails) | 1 (passes) |

  Main passed 0 of 3 runs, with 4 flight frames over budget. The lane passed 1 of 3, with none.
- **What changed on the frame:** main allocates 4 blocks on the frame per run, up to 164 ms each. The lane allocates
  only the first, 2-5 ms. A pass's end took up to 2.9 ms in main; in the lane each step takes at most 1.3 ms.
- **What remains:** the lane beats main in every series. On a heavily loaded machine the loading check is still not
  reliably green. Its slowest frames are mostly inside `vk.record` (17-75 ms), the driver stall above, which the check
  reports apart. The rest come from the first block, `vk.begin_cb` and `vk.reset_pool`, `gpu.finish`, the resident
  recount, and a pass's pending step.

## Known limits

- **The 2× margin** of the fit rule can be exceeded by a build with many tiny parts (each range is rounded up to 256
  bytes) or with vertices shared between parts (each part gets its own copy). The cost is one wait on the frame for the
  rest of an allocation, not a hang.
- **At most one empty block, up to 64 MB, may be held** when `memory_mb` fills with textures while the mesh blocks still
  hold less than the budget: a block grown then sits unused. No second one grows, since its free space is above the
  threshold.
- **A synchronous allocation ignores the budget.** When no block is growing and a mesh fits no free range, the block
  allocator adds a block at once without checking the budget. Fragmented blocks can therefore hold more than the
  resident bytes: `memory_mb` limits resident bytes, not the space blocks hold.

## Later

- Disk-backed world regions (above): a world bigger than memory. It needs the world file format to load a region as a
  journaled edit at a tick boundary (world files and save games), and isn't needed for traversal without stutter,
  which is what this design is for.
- 2D textures are never evicted (sprite sheets and tilesets are few and small next to meshes); evicting them would
  mean keeping CPU copies or re-reading the file, like textured models already do.
- `part_mesh` (re-indexing a level's parts) runs on the engine thread inside the upload budget: measured at 0.3-0.5 ms
  for 40-60 items a frame in the proof world, so it stays there until a profile says otherwise.
- Cells whose members aren't all static never merge, so no proxy swaps for them and nothing to cross-fade.
