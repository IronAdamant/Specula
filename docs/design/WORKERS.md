# Workers and step jobs: keeping the engine thread free

Status: built 2026-10-30 (the bounds lane). Code: `crates/specula/src/workers.rs` (the worker pool and
`Engine::offload`), `crates/specula/src/steps.rs` (`Engine::step_job`). Guard:
`crates/specula/tests/engine_thread_guard.rs`. Measured by the strict-budget harness
(`scripts/budget_check.py`).

## Why

One engine thread owns the world, the GPU and the journal. Agents' calls are served between the
window's presents (`schedule.rs`), and the tracing lane times every call against its class budget
(`engine.stall`, which also says how long the thread itself ran, `cpu_ms`: well under `ms`, it waited
rather than worked, and `lock_ms` says how much of that was for the model library's lock, which the
build workers share; 4 ms for reads and edits, 30 ms for renders, disk work, big batches; see
`trace_sink::Budgets`). A root-cause investigation (2026-10-29) found that the budgets were being
broken by the same mistake over and over (recurring bug class C):

| Instance | Measured | What held the engine thread |
|---|---|---|
| `lease_release {delete: true}` | 8.5 s for 20k files | deleting a scratch folder |
| `anim_check` with 256 copies of a clip | 1.2 s | every check in one call |
| `anim_filmstrip` | 322 ms | every tile rendered, read back and encoded in one call |
| `sight_frame` / `sight_orbit` / `sight_compare` at 2048² | 290 ms | the PNG encode |
| an explicit `world_save` at 20k entities | 310 ms | serialising and writing the world |
| `world_hash {verify: true}` | 150 ms | replaying the whole journal |
| project mirroring | per file | `fsync` of each written file |

The root causes:

1. The scheduler gates when a call *starts*; nothing bounds a call once it runs.
2. There was no standard way to move work off the thread: fourteen hand-rolled threads, each with
   its own answer path, slot counting and failure handling (or none).
3. A blocking operation looks like an ordinary function call, so nothing stops the next one.

## The rule

> Engine-thread code never touches the disk, encodes an image, deletes a folder or loops over
> unbounded work. It snapshots what it needs and hands the rest to a worker (`offload`), or splits
> work that needs the engine (the GPU, the world) into budgeted steps (`step_job`).

The guard test makes the rule mechanical (below); the strict-budget harness proves it at the
largest arguments every schema allows.

## `Engine::offload`: one-shot work off the thread

```rust
let job = Offload::agent(JobKind::CaptureEncode, owner, &session.id).lane(Lane::Any);
self.offload(job, move |cancel| encode_and_write(sheet, path), |engine, result| {
    // On the engine thread, once the work is done (or failed, or was cancelled).
    let _ = reply.send(answer_from(result));
})?;
```

- **work** (`FnOnce(&Cancel) -> R + Send`) runs on a worker thread. It may not touch the engine:
  it owns what it was given (pixels, a write plan, paths). It may poll `cancel.is_set()` in loops.
- **then** (`FnOnce(&mut Engine, Result<R, Failed>)`) runs on the engine thread when the result
  is back (`Engine::pump_workers`, once per loop pass in both loops). It must be cheap: it applies
  a result (stamps, the saved-world cache) and answers. It is timed like any engine-thread work
  (`engine_work_done`, zone `worker_then`).
- **The answer** goes through `jobs::Reply` (the only way to make an answer channel): `then`
  usually owns the reply and sends it; a job that needs no engine follow-up answers from the worker
  (`offload_answer`). Either way every deferred answer has one owner and one sender.

### Bounded slots

- Two worker threads (`WORKERS`), started on first use and named `specula-worker-N`. The
  reference machine (Intel iGPU laptop) has four cores: the engine thread, the window thread and
  the protocol's connection threads need the rest.
- Agent jobs: at most `MAX_AGENT_JOBS` (64: one per agent of a 64-agent swarm, each waiting for its
  answer) queued or running, at most `MAX_PER_OWNER` (4) per agent (by identity key, which a resume
  keeps), and at most `MAX_PENDING_BYTES` (1 GiB) of pixels held between them. Past any of these,
  `offload` refuses with `job_busy` (naming whose jobs fill the queue, `retry_after_s`), and the
  caller answers with it *before* doing any expensive engine-thread work (a capture checks
  `workers_room` before it renders).
