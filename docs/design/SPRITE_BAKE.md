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
  When a row a strip would make the atlas taller than 4096 (many short strips: every clip of
  a model at a few frames), the strips are packed one after another instead, wrapping where a
  row ends, in about a square atlas: their frames are still consecutive sheet frames.
- `assets/sprites/<name>.sheet.json`: the frame data, project file kind `sprite_sheet` (`<name>.sheet.json` directly in a
  folder named `sprites`: see "Where the files go" below; `sprites` is also a folder PNGs are
  mirrored in). Format
  `specula.sprite_sheet`, version 2: the atlas size, the cell, the columns, the **pivot** (the
  model's origin on its ground, in pixels from each cell's top left), the **metres per pixel**,
  the view and **elevation** (the angle the camera looks down, in degrees), the light, the
  source (recipe, params, seed, the model's name and content hash); each strip's clip,
  direction (compass name and facing in degrees), first frame and count, fps and each frame's
  seconds, playback (loop or once), duration, the clip's travel per cycle (`distance_m`), its
  events by frame, which feet are down in each frame, and **`sprite`: the 2D path's own sprite
  object** that plays it. **No list of cells**: every cell is where the grid puts it (a strip's
  frame k is sheet frame `first + k`, at column `(first + k) % columns`, row
  `(first + k) / columns`, packed or not), and its time is the strip's (`frame_times`), so the
  reader derives them; version 1 listed each cell (its rectangle, clip, direction, frame,
  time, duration, pivot in the atlas) and still reads. The shambler's walk at eight directions
  (64 cells) was 29,194 bytes in version 1 and is 5,723 in version 2, written a strip a line;
  the cold try's wraith (every clip,
  264 cells) was 106,322 and is 17,696.

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
   One scale holds every clip, so one clip that reaches far shrinks all the others: the cold
   try's wraith, every clip at once, was a few pixels tall in a 128 cell because its `swoop`
   reaches 2.5 times as far as a middling clip (and not sideways from its root, so `in_place`
   does not help). The answer names such a clip (`scale_set_by`: the clip, and how many times
   coarser the sheet is than a middling clip alone would be, when more than 1.5) so the agent
   can bake it in a sheet of its own.
4. **Every refusal**, before any render, with the numbers: an atlas past 4096 pixels a side or
   7 Mi pixels (`atlas_too_large`, with the width, height and bytes it would have been); past 512
   cells; a clip past 64 frames at the rate asked (`clip_too_long`); an unknown clip; clips on a
   model that has none; pixel art without a palette; a path that is not a sprite file.

Then the bake is **a job of the caller's identity** (`sprite_bake::job`; docs/design/WORKERS.md
and AUTHORITY.md, "Jobs"). Nothing of it waits on the engine thread:

1. The style files (the project light's rig, pixel art's palette) are read on a worker.
2. Each cell is **submitted** to the GPU on a target of the bake's own (`render_async`, its
   colour and ID buffer copied back in the same submission), and **landed** in a later step once
   a poll of the fence finds it done (`settle_offscreen_within(0)`): meanwhile the job answers
   `Step::Wait` and says it has a render on the GPU (`on_gpu`), so the window's pulses look at
   it about every millisecond, as `perf_light`'s renders are landed. Landing is two copies out
   of mapped memory (`Gpu::read_landed`, which never waits and never records a copy). Another
   tool's render on the GPU is never waited for: the bake's goes after it. If another target's
   ID render came between the bake's render and its landing (its ids gone), that cell is
   rendered again. The target's graph images are made a slice a step before the first render
   (the first step of the old bake, which made them, held the windowed iGPU 169 ms).
3. A landed cell's resolve (alpha from the ID buffer, the supersampled colour, the palette, the
   outline, the bleed) and its blit into the atlas run on a worker (two at once), while the GPU
   renders the next cell.
4. The atlas is encoded (`capture::encode_png`, the captures' worker-side encode), checked as
   the project checks a PNG, the frame data read back by its own reader, and the PNG put in the
   content store, on a worker.
5. The two files are committed **by the engine when the bake ends**, in one journal entry,
   through the project's own commit (`ProjectCall::CommitFiles`, which waits for a sync in
   progress as every project commit does), as the agent that asked (its name and identity key),
   under the project rule, and charged to the call that started the bake (one call, one
   commit). A claim on either path that appeared while the bake ran refuses the commit then
   (`claimed`, the world as it was, nothing written), and so does either file having changed
   (`changed`).