- A playtest's own steps (call 0) never count against agents' slots: one playtest runs at a time and
  holds the clock while it waits (review of 998af3d: a flood of agents' calls failed a playtest).
- Engine jobs (writes the engine owes: project mirroring, the trash, world files, replay and perf
  files, drops) are never refused and never run inline: an inline 20,000-file delete is the stall
  this pool exists to prevent (review of 998af3d). Their sources are bounded instead: one world
  write, one project sync and one trash emptying at a time (the trash is one job however many
  folders are released), a replay or a perf capture at a time. Work runs inline only when no
  worker thread could be started.

### Lanes

Writes to the same place must not overlap. A job names a `Lane`: `Any` (encodes), or a serial lane
(`Lane::World` for world files, `Lane::Project` for project mirroring, `Lane::Trash` for deletes):
jobs of one serial lane run one at a time, in the order they were offloaded; other lanes and `Any`
jobs run beside them.

### Cancel, drop and failure

- **Session end**: `pump_workers` checks the live sessions about four times a second; an agent
  job whose session closed is cancelled. A queued one never starts (`then` gets
  `Err(Failed::Cancelled)`); a running one sees `cancel.is_set()` and may stop early.
- **Drop guard**: `then` owns its reply. If the engine shuts down with jobs pending, they are
  drained first (`Engine::workers_drain`, up to 10 s: writes the engine owes complete); anything
  still pending is dropped, and dropping a reply answers its caller "shutting down" (the reply
  channel closes). A pending job is never silently lost.
- **Panics**: the work runs under `catch_unwind`; a panic becomes `Err(Failed::Panicked(message))`
  and `then` answers `worker_failed`. The worker thread survives.
- **No thread**: if the pool can't start a thread, work runs inline (it still completes).

### What the trace sees

Each job carries its call's trace context (`trace::Ctx`): the worker enters it, so events the work
raises follow the call (`trace_call`), and `then` is timed against the call's class.

## `Engine::step_job`: work that needs the engine, in slices

Some work can't leave the thread: it renders on the GPU, reads the world or the journal. It runs as
a `StepJob`:

```rust
trait StepJob {
    fn kind(&self) -> JobKind;
    /// One slice: at most about a millisecond or one GPU render and readback.
    fn step(&mut self, engine: &mut Engine, reply: &Reply) -> Step; // More, Done(answer), Handed
}
```

- Started by `Engine::step_job(start, job)`: its first slices run at once, for up to 2 ms, so a
  small job answers in its call and is never refused; one that must queue is refused (`job_busy`)
  past `MAX_STEP_JOBS` (64) agents' jobs or `MAX_STEPS_PER_OWNER` (4) per agent. A playtest's
  steps always queue. Journal replays (`world_hash {verify}`) also take one of `MAX_REPLAYS` (2)
  slots (each holds a copy of the journal's base world); the rest wait their turn cheaply, and
  replay towards the journal as it was at their call.
- Pumped once per loop pass (`pump_steps`), round-robin over jobs, within a credit like the
  studio's: earned at a share of wall time (`WINDOW_SHARE` 0.25 with a window, `HEADLESS_SHARE`
  0.8 headless), spent by each step's measured time. In debt nothing runs, so a slow step on a
  busy iGPU is followed by frames without step work.
- A step returns `Step::Done(answer)`, `Step::More`, `Step::Wait` (nothing to do until a worker's job
  returns: the job goes to the back, the jobs behind it get their slices, and the pass ends once every
  queued job has waited; the filmstrips' motion checks, and a render still on the GPU), or
  `Step::Handed` (it offloaded its finish,
  an encode, with a clone of the reply).
- **A job with a render on the GPU** (`perf_light`, `timeline_preview`) says so
  (`StepJob::on_gpu`) and answers `Step::Wait` while the render is in flight, never `Step::More`
  (which asked to be stepped again at once and spun on the fence for the whole credit). With a
  window, a pass is once a frame, which landed one render a frame (light review 4: a report took
  3.7 times as long at 20 frames a second); so while any queued job has a render on the GPU the
  window's serve wait also *pulses* (`Schedule::serve_with`): it runs `pump_steps` about every
  millisecond, between calls, in whole-millisecond waits (no busy wait), and not at all when no
  job has a render on the GPU (or has just landed one and has more to do). A pulse starts only
  when what a landing costs (the last landing's cost, kept apart from a look's and not forgotten
  with time) fits before the frame's deadline, and none runs before a landing of the queued jobs
  has been timed by the frame's own pump. When the window pulsed, the frame's own pump leaves
  those jobs alone (`Schedule::frame_pump`): a render that lands after the window's last pulse is
  landed at the next window's start, not in the frame's time. The credit is the same: a pulse
  earns what the time since the last pump earns. Headless, the loop already goes round at once
  while a step job is queued.
- Past its caller's deadline a job answers `timed_out`; when its session closes it is dropped
  (answered `session_closed`).

This generalises what the studio's renders and `vfx_preview` already did (a frame per step within
a render credit). The studio keeps its own queue (its renders answer the studio's caller-thread
waits, not a tool reply), and `vfx_preview`'s sheet encode now goes to the pool instead of its own
thread.

## What moved (C1-C7)

| | Before | Now |
|---|---|---|
| C1 `lease_release {delete}` | `remove_tree_inside` on the engine thread | the folder is **renamed** in place to `.<name>.trash.<pid>.<n>` (one rename in the same folder, never through a link; a shared trash folder made renames wait behind the delete), then the whole trash is emptied by one coalesced job on `Lane::Trash` through `disk::remove_tree_inside` and accounted in `disk::book`; leftovers from a crash are swept at start |
| C2 `anim_check` | every check in one call | a step job: one subject (clip, state or transition) per step; duplicate clips are refused by the schema (`uniqueItems`) |
| C3 `anim_filmstrip` | every tile, readback and the encode in one call | a step job: a row's poses and checks per step, one tile render per step, the sheet's encode and writes offloaded |
| C4 `sight_frame`, `sight_orbit`, `sight_compare` | PNG encode, base64 and file writes inline | render and readback stay (they wait on the GPU, see below); encode, base64 and the PNG and packet writes are offloaded; the answer is sent from the worker |
| C5 explicit `world_save` | `persist::save_for_agent` inline (and a blocking wait for an autosave in flight) | the engine plans (takes the changed subtrees, as the autosave always did), the write runs on `Lane::World`, `then` updates the saved-world cache and answers; an autosave in flight is waited for by queueing behind it on the same lane, never by blocking |
| C6 `world_hash {verify}` | a full journal replay inline | a step job like the trace's replay spot-check: the replay a few entries per step, then the replayed world hashed in steps |
| C7 project mirroring | `write_entry` (temporary file, `fsync`, rename, read-back) on the engine thread | the sync is planned on the engine thread (`Disk::sync_plan`: which paths, which texts), written on `Lane::Project` (`SyncPlan::execute`, which stamps each file as it writes it, so the watcher never mistakes the engine's write for an outside edit), and applied when it returns (`Disk::sync_apply`: pending writes, held files, races imported, hot reload); `project_write` and `project_patch` answer once their file is on disk, as before |

### Ordering: project calls wait for a sync in progress

When syncs ran on the engine thread, the engine's view of the disk (stamps, pending writes, held files) only
changed between whole syncs. To keep that, the project calls that read or write the disk (`Snapshot`, `Commit`,
`ModuleTexts`, `CommitFixes`) are held in order while a sync writes (`project_deferred`) and run when it ends;
edits the watcher found wait in its channel the same way. A `project_mount`, which answers in its own call, waits
for the sync (rare). Otherwise a commit's `refresh` could read a file the worker had just renamed into place but
not yet stamped, and import the engine's own older text as an outside edit.

Autosaves and explicit saves share `Lane::World`; an explicit save asked for while one is writing waits in a
short queue (4), and `world_load` writes the waiting saves first.

### What stays on the engine thread, and why

- **A capture's preparation and submit** (draw list, subjects' projections, the GPU's own
  per-draw records): they read the world and own the Vulkan queue. Since the stalls lane the
  render itself is not waited for (below, "The capture flight").
- **A world save's final step and a project sync's plan**: they read the world, which only the
  engine thread may. The save's copies are taken in slices (below).
- **Loading** (`world_load`, `save_read`'s apply): a load replaces the world. Its read and parse
  run on a worker (`save_tools`; `world_load.rs` since M5 gpuwork, below); the replacement itself
  (`Live::replace`: the journal's base copy, the old world dropped) stays: about 25-40 ms at 5,000
  entities.
- **The bounds overlay's two renders**: a capture with `overlays: ["bounds"]` and the ID buffer
  renders, counts, paints the boxes from the counts and renders again, waiting for the GPU
  (a debug view, not a hot path).

## The capture flight (stalls lane, 2026-10-30)

Code: `crates/specula/src/capture/flight.rs`, `Gpu::render_async` / `Gpu::settle_offscreen`
(`crates/specula-gpu/src/gpu.rs`).

- **Submit, don't wait.** A capture's view is prepared on the engine thread
  (`Engine::render_view_submit`) and submitted with `Gpu::render_async`: its own command buffer
  and fence, the readback copy recorded in the same submission. The call returns; the engine
  serves other agents and presents the window while the GPU renders. Everything the packet says
  about the world (warnings, streaming notes, terrain and water kinds, GI, the camera) is taken
  here, when the frame is drawn.
- **One view in flight.** Later captures wait in the capture queue in the order they came
  (at most 64, then `job_busy`). An orbit's views go back to back (its next view goes to the
  front of the queue), so one orbit at a time holds a sheet; the sheet is made when its first
  view lands and is charged to the workers' byte cap (`Workers::hold`) until its encode job holds
  it. A call still waiting past its deadline is answered and dropped; one whose owner has no room
  left on the workers waits (rendering it would end in a refusal) while later ones go. The packet
  warns when the world changed between an orbit's views.
- **A pass's share.** Each loop pass gives captures a share of the engine thread (6 ms with a
  window, 16 ms headless): it waits for the view in flight a bounded time
  (`Gpu::settle_offscreen_within`: at most 3 ms windowed, 12 ms headless; longer renders land on a
  later pass, polled), reads its ID counts and pixels, lands it (the packet, the encode on a
  worker) and submits the next view at once, and again while the share lasts. Each step is timed
  against the render budget on its own. Headless, the system timer is held at 1 ms while captures
  are busy (and a second after), so the loop's 1 ms waits end on time. A compare's diff, heat
  panel and sheet are made on the worker. In a world of more than 4,096 entities a capture
  starts on the next pass, not inside the call that worked out its views (the views are kept
  while the world is unchanged): the two together took 35-55 ms at 20,000 entities. Headless,
  the subject cache's rebuild uses the rest of the pass's share (not 3 ms), and the capture
  starts in the same pass once it is done. A new size's target is made in a pass of its own and
  that pass's captures end there. Its render graph's images (depth, HDR, the bloom chain, SSAO,
  the ID buffers) are then made ahead, a 4 ms slice a pass (at least one image), by
  `Gpu::prewarm_target`: a prepare-only call that declares the frame with the world's environment
  and the latest frame's content (`ContentHint`: shadow casters, overlay, 2D, effects, water,
  blended), creates only the transients the pool lacks, and binds, retires and waits for nothing
  (no pipelines, no shadow-map rebind: what sank the earlier warm-up frame, a 300 ms window frame in
  `stall_load.py`). The view renders on the pass after the last slice, and its own prepare finds
  every image (`gpu.prepare_graph` 25 ms at 2048 x 2048 before, 0.01 ms after;
  `scripts/size_prewarm_check.py`, Intel iGPU: 17 slices for 7 new sizes, the worst 8.3 ms; on a release build with the iGPU loaded by other engines one large image's allocation took 45-56 ms in its slice: the driver's `vkAllocateMemory`, still off the render's submit; allocating on a worker is the follow-up). A
  frame whose content the prediction missed makes the missing images itself, as before.
- **Why not synchronous headless.** Main rendered and read back synchronously inside the call,
  which is the fastest for small captures but holds the engine thread for the whole GPU frame:
  250-500 ms for a 2048 x 2048 capture of 20,000 entities, past every budget. The bounded waits
  give small captures their synchronous throughput (the GPU is done within the wait) and keep big
  ones within budget, with one code path for both modes. Measured, release builds on the Intel
  iGPU (`scripts/capture_drill.py thru`, 4 agents, 10 s, 100 cubes, 320x180): headless 142
  `sight_frame`/s (main 120), median 29 ms (main 36), 12-view orbit 0.03 s (main 0.05);
  windowed 113/s (main 88), median 37 ms (main 51), orbit 0.04 s (main 0.06). 60 concurrent
  700 px orbits: 60 of 60 succeed, heap peak 50 MB (main 60 of 60, 49 MB).
- **The flash watch's feed** (every second pixel of the capture into its view's history) runs on
  the worker that holds the pixels (the encode job; for orbit tiles, sight_query and filmstrip
  tiles a small job of its own). Its place in the view's order is taken when the capture lands
  (`FlashWatch::feed_for`): the view's state travels with the feed and is handed to the next
  capture of the view (which waits for it on its worker, never on the engine thread); a feed
  dropped unrun hands the state on unchanged. Packets carry the same `flash` report as before.
- **Exclusive use.** Any other offscreen render (filmstrip tiles, effect previews, the studio,
  `render_view` for entity filmstrips) lands the flight first (`land_capture_flight`, which may
  wait): they share the ID buffers and the cached targets. At the GPU level every other offscreen
  use settles a pending render first; the window's frames never do (they share nothing a pending
  render uses, and retired resources are freed by queue order). A mesh or texture destroyed while
  a render is in flight (a `world_delete` during a capture) is retired in queue order instead of
  freed at once (the review of 4c3e6c2 found the use-after-free, then a lost device;
  `crates/specula-gpu/tests/async_render.rs` keeps it).
- **The subject cache.** Every persistent entity's subject data (`sight::SubjectBase`: name,
  path, shape, colour, position, corners) is kept from the world's change record
  (`World::draw_changes_since`), patched with what changed (a renamed entity's subtree has its
  paths read again); a capture only projects. Past the change record (more than 1,024 changes:
  bodies moving every tick) it is patched from the entities' draw stamps (`Entity::draw_stamp`,
  the draw epoch of each entity's latest change, `World::draw_stamps_reach`): one walk of the
  stamps finds what changed, so the cache never falls behind (the review's churn probe: every
  capture answered `engine_busy` after 30 s). After a new world, a change to every entity or
  changes to more than half the entities, it is rebuilt a slice (3 ms) per pass while captures
  wait; the rebuild then patches what changed meanwhile from the stamps, so it always finishes
  (one overtaken by a change to every entity ends in one synchronous read). Subjects are data until the
  packet is made (`Subject::full`, on the worker); the ID buffer's results are a `Visibility`
  per subject. Captures are byte-identical to before (PNGs and packets, the bounds overlay
  included; `same.py`-style comparison in the lane report).