The engine-thread guard has no allowance for `sprite_bake`: its files make no file-system
call, no PNG encode and no blocking readback.

**The call and the job.** The call waits for its bake up to `wait_s` (0 to 20 s, default 20,
under the engine's 25 s answer deadline): a bake that ends by then answers in its call, as
before. One still running answers `{job, status: "running", phase, cells_done, cells}`, and
the bake goes on: `sprite_bake {job: N, wait_s}` gives its progress, or waits for its answer.
The answer is kept for its owner (by identity key, so another connection of the same agent, a
shell's next `specula call`, reads it) for 30 minutes, at most eight finished bakes an owner;
another agent asking is refused `not_job_owner`, naming the owner. A bake outlives the
connection that started it (a shell's `specula call` closes its session at once): its step job
and its worker jobs are kept when the session closes (`Engine::step_job_kept`,
`Offload::outlives_session`) and counted as the agent's all the same. Its job's deadline is 15
minutes. `out_of_time` is gone: a bake is never refused for being long. The ceilings (atlas
size, cells, frames a clip) stay refusals before any work.

- **Busy:** one bake writes an atlas at a time: another bake of the same path is refused
  `job_busy`, naming the owner of the one running and its job; an agent runs at most two bakes
  at once.
- **Cancel:** `{cancel: true}` stops the caller's bakes, `{cancel: true, job: N}` that one. A
  bake is its owner's: another agent's cancel is refused `not_job_owner` naming the owner
  (`owners` when no job is named). A cancelled bake frees its target (one whose render is still
  on the GPU goes back once a later poll finds it done), lets its pin go, takes back what its
  encode put in the content store, and answers `cancelled`, nothing written.
- **Too late (review 1, finding 2):** a cancel is honoured until the bake begins writing its
  two files: while it renders, while its atlas is encoded, and once it is encoded and stored
  (the stored atlas is then taken back: `ContentStore::withdraw`, only while nothing else has
  put or read the same bytes and nothing refers to them). From the moment the bake begins its
  writing a cancel is refused `too_late`, saying the files are being written and that the
  answer says what was written (`world_undo` takes both back); `{cancel: true}` with no job
  lists such bakes under `too_late` and does not count them. One atomic state decides
  (`sprite_bake::Stop`: running, cancelled, writing), so a cancel and the bake's own start of
  its writing can never both win. Before, a cancel while the bake encoded or wrote was ignored
  and answered "cancelled: 1 ... nothing of it is written" while both files were committed.
- **A lead's forced cancel (review 1, finding 4; AUTHORITY.md, "Jobs"):** `{cancel: true,
  job: N, force: true}` from a lead or the owner (`authority::of_session`) stops another
  agent's bake; the bake logs it on the board when it stops (`sprite_bake`, "forced cancel of
  ann's bake of ..."), and its answer says who stopped it. Force from anyone else is refused
  `force_needs_lead`; force names one job; a lead without `force` is refused like anyone. A
  lead (or the owner) may also read any bake (`{job: N}`) and list every bake running or kept
  (`{list: true}`, at most 32, newest first); anyone else lists and reads their own.
- **A caller with no name (review 1, finding 4):** a bake is kept for its owner's identity,
  and a caller that never said `agent_hello` (a shell's `specula call` without `--as`) has
  none but its session's id, which ends with the session: a bake kept past its call would be
  read by nobody (the reviewer saw one, owned by `specula-cli-2ba666`, commit and its answer
  become unreadable). So such a caller's bake runs **within its call or not at all**: asked
  with `wait_s: 0` it is refused at once (`needs_a_name`, saying how to name itself:
  `agent_hello {name}`, or `specula call --as NAME`); if it has not ended at its `wait_s`, it
  is stopped and the call answers `needs_a_name` with the same words, nothing written; if it
  is already writing its files then, the call waits up to 2 s more for its answer and gives
  it. A named caller's bake is kept, as before.
- **Where it goes:** one rule, said by the schema, the parser, the file kinds and the refusals
  alike (`sprite_bake::PATH_RULE`, `sprite_path_ok`): the atlas is a `.png` **directly** in a
  folder named `sprites` under `assets/` (`assets/sprites/wolf.png`,
  `assets/art/sprites/wolf.png`), its frame data beside it as `<name>.sheet.json`. Before, the
  parser took a `.png` anywhere below a `sprites` folder while the frame data's kind needed
  its parent to be one: `assets/sprites/h11/held.png` passed the parser and was refused
  `not_a_sprite_path` with a message restating the rule it met (review 1, finding 5).
- **A test's gate:** with `SPECULA_TEST_BAKE_HOLD` in the engine's environment (atlas file
  names, comma-separated, each with an optional phase), a bake of that atlas holds while the
  engine's clock is paused (`time_control`), its phase saying so: `held.png` before its first
  render, `enc.png@encoded` once its atlas is encoded and stored (a cancel there is honoured
  and lets it go at once), `wr.png@writing` once its writing has begun (a cancel there is
  refused `too_late`; the clock played, it writes). The live check uses it to cancel a bake in
  each phase and to take a claim on a bake's path before it ends; nothing depends on how fast
  the GPU is. In a release build it is read too (the same class as
  `SPECULA_TEST_WORKER_DELAY_MS`): it acts only on an atlas of that exact name while the clock
  is paused, and the bake's deadline still ends it.

**What a bake holds, and when it goes (review 1, findings 1 and 3).** A bake holds a render
target (from the engine's cache, alone while it runs) and a **pin** on its model's build, which
keeps the model uploaded between frames though no entity wants it. The pin was the session's
(pinned once, under the session that started the bake), and the studio prunes a pin whose
session has closed or that sat 60 s unrefreshed: a shell's `specula call` closes its session
at once, so within a second the pin went, and in an engine with a window every cell then
uploaded the model again in `posed_draws` for the window's next sync to drop it (the reviewer
measured a bake started by `specula call` at 54.4 s against 13.1 s from a kept session, with a
`gpu.flush_uploads` zone of mean 28.5 ms and max 368 ms a step; and a textured model, whose
textures leave the CPU at their first upload, cannot be uploaded twice: its bake would have
ended `render_unavailable` midway). The pin is now **the job's** (`Engine::pin_for_kept_job`):
held by no session and no clock, never pruned, asked for again at each step (a map lookup),
and let go at once when the job ends (`Engine::studio_unpin`): no pin outlives its job. The
step queue calls a job's end hook (`StepJob::on_end`) on **every** way it leaves the queue:
answered (in its call or a pass), refused `job_busy` after its first slices ran, past its
deadline, its session closed (never a kept job), the engine shutting down (every queued job is
answered `shutting_down`). There the bake gives its target back (one whose render is still on
the GPU waits in the step queue's orphans until a poll finds that render done: never waited
for, except at shutdown) and lets its pin go. A bake's answer counts its uploads of the build
(`cost.uploads`: 1, or 0 when an entity had it on the GPU), so a check reads what happened
without a clock.

**Cells whose ids are taken.** A landed cell is read out of its readback in the same step its
poll finds it done (it used to wait there for a worker slot, where another tool's ID render
could take its ids). Another tool's render can still settle the bake's and render IDs of its
own before the bake's next step: that cell is rendered again, and was so without end under ID
captures running all the while. It is rendered again at most `MAX_RERENDERS_PER_CELL` (8)
times; past that the bake ends `render_contended`, nothing written, its target given back.
Held by a test that renders IDs on another target between every render of the bake and its
landing.

**Measured** (2026-11-24, a windowed engine at 1280x720 on the integrated GPU, Intel
RaptorLake-S, `--no-activate`, a debug-profile build; the shambler's walk at 16 directions and 32
frames, 512 cells, as jobs; the engine's own stall record, `engine.stall` in its trace):

| | cell 64, supersample 2 | cell 112, supersample 4 |
|---|---|---|
| the second builder's (a render and its readback waited for on the engine thread) | 20.2 s | `out_of_time` after 444 of 512 cells |
| this build | 13.6 to 24.2 s over four runs | 18.6 to 27.4 s, never refused |

- The engine thread no longer waits for the GPU in a bake: the last landing's offscreen record
  says `wait_ms` 0.09 (the poll), `record_ms` 2.6, `submit_ms` 1.3, for a GPU time of 0.25 ms.
- Steps of the bake over the render class's 30 ms: the second builder measured 47 (most 36-75
  ms, the first 169 ms, which made the target). This build, on the same machine, 1 to 8 a bake
  over five runs (31-58 ms; one of 125 ms), and **not zero**. What is left, by the frame
  record's zones: the cell's submit (`sprite_bake.render`: recording the frame and the
  driver's submit), whose thread CPU time was 4 to 28 ms in those steps; the pose's uploads
  (`sprite_bake.pose`); once, the poll itself (`sprite_bake.poll`, 31-65 ms of wall time with
  1-2 ms of CPU). The machine was building other lanes' code throughout (12 to 16 compilers
  running): an engine left idle in a window beside the same load, with no bake at all, had 8 to
  29 frames over budget in 46 s, up to 1,035 ms, with under 2 ms of CPU. So most of what is
  left is not the bake's own work and cannot be told from the machine's on this run; the
  steps whose CPU time itself passed about 20 ms (the 448-pixel ID frames' submit) are the
  bake's, and are not explained.
- The target a bake renders on was destroyed at its end in the first build of this lane (30
  to 152 ms in the driver); it now goes back to the engine's cache of offscreen targets, and a
  bake takes a cached target of its size when no capture's view is in flight.
- `--strict-budget` (the engine ends with an error when any engine-thread stall passed its
  budget) failed the run, exit 1, naming 65 stalls, among them the bake's steps above.

## The view: how far down the camera looks

Every sheet is seen from one elevation, an explicit argument with named values: `view: "side"`
(level, 0 degrees: a platformer's side view, no top face seen), `"three_quarter"` (30 degrees
down: **the default**, the usual 2D three-quarter view, where a head overlaps the top of its
torso and a plinth shows its top face) or `"top"` (90, straight down); `elevation_deg` (0 to 90)
gives any other angle. The angle used is in the answer (`view`, `elevation_deg`) and in the
frame data. Every sheet the second builder baked (the shambler and the angel, and the wraith)
was at the default, `three_quarter`, 30 degrees: their frame data says so. A test projects a
box's top face through the very cameras the bake renders with: at 0 degrees its area is nil (it
is edge on), at 30 and 90 it is not.

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
compares the atlases). Across adapters this is not promised: GPUs shade and round differently.
Measured (2026-11-24, discrete RTX 4090 Laptop against integrated Intel RaptorLake-S, the same
binary): the alpha was identical in every pixel of four sheets (the ID buffer's coverage); the
colour differed in about a quarter of the visible pixels by at most 5 levels (mean 1); in pixel
art 0.5% of the walk's visible pixels (and 0.4% of the angel's) took another palette colour.
The plan (the scale, the pivot, the layout, the frame data) is computed on the CPU and is the
same wherever the same build of the engine runs (the frame data was byte-identical).

## Playback, as written

Shown in a headless engine (2026-11-24): the `e` strip of the shambler's walk spawned with
`world_spawn {shape: "sprite", sprite: <strips[2].sprite>}` and rendered by
`sight_frame {view_2d, time_s}` at nine times one clip frame apart (1/4.706 s): each showed the
next frame, the ninth (one clip duration, 1.7 s, later) was the first again exactly (the same
`visible_px` and box), a time inside a frame showed that frame, and a marker at the sprite's
position stayed on the same pixels in every frame. The 2D path's `sheet` + `anim` reader plays
the strips as written; nothing was added to it.

## Feet, outline, in place, shadow

- **Which feet are down** (`strips[].feet`, `strips[].feet_down`: per frame, the names of the
  feet down). The feet are the ones `anim_check` finds (a graph's `feet`, else the clip's
  footstep bones, else bones named `foot*`); a foot is down in a frame when it is within 2 cm
  (`FOOT_DOWN_M`) of its ground, its 15th-percentile height over the clip (as
  `measure::foot_contact` finds the ground). A model with none of those says so in the answer.
- **Outline** (`outline: true`, pixel art only): the palette's darkest colour (Oklab lightness)
  on every transparent pixel beside the silhouette, **never on a cell's outermost pixels**: the
  fit keeps two pixels clear round every posed vertex and the outline is drawn at most one
  pixel out, never in the outermost ring (`plan::outline`'s margin). Held by a test over a
  silhouette that reaches the second pixel from the edge (the ring stays clear, the outline is
  drawn inside), and on the GPU by the outlined pixel-art walk at eight directions: every
  cell's outermost pixels are transparent. Should a model still reach a cell's outermost pixel
  (the fit failing), an outlined bake is refused (`no_room_for_outline`, with a cell two
  pixels larger a side) and nothing is written.
- **In place** (`in_place: true`): each frame's root bone is put back over its rest place
  sideways (its height kept), so a 2D game that moves the sprite itself gets no wander. The
  wander is said either way (`root_drift`, with what was done with it).
- **Shadows.** A sprite carries no ground: only the model is drawn, so nothing receives a cast
  shadow. By default no shadow is baked in at all (a 2D game draws its own): the shadow map is
  off in every bake, whatever the rig says. Looked at with it on (the first build): no ground,
  so nothing under the model; the model shadowed itself (a wing on the body, an arm on the
  torso; 13% of the shambler's and 12% of the angel's visible pixels changed, by up to 102
  levels; the alpha not at all), plausibly and without visible artefacts, but that is a cast shadow baked in and the
  cascades are fitted for a perspective view, not this one; a self-shadow option is left for a
  game that asks. `shadow: "blob"` adds one cell after the strips (beginning a row):
  a soft, alpha-only ellipse (black, alpha 0.6 at its middle falling smoothly to 0; pixel art:
  one alpha, half, inside 85% of the ellipse), as wide as the model's footprint (its lowest
  fifth of height, from the first pose) and as tall as that seen from the sheet's elevation,
  on a pivot of its own (the cell's middle). The frame data's `shadow` gives its frame, rect,
  pivot, radii and the 2D path's sprite object that draws it, to be drawn under the model at
  the model's pivot.

## Where the files go (and what that changed for existing projects)

A bake's atlas must be a project file, so `sprites` joined the folders a project mirrors PNGs
from (`project::PNG_FOLDERS`: `textures`, `images`, `references`, `icons`, `sprites`; any
folder on the way counts, as for the others). What that changed for a 2D project that already
keeps hand-made art in a `sprites` folder (review 1, finding 6), decided 2026-11-24:

- **Its PNGs are mirrored from then on**: listed by `project_list`, read by `project_read`,
  put in the content store, counted against the binary store's room, journaled when changed,
  exactly as a PNG under `textures` has always been. Judged right: a sprite is art the project
  owns, and the engine's 2D path reads it from there. A project with more sprite art than the
  binary store holds would see the store's own `store_full` refusals, as one with too many
  textures does.
- **Its JSON is not touched.** The first build made every `*.json` directly in a `sprites`
  folder the frame-data kind, validated by the sheet reader: an Aseprite or TexturePacker
  sheet (`hero.json` beside `hero.png`) would have been mirrored as `sprite_sheet` and shown
  `invalid_sprite_sheet` problems, and `project_write` to it refused without `allow_errors`.
  Narrowed: the kind is `<name>.sheet.json` directly in a `sprites` folder only, the name
  `sprite_bake` writes; any other JSON there is not mirrored, as before the kind existed
  (tested: `a_projects_own_sprite_sheets_are_left_as_they_were`, with an Aseprite sheet).
- PROJECT_DATA.md's table has the `sprite_sheet` row and the `png` row's fifth folder.

## The multi-file commit

`ProjectCall::CommitFiles` writes several project files in one journal entry. It is held as the
single-file commit is: each file goes through `project_prepare` (its base version and hash, its
mount and kind, a linked folder, the store's room, the binary store's room, the disk budget, a
second spelling of an existing file); then the batch as a whole: two spellings of one file
refused (`check_batch`, as the taste tools' batches are), the store's and the binary store's
room and the disk budget for all the files together. All are written in one
`world.commit(.., Rule::Project, ..)` through `project_engine::files_commit`, so a claim on any
one refuses all of them and one `world_undo` takes all back. Each file's disk state is answered
for itself (`written`, `pending`, `held`). The regression table `every_write_path_holds_the_policy`
has two rows for it (both files written; the frame data claimed by another agent, refused with
the world as it was), and `sprite_bake_writes_and_undoes_its_two_files_together` holds one entry
and one undo. One tool call makes one commit (AGENTS rule 14).

## Not done

- A model's world anim assets (clips written with `anim_set`) are not baked: the build's own
  clips are. What it would take: the engine lends, with the build, the anim set the world's
  assets laid over it (`Library::anim_set_for_write(handle, world.anim_assets())`, as a graph
  write settles against it) when the caller asks for it, and the plan reads that set instead
  of `built.anims`. The renders need nothing new (they draw the plan's skinning matrices).
  It touches the model call's lending path in the engine, which other lanes share; left for a
  change of its own.
- Root motion beyond the drift and `in_place` (a clip's travel is `distance_m`).
- Vector 2D (`formae::draw` frame by frame) and a 2D skeleton: stage D2, not designed here.