## The stalls lane's other fixes

| | Before | Now |
|---|---|---|
| `world_query` / `world_list` | a walk of every entity with a path string each, then every item made and measured | the total counted over the entities without paths; the page walked from just after its cursor (subtrees before it skipped unvisited), paths made for its items only; pages of 64 items or more listed on a worker. Since the fix round, a tag or creator filter reads only its index's entities (`World::tagged`, `made_by`, kept by link, unlink and update): a filter nobody passes costs nothing, and up to 2,048 candidates are ordered by path without a walk; a scoped query (`path_prefix`) counts by walking its subtree alone (a count over every entity was 19-22 ms at 100k for a 6-entity scope) |
| `vfx_get` | the built-in effects parsed by the first caller (25-85 ms in a dev build) | parsed on a worker at start |
| `world_save` plan | every changed top-level subtree copied in the call | copied in 2 ms slices a loop pass (`persist::Planner`), finished with the world as it is then (copies whose subtree changed since are taken again); loose entities unchanged since the previous save reuse their records (autosaves too). Since the fix round the top level is scanned in slices too (kind and stamp of each top-level entity, stale copies copied again in the slices): after the scan each step catches up with the top-level entities whose subtrees changed since it began (`World::top_changes_since`, a stamp-ordered log kept with the subtree stamps: O(changes)), oldest first, a slice at a time, and `finish` reads only what changed since the last step (a body moving every tick: one entity; review of 07f3276 measured 56-122 ms when it read the whole top level); a save being planned is flushed before `save_read` or `world_load` replaces the world, and a plan whose world was replaced anyway answers `world_replaced` and writes nothing; the loose section's items are made in those slices too, and the previous save's cache is freed on a worker (dropping 100,000 records held the engine thread 85-95 ms, on main too). `big_save.py` at 100,000 entities: no stall under `--strict-budget` (main: 85-127 ms); the first save takes 4.3 s of wall time in 2 ms slices (main 1.0 s in one stall) |
| `desync_trace` gather | every journal change marked in the call | a step job, 200 changes a step (`desync::Gather`), entities evicted meanwhile marked by `entities_from` |
| `anim_check` | a step job (one state's three cycles 30-80 ms a step) | on a worker (the checks read only the job's own data) |
| `lease_take` (scratch), `lease_renew` | folder and marker made on the engine thread (up to 40 ms on a busy disk) | made on a worker; the call answers once they exist (a failure gives the lease back) |

### What the stalls lane left (`budget_check.py`, 20,000 cubes, Intel iGPU, dev build)

Before: `sight_frame` 427 ms, `sight_orbit` 2.6 s, `sight_compare` 480 ms, `desync_trace` 380 ms,
`anim_filmstrip` 57 ms a step, `world_save` 37 ms, `world_query` 15 ms, `lease_take` 39 ms. After,
each over its budget at most once or twice a run, and within about twice it:

| Stall | Engine-thread time | Why it is left |
|---|---|---|
| a capture's first render at a new size | 36-67 ms (2048x2048) | fixed in M5: the images are made ahead, a slice a pass (`Gpu::prewarm_target`, `size_prewarm_check.py`) |
| `anim_filmstrip` tile | 34 ms | a tile renders and reads back synchronously (it shares the offscreen targets with captures); an async tile needs the GPU's one pending slot shared with the capture flight |
| `sight_compare` diff at 2048x2048 | 35 ms | its submit (the draw list and subjects of 20,000 entities, the GPU's per-draw records) |
| `world_query` / `world_list` with a filter | 4.3-4.5 ms (read budget 4) | the total is a count over every entity; an index per common filter (tags, creator, shape) would make it O(matches) |

### What the fix round left (2026-10-30, same harness)

The filmstrip tile, `world_query` / `world_list` and the compare diff no longer stall. What is
left is one view's preparation at 20,000 entities, now a step of its own (never part of the
call that asked): the draw list (5-7 ms), the subjects' projection (6-8 ms), the GPU's frame
preparation and submit (4-6 ms) and the packet's facts (2-3 ms), 20-30 ms on a quiet iGPU laptop
and 32-56 ms, now and then, with other sessions building. It is O(entities) per view; the next
step is a draw cache for captures (the window has one) and the subjects projected on a worker.
`budget_check.py`'s `KNOWN` lists those tools with their measured peaks (dev build: 36-55 ms).

Runs vary with machine load. One release run passed `budget_check.py` with no stall at all
(without `--known-ok`); three of the review's release runs (of 07f3276) stalled on these three
tools, peaking at 55 ms (`sight_orbit`) and 70 ms (`sight_compare`). So the per-view preparation
is a known stall on release builds too, and `KNOWN` holds the peaks seen with about 40% headroom
(`sight_frame` 80, `sight_compare` 100, `sight_orbit` 80 ms). **The milestone audit runs
`budget_check.py --known-ok`**; a known tool past its maximum, or any other stall, still fails.
`lease_release {delete}` (40-95 ms on main and here: renaming a folder of 20,000 files and
canonicalising its path) now moves the folders into the trash on a worker and answers from there.

## Recipe writes and studio sheets in the window (M5 render follow-ups, 2026-11-06)

The M4 exit proof's builder load (4 builders writing, checking and staging 33 recipes, then a contact sheet) still
raised frame-budget incidents in Sepulcra's window on the iGPU. `scripts/recipe_stall_check.py` reproduces it (the
same load, smaller) and reads every `engine.stall` and `frame.over_budget` of the run. Before: 51 `project_write`
stalls (`worker_then`, 4-66 ms, growing with the recipe folder), 37 window frame stalls (up to 154 ms) and 3
`frame.over_budget`. The causes, each found from zones (`perf_capture` over the writes):

- **The model library's poll on the engine thread.** Each project write forced `Library::poll`: a `stat` of every
  cached recipe file, reads of changed ones, and a re-key of **every** recipe in the folder (each parsed again),
  under the library's lock. Now a poll is `scan_plan` (no disk) / `ScanPlan::run` (a worker, `JobKind::AssetScan`) /
  `apply_scan` (`asset_scan.rs`); a project write puts its text (or its deletion, or a style) into the cache at once
  (`note_written`) and re-keys only the written recipe and those including it (`RecipeInfo::includes`); a scan
  planned before a note, or before a newer applied scan, is dropped (it read an older disk). `model_reload` keeps
  the synchronous poll (it answers with the result).
  **What a scan compares.** A file's time, once that time is more than two seconds old (`models::FRESH`,
  `stamp_of`: the time is taken before the file is read, with the clock read on both sides). File times move
  in steps of the clock, so a file written again within the step it was read in keeps its time; a text read
  sooner than that after its file's time has no time in the cache, every scan reads it again (on the worker;
  `model_reload`'s own poll where it runs) and compares what it says, and a file that says the same changes
  nothing. So a second write within a tick of the first is seen, at the cost of two or three more reads of a
  file just written. Not seen, as before: a writer that puts a file's old time back; and on a share whose
  server stamps the writes with a clock more than two seconds ahead of this machine's, two writes in one of
  its ticks. (A file renamed into a folder whose time did not move is found by a request for it, not by a scan.)
- **Model uploads waiting for the GPU.** A rebuilt model landing uploaded synchronously (`upload_mesh_with`, a fence
  wait per mesh) and freed the old build with `destroy_mesh` (waits for frames in flight). Now uploads are queued
  (`queue_mesh`, `queue_texture_levels`) and frees retired (`retire_*`), and a window frame uploads for at most 3 ms
  (`GpuModels::upload_slice`, windowed engines only), the rest on the next frames.
- **Studio renders waiting for the GPU.** Each studio render rendered and read back synchronously. Now one is in
  flight at a time (`render_async`), landed by a later pump with a poll; a new size's target is prewarmed first; with
  a window its GPU time is charged against the studio's 25% share too. Captures and studio renders give way to each
  other at most three passes (`TURN_PASSES`), then land the other's render and go.

Two more causes the zones showed on the way: re-resolving every model entity (with a draw-cache reset) after
each write, 18 ms (now only the entities of the handles re-keyed, `Reload::handles`, plus the entities left
unresolved, each distinct spec once, when files changed: they have no handle a re-key could name, and a written or
fixed recipe must bring them back), and scans re-keying a whole folder when one file changed (now only the changed
recipes and their includers). Review R1 then found the re-key itself at 12 ms on the engine thread under the load
(parsing recipes and reading their anim sets): the engine's library re-keys in 2 ms slices (`REKEY_SLICE`, at least
one recipe a call), the rest on the next loop passes (`Library::rekey_more`), the handles left keeping their last
build meanwhile; `model_reload`'s synchronous poll is never sliced.

After, the same script on release builds, the iGPU shared with six other lanes' builds and engines (runs vary a
lot with that load): `project_write` stalls went from 51 (worst 66 ms) to 0-6 a run, almost all 4-10 ms (one of
21 ms and one of 33 ms seen); `frame.over_budget` from 53 to 0 in the last three runs. What is left are window
frames over 30 ms when the GPU is saturated: the queued upload path waiting for a command buffer or staging slot
(`gpu.stage`/`vk.begin_cb`, up to 47 ms), texture and image allocation (`gpu.create_image`, `gpu.make_image`:
45-61 ms for one large image on a loaded iGPU, also the worst prewarm slice), and Sepulcra's HUD waiting for a fence
(`game_overlay/vk.wait_fence`, up to 32 ms). The next step is allocating images on a worker thread (Vulkan allows it)
and a non-waiting staging slot policy.

## GPU memory, view preparation and loads off the engine thread (M5 gpuwork, 2026-11-07)

What the render follow-ups left (above) was allocation, not drawing:

- **The image heap** (`specula-gpu/src/heap.rs`). Every image (graph transients, textures,
  targets, the window's depth, GI volumes) is placed in device-local blocks of 64 MB that a worker
  thread (`specula-image-heap`) allocates ahead: after each placement the heap keeps a free range
  of 40 MB per memory type (a 2048 x 2048 RGBA16F image), and empty blocks are freed there too.
  The engine thread allocates only when an image must be placed now and nothing holds it (bigger
  than a block, or a burst past the room ahead): counted as `sync_blocks` (perf_frame
  `memory.image_heap`, with `grown_ahead`, `growth_waits`, `deferred`, `freed_ahead`,
  `buffers_ahead`), and their room is the `reserve` memory kind. A capture's prewarm slice never
  allocates: with no room it defers (a growth sized for the image starts) and the next slice places
  it. Drivers that require an image's own allocation still get one.
- **Buffers made ahead.** A target's readback (16 MB at 2048 x 2048, host-visible) and sight's ID
  readback and sets for a new size get their memory on the same worker
  (`Device::create_buffer_ahead`); the prewarm polls them, the render adopts them.
- **Uploads never wait for a slot**: six batch slots (three before), and a window frame's model
  uploads stop for the frame when every slot is in flight. `update_texture` (a HUD's glyph atlas
  growing) is queued in the upload batch instead of a staging allocation and a fence wait.
- **Captures' draw list** comes from a cache of their own (`DrawCache::for_captures`: one without
  and one with the `ui` layer, LODs chosen afresh as before), patched with what changed instead of
  a walk of every entity per view; `tests/draw_cache_fuzz.rs` holds it equal to the walk.
- **Subjects projected on a worker.** A view's subject bases are taken on the engine thread
  (`sight::subject_set`: the cache's bases in the frame, 2D cards as drawn); their projection runs
  on a worker (`JobKind::SubjectProject`) while the GPU renders. **Whoever gets there first
  projects:** the worker if it picked the job up before the view settled, else the view itself, at
  once (`captures.late_projections` counts those); a worker in the middle of it is waited for (at
  most 20 ms, under the render budget: the work the view would otherwise repeat; it was 250 ms, which a
  worker switched out on a busy machine could use up). A view never waits passes for a worker:
  it used to wait up to three, which made an orbit's pass count depend on the pool's timing
  (review R1). Views that paint overlays project at once (the overlays need the boxes).
- **world_load** (`world_load.rs`) checks the write policy, force and unsaved changes, then reads
  and parses the world, writes the outgoing board and reads the new one on a worker in the world
  files' lane; its continuation checks again (agents may have written meanwhile) and replaces the
  world. A load waits while the world's folder is being written (until its request's deadline);
  a second load meanwhile is refused with `job_busy`, naming the loader, and `retry`. At 5,000
  cubes the engine thread went from 112-250 ms to about 42 ms (the replacement).
- **A load is a world write.** From the moment a load is accepted until it has answered, a
  `world_save` is refused (`job_busy`, the loader as `owner`, `retry`), and the autosave and the
  board's autosave skip their turn. Before this (review R1, the blocker) a save that arrived
  while the load read was written after it and named the loaded world after the one it
  replaced, so the next save wrote the new world over the old one's folder; and an autosave in
  that window saved the changes `force` discards. As a backstop for every path, a save's or an
  autosave's result that comes back to a different world (`world_generation`) marks nothing.
- **The image heap has two threads.** Askers' own allocations (a target's readback, which its
  first render may wait for on the engine thread) go to one; speculative 64 MB block growth to
  the other. On one thread the readback queued behind the growth: 45-178 ms on a loaded iGPU
  (review R1).

`scripts/size_prewarm_check.py` and `scripts/recipe_stall_check.py` read the image heap's
counters and fail when a block was allocated on the engine thread during their load.

## The guard

`tests/engine_thread_guard.rs` scans the engine crate's non-test code (the shared lexer of the
write-policy and disk guards: comments, literals and test items blanked) for blocking calls in
engine-thread modules: `fs::` calls, `sync_all`, `remove_dir_all`, PNG encodes, `read_rgba8`,
`thread::Builder` / `thread::spawn`, and since the review of 998af3d disk work through the engine's
own helpers (`disk::book()` writes, appends, copies and folders, `remove_tree_inside`,
`remove_file_inside`, `persist` reads and writes) and file-system stats (`.exists()`,
`.metadata()`, `.is_file()`, `.is_dir()`, `.canonicalize()`, `.read_dir()`). Imports that would hide
`fs` calls (`use std::fs as x`, `use std::fs::{..}`, `use std::{fs, ..}`) are refused everywhere.
Moved to workers in that review: `desync_trace`'s render and write, a playtest's replay file, and
`perf_capture`'s files. Each file may make only the calls its allow-list entry
counts, with the reason (a worker-side module, a caller-thread tool, start-up, a bounded small
read); a new one anywhere else fails, as does a count that grows. Moving a call to a worker
lowers the count, and the test then asks for the lower number, so the list only shrinks.

## The strict-budget harness

`scripts/budget_check.py` starts a headless engine with `--strict-budget`, builds a large world
(20k entities, a 20k-file scratch lease, a long journal), then calls every tool twice: with
schema-maximal fuzz (the largest string and array each property allows: the bounds must refuse or
bound it cheaply), and the heavy tools with their largest *valid* arguments (2048² captures, the
largest orbits and filmstrips, 256 distinct clips, a 1000-op batch, a 20k-entity save, a verify
over the whole journal, a 20k-file delete). It reads `trace_incidents` and fails on any
`engine.stall`, and the engine's exit status (a strict run fails at shutdown when a stall
happened). The integrator runs it at milestone audits.

### What it found (2026-10-30, Intel iGPU, a dev-profile build, 20,000 cubes)

The C1-C6 instances no longer stall: `lease_release {delete}` of 20,000 files answers in 32-47 ms of
wall time with nothing on the engine thread past budget, `world_hash {verify}` runs in about 100
one-millisecond steps, the explicit `world_save` no longer blocks on an autosave. What the harness still
reports, each a finding for a later lane (none is a regression; each was there before, hidden behind
the bigger stalls):

| Stall | Engine-thread time | Why |
|---|---|---|
| `sight_frame`, `sight_compare` at 20k entities | 250-500 ms | the subject list (`sight::subjects`, about 90 ms for 20k entities), the render's fence wait and the readback: O(entities) per capture, at any image size (256x256 costs nearly as much) |
| `sight_orbit` (12 views) | 2.5-4 s | 12 such renders in one call: an orbit should become a step job (a view per step), once a view itself fits a budget |
| `world_query` / `world_list` at 20k | 11-40 ms (read budget 4 ms) | filters scan every entity |
| `vfx_get` after big world changes | 35-45 ms | the effect library's rescan of every entity |
| `world_save` plan at 20k loose entities | 36-49 ms (disk budget 30 ms) | the plan copies every changed top-level entity |
| `anim_check` / `anim_filmstrip` single steps | 30-60 ms | one state check (3 cycles) or one tile on a loaded iGPU: the smallest slice `specula-anim` offers |
| `desync_trace` of 600 ticks at 20k entities | 230-270 ms | gathering the ticks from the journal (its 3-12 MB file is rendered and written on a worker since the review of 998af3d) |

`budget_check.py` marks these KNOWN and any other stall NEW (with its zone); `--known-ok` passes a run
whose only stalls are known ones, which is how the milestone audit runs it until the follow-up lane
lands.

### Window frames under heavy calls (`scripts/stall_load.py`)

Sepulcra windowed on the Intel iGPU (1280x720, seed 7), one agent looping the heavy tools (2048x2048
`sight_frame`, a 12-view orbit, the largest filmstrip, `anim_check`, `world_save`, `world_hash
{verify}`, `lease_release {delete}` of 20,000 files, `project_write`), a poller reading the window's
frame pacing every second; release builds of main (d679814) and this lane, alternating; no builds or
other engines running (the usual desktop apps open):

| Run | Worst window frame | Median of per-second p95 |
|---|---|---|
| main, 1 | 4,407 ms | 8.9 ms |
| bounds, 1 | 170 ms | 9.0 ms |
| main, 2 | 6,378 ms | 9.2 ms |
| bounds, 2 | 188 ms | 9.2 ms |

The multi-second frames were the scratch delete; what is left (150-190 ms) is the orbit and the
2048x2048 capture's render and readback (above). The cost: an `anim_filmstrip` of 128 tiles now takes
about 6 s of wall time with a window (its steps get a quarter of the engine thread) instead of 0.4 s
in one stall.

