# What light a machine can run: `perf_light`

M6 part 4 (ROADMAP, "Form within an agent's reach"). The owner, on light (2026-11-16): "People will
have different requirements and AI agents will need to straight up tell them on what's possible and
what's not. This engine will only act as a guide in this case." And on machines (2026-11-14): the
agent sets the variables for the machine it is on; the engine's part is to report what it measured
where it runs.

So the engine has **no quality preset to pick for a machine it has never seen**, and no table of
what a named GPU can do. Light in Specula is the world's own settings (`world_env`: the sun's
shadows, ambient occlusion, bloom, anti-aliasing, fog, the sky, grading), the baked GI's tier
(`gi_settings`) and the point lights in view. What that costs is measured **where the engine runs,
for the view that is asked about, at the size it will be shown**: one tool, `perf_light`.

## What it does

`perf_light {width, height, camera, camera_of | eye, target, fov_y_deg | ortho | view_2d, include_ui, frames, budget_ms}` (every
argument optional; the view is `sight_frame`'s, by the same parser, `include_ui` with it):

1. Draws the view offscreen **as the world has it**. That first render says what the view pays
   for: which of the world's features are on, whether baked GI was drawn, how many point lights
   the frame was given.
2. For each feature that is on, draws the view **with that one feature left out**, and once
   more with all of them out (when more than one is on).
3. Does that in **alternating rounds**, `frames` of them (5 to 16, 9 unless given), after one
   round that is drawn and not counted. Every round draws each variant once. Rounds go in
   pairs, and **the second of a pair draws the first's order backwards**, so that the frames
   of one round are near each other in time and a machine whose times drift moves both
   orders alike. **The order changes from pair to pair**, the view as it is with it, by a
   fixed sequence (`light_noise::variant_at`: the same report draws in the same order every
   time, on every machine), so that a load that keeps step with the rounds does not fall on
   the same variants every round. (The first round draws the view as it is first: its first
   render says which variants there are.)
4. Takes each render's GPU time from **the adapter's own timestamps** (first pass start to last
   pass end; the ID buffer is not drawn), and answers the median of each variant. **A feature's
   cost is read from pairs**: the view as it is less the view without it, frame by frame of the
   same round, and the middle of those differences (`light_noise::cost`).

It changes nothing in the world and writes nothing: a feature is left out of the measuring render
alone (`light_tools::strip`), never out of the world's settings. It is a step job: a render is
submitted in one step and landed in a later one, between frames, so the GPU's work never holds
the engine thread ([WORKERS.md](WORKERS.md)); a step that finds its render still on the GPU
answers `Step::Wait` and is looked at again a pass later, never spinning on the fence; a view carries its own timings from the moment it
settles (`Rendered::gpu`), because another offscreen render would have replaced the GPU's last
record before the job asked for it. **A measuring render lands its timings alone**: its image is
not copied back, either into a buffer the host can read (the GPU's copy is left out of its
submission: `Gpu::render_async_timings`, the same render with that one copy left out) or by the
host; its subjects are not projected, and it takes no place in the flash watch. (Its colour
image is still drawn: the passes that make it are what is measured.)

Before its first render it **waits for what the view is still loading** (models building,
streamed cells, sprite, tileset and effect textures, terrain) **the way `sight_frame` waits, by
the same code** (`Engine::views_waiting`): built models uploaded and their readiness refreshed
first (`Engine::capture_sync`), the view asked about **freshly once** (texture files in view
checked for changes now) and after that only looked at. It waits for up to ten seconds, and not
when the call's time would not hold a whole wait; then it measures the view as it is and says so
in `warnings`. `waited` says why the wait ended: `not_needed`, `ready`, `limit` or `call_time`.

**A job whose call's time is nearly out answers with the whole rounds it has**, at whatever step
it finds that out (three seconds before the end): `partial` says how many rounds it counted of
how many were asked, and with fewer than five it gives no advice for a budget and says why. One
whose time runs out before a single round is counted answers `measured_nothing`, with how many
renders it made and how long it took: never a bare timeout where it could have known. (The
engine's own deadline, which answers `timed_out`, is still there behind it: a render that is in
flight when the time runs out is waited for, and if the call's whole time goes in the wait, that
is the answer.)

## What it answers

| Field | What |
|---|---|
| `adapter`, `size`, `camera` | which GPU, at what size, through which view |
| `budget_ms`, `budget_from`, `holds_budget` | the GPU time a frame may take: the call's, or this adapter's `gpu_ms` from `perf.toml`; and whether the view as it is holds it |
| `as_it_is` | the view as the world has it: `gpu_ms` (the median), `min_ms`, `max_ms`, `frames` |
| `without_all` | every feature that is on left out: what is left is the scene's own drawing |
| `features` | one row for each of `shadows`, `ssao`, `bloom`, `fxaa`, `fog`, `sky`, `grading`, `gi`, `point_lights`, the costliest first: `on`; and when it is on, `cost_ms` (the middle of the differences between the view as it is and the view without it, frame by frame of the same round), `share` of the frame, `without_ms` (that variant's median), `noise_ms`, `told_from_noise`, and `to_change` (the call that switches it off or turns it down) |
| `passes` | the view's dearest passes as it is, by name (`shadow`, `scene`, `ssao`, `post_bloom`, ...) |
| `to_hold_budget` | only when the budget is not held. `advised: true`: which features to `give_up`, costliest first, the `estimate_ms` that leaves, whether that is `enough`, and a `note`. `advised: false`: no advice, `why` (`too_few_frames`: fewer than five rounds counted; `busy`: the frames were too uneven to advise on) and a `note` |
| `load` | whether the machine was `steady` while it was measured: how many of all the `frames` were `late_frames` (more than a twentieth over the mean of their own variant's quickest third, and never of fewer than its quickest two: at five frames, its quickest two), and `what` that means |
| `waited` | why the wait for what the view was loading ended: `not_needed`, `ready`, `limit` (ten seconds) or `call_time` |
| `partial` | only when the call's time ran short: `rounds_counted`, `rounds_asked`, `why` |
| `settings` | what the features are set to (shadow resolution and cascades, ssao samples, bloom levels, the GI's tier, the lights in view) |
| `engine_light` | what the engine's light `has` and `has_not`, whatever the machine |
| `measured`, `renders`, `took_ms`, `warnings` | how it was measured, how many renders it took and how long, and anything to know: what `sight_frame` would have warned of in the view as it is (a picture that failed to load, models not drawn, lights left out of the frame, the camera's), GI that failed to bind, things still loading, rounds cut short |

**What is and is not said:**

- **A cost inside the noise is not a cost.** A cost is read from pairs, the view as it is less
  the view without the feature in the same round, and is told from the noise only when the
  middle of those differences is more than some times their own spread (the middle of their
  distances from it; five times for five pairs, four for six or seven, 3.2 for eight to eleven,
  2.5 from twelve), never less than a two-hundredth of the frame, **and** nearly every pair says
  the same (every pair of five, all but one to eleven, all but three from twelve). A cost
  smaller than that has `told_from_noise: false`, and is never named in `give_up`. Load that
  comes and goes (another program compiling) moves both frames of a pair alike far more often
  than it moves two separate medians alike, drawing every second round backwards takes a
  slow drift out of the pairs, and an order that changes from pair to pair keeps a load in
  step with the rounds from falling on the same variants. What that holds to, by simulation,
  is in the tables under "What was mended, third round" (the false rates there are for one
  feature; a whole report's are beside them). (The least frame to the most was tried first: one frame of nine
  0.3 ms late hid every cost under 0.3 ms. Then two separate medians, each with its own scatter:
  it called a costless feature a cost up to one time in ten on a machine busy by turns.)
- **Frames too uneven are said, not advised on.** When a fifth or more of all the frames are
  late (above), `load.steady` is false and `to_hold_budget` gives no advice: a feature that
  costs nothing named as the one to give up is the worst thing this tool could say. The costs
  are still answered, each with whether it is told. `load.what` says what that may be (another
  program on the GPU, a clock that moved, or the machine's own unevenness from frame to frame)
  and that the frames alone do not say which: a machine whose frames scatter by 3% of the frame
  with no load at all is called not steady in most reports, though the rule is sound there
  (the orders' table below). More frames do not change that (the share of late frames is the
  machine's, not the run's).
- **A feature that is off has no cost made up for it.** It is listed as off. To know what
  turning it on would cost, turn it on and ask again.
- **`to_hold_budget` is an estimate**: costs measured apart do not add up exactly (shadows cost
  more with more lights; fxaa costs by the pixels). It names the costliest features that are
  told from the noise until the estimate holds, passing over one that is not told. It never estimates under what was measured
  with everything out, it says so when even that is over the budget (**light is then not what
  keeps the view over**: draw less, or at a smaller size, or allow more time), and its note says
  to make the change and measure again.
- **The numbers are this adapter's, at this size, for this view.** A report from the developer's
  desktop says nothing of a player's laptop. Measure where the game will run; the low end the
  engine holds itself to is an integrated GPU ([perf/README.md](../../perf/README.md)).
- **It measures the GPU.** What the CPU spends on a frame (culling, the draw lists, the game) is
  `perf_frame`'s and `perf_zones`' to say, and a window's frame also pays for presenting.
  (`perf_frame`'s `offscreen` record is the GPU's last offscreen render: after a `perf_light` that
  is one of its measuring renders, drawn without its light. Take a `sight_frame` before reading it.)
- **Grading is on when it draws**: enabled, and with a value that is not neutral or a LUT. A
  feature that is on in the settings and draws nothing is listed as off.

## What the engine's light has, and has not

Said with every report (`engine_light`), so that nobody promises a game what the engine does not
have:

- **Has:** one sun or moon with cascaded shadow maps (1 to 4 cascades); up to 16 point lights a
  frame, which cast no shadows; ambient occlusion from the depth buffer; baked global
  illumination (irradiance probe volumes baked on the CPU, [GI.md](GI.md)) at tier off, low or
  high; bloom, anti-aliasing (FXAA), height fog, an analytic sky, colour grading with LUTs;
  glossy surfaces that reflect the sky and the ambient light, dimmed by the baked GI.
- **Has not:** ray tracing on the GPU (the engine never uses ray-tracing hardware, on any
  adapter: the owner's rule, vendor-neutral always); bounce light that follows a moving light or
  object (the GI is baked, and is baked again when the scene changes); reflections of the scene
  itself; shadows from point lights.

The ladder of light that would add to this (probes refreshed through distance fields,
screen-space traces: [research/FORM_AND_LIGHT.md](../research/FORM_AND_LIGHT.md)) is after M6.

## What it said on the development laptop (2026-11-19)

Sepulcra's own scene (map 1 as a game begins: shadows, ambient occlusion, bloom, FXAA, fog, the
sky and grading on; no baked GI drawn and no point light in view), headless, nine frames a variant
(90 renders, under a second in all), with the noise rule of the first round (two separate medians;
the rule now is in "What was mended, second round", its order of rounds in "third round"). These are this laptop's numbers and nobody else's.

| | Integrated (Intel Raptor Lake), 1280x720 | the same, 1920x1080 |
|---|---|---|
| The view as it is | 2.58 ms | 5.59 ms |
| With all seven out | 0.23 ms | 0.40 ms |
| bloom | 0.47 ms (18%) | 1.19 ms (21%) |
| the sky | 0.42 ms (16%) | 1.08 ms (19%) |
| ambient occlusion | 0.39 ms (15%) | 0.91 ms (16%) |
| grading | 0.28 ms (11%) | 0.55 ms (10%) |
| the sun's shadows | 0.24 ms (9%) | 0.38 ms (7%) |
| FXAA | 0.21 ms (8%) | 0.49 ms (9%) |
| fog | 0.13 ms (5%) | 0.32 ms (6%) |

Every cost there was told from the noise (0.03 ms at 720p, 0.14 at 1080p), and the seven add up to
within a tenth of what leaving all of them out saves. Nine tenths of this scene's GPU time on the
integrated GPU is its light and post, not its drawing; it holds a 60 Hz frame five times over at
720p and three times at 1080p. On the discrete GPU of the same laptop (an RTX 4090) the same view
took 0.18 ms at 720p and 0.32 at 1080p, bloom the dearest at a quarter.

**And it said when it could not tell.** Taken again from a debug build, whose frames reach the GPU
further apart, the discrete adapter's times wandered between 0.44 and 0.96 ms from frame to frame
(a GPU that idles between frames clocks itself down), a feature's cost came out negative, and the
report marked every cost on that adapter as not told from the noise, where the integrated GPU's
answers were the same as before to a hundredth of a millisecond. That is the answer wanted there:
no number to trust, said plainly.

## Bounds

Every argument has its range in the schema (the view's are `sight_frame`'s: 64 to 2048 pixels a
side; `frames` 5 to 16; `budget_ms` 0.1 to 1000). The work is bounded by them: at most eleven
variants (the view as it is, nine features, all out) by seventeen rounds, 187 renders, one a
step, and by the call's own time (above). The answer is a few kilobytes whatever the world. Read-only (`authority::access`), in the
`perf` family, named in the kit `find_slow_frame`.

## Tests

- Without a GPU: the arguments' bounds; a frame with its light taken out has none of it left and
  one feature out leaves the others alone; the report's numbers from measurements given to it
  (costs, their order, what is told from the noise, what to give up for a budget, that a budget
  under the scene's own cost is said to be not light's doing, and that too few rounds or a busy
  machine give no advice and say why); the noise rule on pairs by hand
  (`light_noise::a_cost_is_read_from_pairs_of_the_same_round`) and on fixed simulated sequences,
  where it holds and the rule before it did not
  (`the_noise_rule_holds_when_the_machine_is_busy_by_turns`, which also holds a load in step
  with the rounds: a third of costless features told in one order every pair, under 8% in the
  order now); the line for a late frame by hand (6% over is late, 4% is not); the rounds' order
  (`the_rounds_order_changes_from_pair_to_pair_and_mirrors_within_one`: each round draws every
  variant once, a pair's second round is its first backwards, the order and the view as it is
  move from pair to pair, two pairs' orders pinned); and the tables under "What was mended",
  recomputed by ignored tests (`the_noise_table_in_light_md`, `the_orders_table_in_light_md`,
  run with `--ignored`).
- With one (skips cleanly without): a small world with shadows, ambient occlusion, bloom and
  FXAA on is measured; the report has those four on and the other five off, each measured alone
  and all together, in the rounds asked for and one more; and **the world is as it was** (the
  same revision, the same settings), **no measuring render compiled a pipeline**, and **no
  measuring render's image was copied back** (`Gpu::readbacks`: the GPU's copies and the host's
  reads, both unchanged). What each costs is the machine's to say: the test holds what was drawn
  and counted, never a time. Two jobs of two views stepped in turn both finish with their own
  timings; a job whose time runs out part-way through its fourth round answers with three whole
  rounds and `partial`; one whose time is out at its first step answers `measured_nothing`.
  Every job in these tests is stepped by `step_once`, which steps again at once after a render
  is submitted, as a pass of the engine would, and fails when a step finds its render still on
  the GPU and answers anything but `Step::Wait`.
- With one, the wait (`a_view_whose_sprite_is_loading_waits_for_it_and_no_longer`): a 2D view
  with a sprite nobody has loaded waits until the picture is in (`waited: ready`, no warning),
  asking freshly once (`TwoDGpu::fresh_checks`); a view that is ready does not wait; a view still
  loading when its wait runs out is measured and says so (`waited: limit`); a call whose time
  would not hold a whole wait measures at once and says `call_time` (by the job's state: its
  deadline is under a whole wait at its first step); and a picture that failed to load is in
  `warnings` as `sight_frame` says it.

## Its first review

A model that did not write it (2026-11-19) said **not to merge it**, with two major findings,
both mended, each a case of a test. **`to_hold_budget` stopped at the first feature it could not
tell from the noise**, so a dear and noisy feature hid a cheaper one that would have held the
budget, and the note said nothing would. **Every measuring render copied its whole image back to
the CPU on the engine thread and threw it away**: 19 of a step's 23 ms at 2048 pixels a side on
the integrated GPU, 44 stalls in one call of 16 frames, and a call at the schema's limit timed out
once with nothing to show. Smaller, mended too: the noise rule called a costless feature a cost
one time in twenty at five frames and one in six at three (the least is five now, nine unless
given, and the rule widens for fewer frames); grading was listed as on when it drew nothing; a
view still loading was measured without waiting or saying; `include_ui` could not be asked for;
and `perf_frame`'s offscreen record after a report is a measuring render's, which this page now
says. It found sound: that no measuring render leaks into another view (two reports and nine
`sight_frame` calls at once: every capture identical to one taken later), that the timings are
each render's own, the job's handling of owners, closed sessions and deadlines, and every claim
of what the engine's light has and has not, against the renderer.

## Its second review, which stopped early

A second model was given the mends (2026-11-20). **It stopped before it had run the engine**:
to weigh another piece it was given with this one it needed a temporary counter in a tracked
source file of its own worktree, the permission for that was refused, and its brief said to
stop there. So what follows was found by reading the code and by a simulation of the noise
rule, and **nothing of it has been seen in a running engine yet**. Its provisional verdict
is **not to merge the mends**, on the first finding.

1. **Major, by reading: the wait for a view that is still loading can keep itself
   waiting.** Until its first render the job asks at every step whether the view is ready,
   and asks freshly each time, where `sight_frame` asks freshly once and then only looks.
   In 2D a fresh question while a check is in flight orders one more check, and an ordered
   check counts as loading: a view with a sprite not yet loaded would then wait the whole
   ten seconds and be measured with a warning that things were still loading. Each such
   step also reads file times on the engine thread, and for effects' textures scans every
   one of them (AGENTS rule 15). The wait also leaves out the upload of built models that
   `sight_frame` does first.
2. *Minor:* an answer cut short with fewer than five rounds still gives advice for the
   budget, with a note that giving up every feature told from the noise is not enough; the
   reason is that too few frames were counted. Nothing but a warning's words marks the
   answer as partial. A call whose time runs out in its first two rounds answers nothing.
3. *Minor:* "no image is read back" is true of the processor's copy and not of the
   graphics card's: each measuring render still copies its image into a buffer the host can
   read (16 MB at 2048 pixels a side), after the last timed pass, so outside the times.
4. *Minor:* the noise rule holds for noise that is spread evenly and not for a machine
   that is busy by turns. Simulated, 100,000 pairs a cell: a feature that costs nothing is
   called a cost 0.5% to 0.6% of the time at five, seven and nine frames, as the source
   says; with a third of the frames late by a fifth it is 1.5% at nine frames and 3.0% at
   five; with half of them late, 9.3% at nine. A real cost of 2% of the frame is told 41%
   of the time at nine frames and 1% noise, one of 5% nearly always.

By reading it found the first review's mends in place at their causes (the walk for a
budget, the copy skipped only for the report's own renders, the frame counts, grading, the
interface's `include_ui`, the offscreen record's sentence), each but the skipped copy with
a test. **Not checked:** any run on either adapter, this page sentence by sentence, and
cancelling. **None of this is mended yet.** (As found; mended since: the next section.)

## What was mended, second round

A builder that wrote none of it (2026-11-20) took each of the four findings above, saw it first
where it could, found its cause, and mended it there. On a busy machine (other agents compiling).

1. **The wait for a view still loading.** *Seen*, in a running engine (headless, debug build):
   a sprite whose picture had been written just before the call kept `perf_light` waiting
   1.9 to 2.0 seconds on either adapter, where `sight_frame` of the same view took a tenth of
   a second and the same view with an older file took 6 to 18 ms. **Not the whole ten
   seconds the review expected**: the wait that kept itself going let go at about the time the
   file became older than the two seconds within which a file's time is not trusted (and its
   contents are read again). Only 2D textures (sprites, tilesets) kept themselves waiting;
   effects' textures were rescanned, every file's time read on the engine thread, at every
   step, and did not loop; models, streamed cells, terrain, shapes and text take no fresh
   question. *Cause*: the job had a copy of `sight_frame`'s wait that asked freshly at every
   step and left out its first step (built models uploaded, readiness refreshed). *Mend*:
   **one way to wait, shared**: `Engine::capture_sync` and `Engine::views_waiting`, which
   `sight_frame`, its parked captures and `perf_light` all call; the job asks freshly once, at
   its first look, then only looks. (`sight_frame` itself changed in one way: at the request it
   asked streaming, then the models, and asked nothing more while a model was building, so
   with a model building its texture files were not checked freshly; every part is asked
   now.) `waited` says why the wait ended. *Tests*: a
   sprite view waits until the picture is in and its wait ends `ready`, with one fresh check
   (the count, `TwoDGpu::fresh_checks`, not a time); a ready view does not wait; a view still
   loading when the wait runs out ends `limit`, with the warning. *What remains*: the one fresh
   check at the request still reads the files' times on the engine thread, as `sight_frame`'s
   always has (up to 256 sprite and tileset files, and every effect texture the engine holds);
   moving it to the loaders would change `sight_frame` too and was not done.
2. **A cut-short answer.** *Cause*: the deadline was looked at only from the third round, at
   a round's start, so a call whose time ran out sooner (a long wait, a slow adapter) was
   dropped by the engine as `timed_out`; and the walk for a budget ran on too few frames.
   *Mend*: the job looks at every step; it answers with whole rounds and `partial`
   (`rounds_counted`, `rounds_asked`, `why`); with fewer than five rounds `to_hold_budget`
   has `advised: false`, `why: too_few_frames`; with none it answers `measured_nothing` with
   its renders and time. *Tests*: by state, not time: a job whose deadline is moved to now
   part-way through its fourth round answers three rounds and `partial`; one out of time at its
   first step answers `measured_nothing`; the report from two rounds gives no advice and says
   why. *What remains*: a render in flight is waited for; if the call's last three seconds go
   in one render, the engine's own `timed_out` is still the answer.
3. **The copy of the image.** *Mend*: a measuring render no longer copies its image into the
   host's buffer: `Gpu::render_async_timings` is the same render with only that copy left out
   of its submission (the render graph, pipelines and shaders are untouched; a later
   `read_rgba8` of the target makes the copy itself). *Test*: `Gpu::readbacks` counts the
   GPU's copies and the host's reads; a whole report leaves both unchanged, so putting back
   either copy fails it. The colour image is still drawn: it is what is measured.
4. **The noise rule on a machine busy by turns.** Seven rules were tried in a scratch
   simulation (separate medians as before; pairs of a round, their median and spread, with and
   without a sign test; the means of each variant's quickest third, with its own spread and
   with a spread pooled over the variants), then the chosen one and the one before it in the
   repository's own (`light_noise`, deterministic). The quickest third told costs best under
   load and called costless features costs 4% to 30% of the time; the pairs held. **Chosen**:
   the median of the pairs' differences past some times their spread, with nearly every pair
   agreeing, rounds drawn every second one backwards (forwards only, a slow drift made a third
   of costless features costs), and a busy machine said: a fifth or more of the frames over a
   twentieth late gives `load.steady: false` and no advice.

   The table (`the_noise_table_in_light_md`; ten variants, eight costless features and one
   that costs, drawn furthest from the view as it is; 3,000 reports a cell; a 5 ms frame). Each
   cell: the share of costless features called a cost, % (in brackets the share of reports
   that found the machine busy, %), then how often a cost of 1%, 2%, 5% and 10% of the frame is
   told, %.

   The rule before (two separate medians):

   | Noise | 5 frames | 7 frames | 9 frames | 16 frames |
   |---|---|---|---|---|
   | even, 1% | 0.6 (0.0) 2.8/10.6/64.6/99.1 | 0.7 (0.0) 4.6/21.8/94.1/100.0 | 0.6 (0.0) 7.0/38.6/99.4/100.0 | 0.0 (0.0) 2.6/37.7/100.0/100.0 |
   | skewed | 0.5 (0.0) 2.8/9.0/53.0/93.1 | 0.5 (0.0) 2.5/18.3/81.5/99.5 | 0.4 (0.0) 6.2/32.4/95.2/100.0 | 0.0 (0.0) 1.1/25.8/98.6/100.0 |
   | a third late by a fifth | 3.9 (98.9) 17.1/41.7/66.4/66.0 | 3.0 (99.7) 28.9/56.8/70.4/71.9 | 2.7 (99.8) 40.1/70.7/75.9/77.0 | 0.2 (100.0) 39.7/74.5/76.6/76.7 |
   | half late by a fifth | 8.6 (100.0) 11.6/21.8/33.1/34.4 | 7.7 (100.0) 13.1/25.2/33.6/33.7 | 9.8 (100.0) 15.9/30.7/34.3/34.6 | 1.9 (100.0) 5.3/17.8/18.1/21.1 |
   | bursts, a third of the time, ~8 renders | 7.9 (73.5) 28.4/53.0/70.3/72.0 | 7.7 (80.1) 41.2/65.9/74.9/77.8 | 6.0 (81.6) 48.3/71.6/76.6/76.1 | 0.9 (88.1) 42.0/66.7/70.6/69.9 |
   | bursts, half the time, ~8 renders | 12.0 (95.6) 22.6/35.0/48.8/50.4 | 12.1 (98.0) 26.2/40.5/48.4/50.6 | 11.0 (98.9) 26.5/41.6/45.6/46.8 | 3.3 (99.9) 12.2/23.9/27.3/29.6 |
   | bursts, half the time, ~40 renders | 6.7 (56.7) 42.0/72.8/89.3/89.9 | 6.9 (68.8) 53.1/80.5/89.5/89.7 | 5.9 (75.1) 56.4/78.7/83.3/83.4 | 0.8 (87.9) 35.4/52.1/57.8/60.2 |
   | a slow drift of a tenth | 0.0 (70.3) 0.0/0.0/0.0/0.0 | 0.0 (100.0) 0.0/0.0/0.0/55.4 | 0.0 (100.0) 0.0/0.0/1.8/100.0 | 0.0 (100.0) 0.0/0.0/0.0/100.0 |

   The rule chosen (pairs, rounds turning back, busy said), in this round's order (every pair
   of rounds in one order; the third round changed the order, and its table is under "What was
   mended, third round"):

   | Noise | 5 frames | 7 frames | 9 frames | 16 frames |
   |---|---|---|---|---|
   | even, 1% | 0.9 (0.0) 5.1/20.5/70.1/98.4 | 0.8 (0.0) 4.9/22.1/83.8/99.8 | 0.4 (0.0) 5.3/27.4/94.4/100.0 | 0.0 (0.0) 3.8/39.6/99.9/100.0 |
   | skewed | 0.6 (0.0) 3.6/12.2/52.1/89.9 | 0.5 (0.0) 3.7/15.5/68.0/97.5 | 0.2 (0.0) 3.2/18.9/81.9/99.5 | 0.0 (0.0) 2.0/24.2/97.3/100.0 |
   | a third late by a fifth | 0.5 (98.9) 5.9/12.6/18.2/20.1 | 0.4 (99.7) 10.4/20.1/27.5/31.6 | 0.1 (99.8) 8.1/16.0/18.7/24.7 | 0.0 (100.0) 9.1/17.1/20.3/28.7 |
   | half late by a fifth | 1.0 (100.0) 2.3/3.8/5.8/8.8 | 0.7 (100.0) 2.1/4.2/8.1/11.8 | 0.3 (100.0) 1.3/2.2/4.6/7.6 | 0.0 (100.0) 0.5/0.8/2.7/6.4 |
   | bursts, a third of the time, ~8 renders | 0.5 (73.5) 16.2/27.7/35.5/37.4 | 0.3 (80.1) 25.2/44.4/50.1/54.5 | 0.1 (81.6) 25.7/39.2/41.9/44.6 | 0.0 (88.1) 39.8/52.9/53.5/55.3 |
   | bursts, half the time, ~8 renders | 1.0 (95.6) 8.3/13.6/20.7/23.5 | 0.8 (98.0) 11.4/23.1/29.1/33.5 | 0.3 (98.9) 9.0/17.8/20.8/24.6 | 0.0 (99.9) 10.2/18.2/21.4/27.7 |
   | bursts, half the time, ~40 renders | 0.1 (56.7) 37.4/57.7/65.6/65.3 | 0.0 (68.8) 56.0/83.7/88.0/88.3 | 0.0 (75.1) 66.2/80.7/81.5/81.0 | 0.0 (87.9) 90.8/94.4/94.8/95.0 |
   | a slow drift of a tenth | 0.0 (70.3) 0.7/21.1/76.8/99.9 | 0.0 (100.0) 3.4/19.0/93.1/100.0 | 0.0 (100.0) 3.5/33.4/100.0/100.0 | 0.0 (100.0) 30.7/99.0/100.0/100.0 |

   **No rule told a cost of a twentieth of the frame reliably under load that comes and goes
   in short bursts while keeping costless features under 2%**: with half the frames late in
   bursts of about eight renders the chosen rule tells a 5% cost a fifth to a third of the
   time. It keeps the false rate at or under 1% a feature in every case simulated (the rule
   before: up to 12%), and on such a machine it says so (`load.steady: false`) and gives no advice for a budget: in
   96% to 100% of reports with half the frames late, in 74% to 88% with a third of them late in
   short bursts, and in 57% to 88% with long bursts (where the pairs read costs best). Where
   it does not say so it still advises from costs whose false rate is in the table. On even noise it calls a
   costless feature a cost 0.9% of the time at five frames (0.6% before) and tells a 5% cost a
   little less often at seven and nine frames (84% and 94%, from 94% and 99%). A slow drift is
   also found "busy" (the frames' times move by more than a twentieth), though the pairs read
   it well: no advice is given then either.

*Before and after, the same scene* (five cubes; shadows, ambient occlusion, bloom, FXAA, fog
and the sky on; 1280x720; nine frames; budget 0.2 ms; debug build, headless, busy machine):

| | Integrated, before | Integrated, after | Discrete, before | Discrete, after |
|---|---|---|---|---|
| As it is | 1.96 ms | 1.93 ms | 0.83 ms | 0.83 ms |
| bloom, sky, ssao | 0.50, 0.45, 0.37 | 0.50, 0.45, 0.37 | 0.29, 0.12, 0.12 | 0.30, 0.11, 0.12 |
| fxaa, shadows, fog | 0.20, 0.15, 0.13 | 0.19, 0.14, 0.12 | 0.10, 0.08, 0.02 | 0.10, 0.07, 0.02 |
| Told from the noise | all six | all six (noise 0.01 to 0.02 ms) | all six | all six (fog: 0.017 ms, noise 0.016) |
| Advice for 0.2 ms | give up all six | the same; `load.steady` | give up five | the same; `load.steady` |
| A sprite written just before | 1.9 to 2.0 s, no warning | 0.11 s, `waited: ready` | 2.0 s | 0.19 s, `waited: ready` |

(Fog on the discrete adapter is told by a thousandth of a millisecond: a cost that small is told
or not by chance. Every number here is one call on a busy machine.)

*Faults seeded back, each once, and what caught it*: a fresh question at every step (the
sprite test, by its count of fresh checks); the rule before (separate medians, no pairs, no
sign test: both `light_noise` tests); every round forwards (the order's own check and the
drift case); advice from too few rounds, and on a busy machine (the report test); the GPU's
copy put back, and the host's read put back (the GPU test, by `Gpu::readbacks`); the deadline
looked at only from the third round (the GPU test's `measured_nothing`). Every one was caught.

*Cancelling*, read and not changed: a step job is dropped when its session closes
(`session_closed`) and at its call's deadline (`timed_out`); there is no owner's cancel for a
step job. A job dropped with a render in flight leaves that render to be landed and thrown
away before the next offscreen render (`land_capture_flight`), as before.

## Its third review

A third model (2026-11-20, of the second round's mends, with the engine running on both
adapters on a busy machine) said **not to merge, for one test that is missing**, and that
with it and the smaller ones below it would say merge. In every run the tool behaved as
this page says: it told every real cost and gave the same advice run after run; beside a
second engine drawing large reports on the same adapter, four reports of eight said the
machine was not steady and gave no advice, the other four advised rightly, and no report
advised from a busy run; drawing every second round backwards changed a frame's time by
under a hundredth of the frame; `sight_frame` before and after six reports gave the same
bytes on both adapters, with no validation message. Its own simulation, written from this
page and not from the code, reproduces the table above on even noise and on bursts.

1. **A stated rule no test holds: the wait gives way when the call's time would not hold a
   whole wait.** With that cut taken out every test passes; a call with under thirteen
   seconds left and a view still loading would then answer `measured_nothing` where it
   should measure.
2. *Minor:* the line for a late frame (a twentieth over its variant's quickest third) is
   held only by the table's ignored test.
3. *Minor, and not in the table above:* **load that keeps step with the rounds and stays
   under the line for a late frame breaks the rule while the machine is called steady.**
   Simulated: with 2% to 4% more over half of each round, 30% to 36% of features that cost
   nothing are told as costs; with the load's period a twentieth off a round's length, 14%
   to 17%. No real source of such a load was found, and none showed on this machine.
4. *Minor:* **a machine that is merely uneven is called busy**: with even noise of 3% of
   the frame, most reports give no advice though the rule is sound there, and the words
   name another program or a clock that moved, which is not what happened.
5. *Minor:* the false rate above is for one feature; for a whole report on even noise of 1%
   at five frames, 3.9% of steady reports tell some costless feature a cost.
6. *Minor:* "the view as it is first" is not true of a round drawn backwards; one sentence
   about `sight_frame`'s old wait is slightly off.
7. *Minor, and older than this round:* a 2D picture that failed to load is warned of by
   `sight_frame` and not by `perf_light`.

Not settled: whether any real load keeps step with the rounds; a release build (every run
was a debug build). Not checked: an engine with a window; a session closing while the job
waits; models still building during a report. Seen and older than this round: while a
render is still on the graphics card the job asks to be stepped again within its step's
credit, up to 12 ms a pump, where it could wait. **None of this is mended yet.** (As found;
mended since: the next section.)

## What was mended, third round

A builder that wrote none of it (2026-11-20) took the third review's findings and the two
things it did not check. Every mend has a test that fails without it (the faults seeded back
are at the end). On a busy machine; nothing here rests on a time.

1. **The wait gives way to the call's time** (`waited: call_time`): *test*, in the sprite test,
   by the job's state: a picture nobody has loaded, the job's deadline five seconds away at its
   first step (under a whole wait, past the three-second margin): that step decides
   `call_time`, and with its deadline then moved far the job measures five rounds and says
   `call_time`. With the cut taken out the first step only waits, and the test fails.
2. **The line for a late frame**: a hand case in
   `a_cost_is_read_from_pairs_of_the_same_round`: two frames of ten 6% over the quick third are
   late and make the machine not steady, two 4% over are not. A line at 3% or 7% fails it.
3. **A load that keeps step with the rounds.** The lead's decision was to shuffle the variants
   between the view as it is and the last one, pair by pair, if the simulation said it helped
   and cost nothing elsewhere. **It did not help**: with the view as it is always at an end
   of a round, its place is half of every pair, and a load in step with the rounds still falls
   on it the same way every round (34% of costless features told at five frames went to 21%,
   and at sixteen frames rose from 30% to 46%). **What was taken instead**: from the second
   pair of rounds on, every place is shuffled, the view as it is too; within a pair the second
   round is still the first backwards; the first round draws the view as it is first, since
   its first render says which variants there are (`light_noise::variant_at`; a fixed
   splitmix64 sequence of the pair's number, the same on every machine and every run; two
   pairs' orders are pinned in a test).

   The table (`the_orders_table_in_light_md`; the rule now; ten variants; 3,000 reports a cell,
   the main table's seeds at 5, 9 and 16 frames, so its cells come again here). Each cell as the
   main table's, then `r`: of the reports found steady, the share in which some costless
   feature is told a cost more than nothing, % (`-`: none was steady). "serpentine" is the
   second round's order, "middle shuffled" the lead's, "all shuffled" the one taken.

   | Noise | Order | 5 frames | 9 frames | 16 frames |
   |---|---|---|---|---|
   | even, 1% | serpentine | 0.9 (0.0) 5.1/20.5/70.1/98.4 r 3.1 | 0.4 (0.0) 5.3/27.4/94.4/100.0 r 1.3 | 0.0 (0.0) 3.8/39.6/99.9/100.0 r 0.2 |
   | even, 1% | middle shuffled | 0.8 (0.0) 4.6/19.3/71.2/98.6 r 2.9 | 0.4 (0.0) 5.6/27.9/94.9/100.0 r 1.5 | 0.0 (0.0) 4.1/38.9/99.9/100.0 r 0.1 |
   | even, 1% | all shuffled | 0.9 (0.0) 4.8/18.8/69.6/98.3 r 3.4 | 0.4 (0.0) 5.2/27.6/94.9/100.0 r 1.5 | 0.1 (0.0) 3.7/38.4/99.9/100.0 r 0.3 |
   | skewed | serpentine | 0.6 (0.0) 3.6/12.2/52.1/89.9 r 1.9 | 0.2 (0.0) 3.2/18.9/81.9/99.5 r 0.5 | 0.0 (0.0) 2.0/24.2/97.3/100.0 r 0.1 |
   | skewed | middle shuffled | 0.6 (0.0) 3.5/12.0/52.5/89.4 r 2.1 | 0.1 (0.0) 3.1/18.5/82.1/99.7 r 0.7 | 0.0 (0.0) 1.9/25.0/97.4/100.0 r 0.0 |
   | skewed | all shuffled | 0.7 (0.0) 3.0/12.2/53.6/91.0 r 2.6 | 0.2 (0.0) 2.9/18.2/81.2/99.7 r 0.7 | 0.0 (0.0) 1.9/25.3/97.0/100.0 r 0.0 |
   | a third late by a fifth | serpentine | 0.5 (98.9) 5.9/12.6/18.2/20.1 r 0.0 | 0.1 (99.8) 8.1/16.0/18.7/24.7 r 0.0 | 0.0 (100.0) 9.1/17.1/20.3/28.7 r - |
   | a third late by a fifth | middle shuffled | 0.5 (98.9) 5.8/11.8/16.8/19.6 r 0.0 | 0.2 (99.8) 7.3/16.7/18.9/22.2 r 0.0 | 0.0 (100.0) 9.3/17.1/21.8/29.4 r - |
   | a third late by a fifth | all shuffled | 0.5 (98.9) 6.0/11.6/16.9/18.9 r 0.0 | 0.2 (99.8) 7.7/17.2/20.0/22.8 r 0.0 | 0.0 (100.0) 10.2/18.1/23.2/28.8 r - |
   | half late by a fifth | serpentine | 1.0 (100.0) 2.3/3.8/5.8/8.8 r - | 0.3 (100.0) 1.3/2.2/4.6/7.6 r - | 0.0 (100.0) 0.5/0.8/2.7/6.4 r - |
   | half late by a fifth | middle shuffled | 1.0 (100.0) 1.9/3.8/6.7/9.1 r - | 0.3 (100.0) 1.0/2.7/3.9/8.7 r - | 0.0 (100.0) 0.4/0.8/2.7/6.4 r - |
   | half late by a fifth | all shuffled | 0.7 (100.0) 2.1/3.2/5.3/8.6 r - | 0.3 (100.0) 1.3/2.2/4.5/8.6 r - | 0.0 (100.0) 0.3/0.9/2.6/7.2 r - |
   | bursts, a third of the time, ~8 renders | serpentine | 0.5 (73.5) 16.2/27.7/35.5/37.4 r 0.5 | 0.1 (81.6) 25.7/39.2/41.9/44.6 r 0.2 | 0.0 (88.1) 39.8/52.9/53.5/55.3 r 0.0 |
   | bursts, a third of the time, ~8 renders | middle shuffled | 0.4 (73.5) 23.6/38.1/44.9/46.1 r 0.5 | 0.0 (81.6) 38.2/53.7/56.9/60.0 r 0.2 | 0.0 (88.1) 70.4/81.1/81.3/81.7 r 0.0 |
   | bursts, a third of the time, ~8 renders | all shuffled | 0.1 (73.6) 39.5/59.2/61.6/63.1 r 0.0 | 0.0 (81.6) 61.4/77.8/78.2/79.9 r 0.0 | 0.0 (88.1) 90.1/94.7/95.2/94.7 r 0.0 |
   | bursts, half the time, ~8 renders | serpentine | 1.0 (95.6) 8.3/13.6/20.7/23.5 r 1.5 | 0.3 (98.9) 9.0/17.8/20.8/24.6 r 0.0 | 0.0 (99.9) 10.2/18.2/21.4/27.7 r 0.0 |
   | bursts, half the time, ~8 renders | middle shuffled | 0.8 (95.8) 11.8/20.8/29.6/32.4 r 0.0 | 0.1 (98.9) 18.2/31.3/35.4/39.0 r 0.0 | 0.0 (99.9) 40.7/53.9/56.5/62.1 r 0.0 |
   | bursts, half the time, ~8 renders | all shuffled | 0.3 (95.8) 26.7/42.3/50.1/49.5 r 0.0 | 0.0 (98.9) 43.8/62.5/62.6/65.6 r 0.0 | 0.0 (99.9) 75.4/85.4/85.0/86.7 r 0.0 |
   | bursts, half the time, ~40 renders | serpentine | 0.1 (56.7) 37.4/57.7/65.6/65.3 r 0.1 | 0.0 (75.1) 66.2/80.7/81.5/81.0 r 0.0 | 0.0 (87.9) 90.8/94.4/94.8/95.0 r 0.0 |
   | bursts, half the time, ~40 renders | middle shuffled | 0.0 (56.7) 44.1/66.6/73.2/72.3 r 0.1 | 0.0 (75.1) 77.1/88.2/88.9/88.3 r 0.0 | 0.0 (87.9) 96.9/98.6/99.0/98.8 r 0.0 |
   | bursts, half the time, ~40 renders | all shuffled | 0.0 (56.8) 56.6/81.7/85.5/85.1 r 0.1 | 0.0 (75.1) 86.2/95.9/96.4/96.2 r 0.0 | 0.0 (87.9) 99.4/99.8/99.9/99.8 r 0.0 |
   | a slow drift of a tenth | serpentine | 0.0 (70.3) 0.7/21.1/76.8/99.9 r 0.1 | 0.0 (100.0) 3.5/33.4/100.0/100.0 r - | 0.0 (100.0) 30.7/99.0/100.0/100.0 r - |
   | a slow drift of a tenth | middle shuffled | 0.0 (56.8) 0.3/5.7/56.7/100.0 r 0.2 | 0.0 (100.0) 9.0/64.6/100.0/100.0 r - | 0.0 (100.0) 67.8/100.0/100.0/100.0 r - |
   | a slow drift of a tenth | all shuffled | 0.1 (75.4) 14.7/53.3/99.3/100.0 r 0.8 | 0.0 (100.0) 41.7/94.6/100.0/100.0 r - | 0.0 (100.0) 83.7/100.0/100.0/100.0 r - |
   | +4% over half of each round | serpentine | 34.0 (0.0) 40.1/52.8/82.8/99.9 r 40.7 | 35.5 (0.0) 48.0/58.5/82.0/100.0 r 38.2 | 30.1 (0.0) 41.4/38.8/60.0/90.8 r 38.8 |
   | +4% over half of each round | middle shuffled | 21.3 (0.0) 15.2/24.2/70.4/79.3 r 38.6 | 35.1 (0.0) 40.2/40.9/49.3/80.7 r 37.6 | 46.1 (0.0) 40.4/39.9/59.3/100.0 r 38.7 |
   | +4% over half of each round | all shuffled | 3.2 (0.0) 11.0/18.2/40.7/39.3 r 11.0 | 4.2 (0.0) 15.2/21.1/39.2/42.1 r 16.9 | 2.5 (0.0) 28.9/30.1/47.2/97.8 r 9.4 |
   | +2% over half of each round | serpentine | 31.2 (0.0) 37.1/54.7/94.9/100.0 r 40.7 | 32.0 (0.0) 45.9/62.0/99.1/100.0 r 38.2 | 30.1 (0.0) 42.0/59.9/91.5/100.0 r 38.8 |
   | +2% over half of each round | middle shuffled | 14.9 (0.0) 12.0/25.7/76.9/94.9 r 36.0 | 29.8 (0.0) 32.7/39.9/81.9/100.0 r 37.0 | 39.2 (0.0) 39.3/40.6/100.0/100.0 r 38.7 |
   | +2% over half of each round | all shuffled | 2.5 (0.0) 11.0/26.2/41.1/89.5 r 9.4 | 3.8 (0.0) 15.2/35.8/51.0/100.0 r 15.7 | 1.1 (0.0) 28.9/39.2/97.6/100.0 r 4.4 |
   | +4%, a period of 0.95 rounds | serpentine | 21.2 (0.0) 20.0/33.2/77.3/79.4 r 31.4 | 14.4 (0.0) 34.0/54.2/100.0/100.0 r 24.4 | 0.0 (0.0) 71.3/86.7/100.0/100.0 r 0.0 |
   | +4%, a period of 0.95 rounds | middle shuffled | 14.7 (0.0) 16.0/26.7/87.9/100.0 r 29.9 | 11.0 (0.0) 18.8/20.1/29.1/45.8 r 22.4 | 0.0 (0.0) 0.0/0.0/22.3/99.8 r 0.0 |
   | +4%, a period of 0.95 rounds | all shuffled | 4.3 (0.0) 5.4/11.1/41.5/42.2 r 13.9 | 3.5 (0.0) 7.4/19.1/48.5/48.1 r 11.0 | 0.9 (0.0) 10.8/15.8/46.2/99.8 r 3.7 |
   | +4%, a period of 1.05 rounds | serpentine | 21.2 (0.0) 11.6/19.9/57.7/63.0 r 29.3 | 16.1 (0.0) 6.9/17.8/57.2/58.0 r 23.5 | 0.0 (0.0) 15.6/28.4/57.2/99.9 r 0.0 |
   | +4%, a period of 1.05 rounds | middle shuffled | 17.2 (0.0) 5.1/12.2/53.9/62.7 r 30.0 | 12.2 (0.0) 13.3/18.3/47.5/67.9 r 21.9 | 0.0 (0.0) 0.0/0.0/10.4/99.6 r 0.0 |
   | +4%, a period of 1.05 rounds | all shuffled | 1.8 (0.0) 10.6/19.4/70.4/81.5 r 6.7 | 1.5 (0.0) 10.2/22.4/60.7/63.6 r 5.6 | 0.0 (0.0) 12.6/21.8/60.8/100.0 r 0.0 |
   | +4%, a period of 1.2 rounds | serpentine | 0.0 (0.0) 0.0/0.0/0.0/0.0 r 0.0 | 0.0 (0.0) 0.0/0.0/0.0/1.0 r 0.0 | 0.0 (0.0) 0.0/0.0/0.0/96.5 r 0.0 |
   | +4%, a period of 1.2 rounds | middle shuffled | 0.9 (0.0) 0.0/0.0/42.9/49.3 r 2.2 | 0.8 (0.0) 0.0/0.0/0.0/1.4 r 3.2 | 0.7 (0.0) 3.4/8.5/16.6/96.5 r 2.7 |
   | +4%, a period of 1.2 rounds | all shuffled | 3.2 (0.0) 16.8/23.6/59.5/66.7 r 9.3 | 2.9 (0.0) 13.3/17.2/59.9/68.7 r 7.0 | 0.1 (0.0) 37.0/49.8/67.8/100.0 r 0.6 |
   | +4%, a period of 2 rounds | serpentine | 25.3 (0.0) 82.6/85.7/84.7/100.0 r 39.6 | 27.6 (0.0) 84.7/89.6/93.4/100.0 r 38.1 | 20.2 (0.0) 81.8/78.1/89.4/100.0 r 34.2 |
   | +4%, a period of 2 rounds | middle shuffled | 13.9 (0.0) 39.5/46.2/83.1/100.0 r 30.1 | 19.0 (0.0) 42.0/49.8/89.3/100.0 r 33.7 | 16.6 (0.0) 31.4/34.3/67.5/100.0 r 30.2 |
   | +4%, a period of 2 rounds | all shuffled | 2.9 (0.0) 50.8/68.1/88.8/100.0 r 6.9 | 0.0 (0.0) 50.8/58.9/71.4/69.9 r 0.0 | 0.0 (0.0) 65.6/70.7/82.7/100.0 r 0.0 |
   | even, 2% | serpentine | 0.9 (0.3) 2.1/5.0/28.7/71.1 r 3.3 | 0.4 (0.2) 1.4/4.8/41.6/94.2 r 1.3 | 0.0 (0.0) 0.3/3.7/63.3/100.0 r 0.2 |
   | even, 2% | middle shuffled | 0.9 (0.2) 1.5/4.9/27.7/72.1 r 3.2 | 0.4 (0.0) 1.0/4.8/43.5/95.2 r 1.5 | 0.0 (0.0) 0.5/4.4/62.7/100.0 r 0.1 |
   | even, 2% | all shuffled | 0.9 (0.1) 1.9/5.4/27.7/70.0 r 3.6 | 0.4 (0.1) 1.1/5.2/45.0/95.5 r 1.5 | 0.1 (0.0) 0.4/4.5/63.3/100.0 r 0.3 |
   | even, 3% | serpentine | 0.9 (58.0) 1.6/2.7/13.7/44.8 r 4.2 | 0.4 (91.6) 0.8/1.9/16.4/68.1 r 1.6 | 0.0 (99.0) 0.2/1.0/23.4/91.9 r 0.0 |
   | even, 3% | middle shuffled | 0.9 (58.9) 1.1/2.3/12.4/45.2 r 4.4 | 0.4 (91.7) 0.7/2.4/17.3/69.3 r 2.4 | 0.0 (98.7) 0.1/1.4/23.2/91.0 r 0.0 |
   | even, 3% | all shuffled | 0.9 (57.7) 1.2/2.8/12.6/43.8 r 4.8 | 0.4 (91.7) 0.5/2.2/18.1/69.4 r 2.4 | 0.1 (98.6) 0.2/1.6/23.0/90.0 r 2.4 |

   **Read row by row.** (The rows of a load in step with the rounds are means over the load's
   phase. The order is the same in every report, so a load locked to the rounds at one phase
   hits the same feature each time: "What remains" below.) On a load in step with the rounds
   the order taken tells costless features as costs in 1.1% to 4.2% of cases where the second round's order told 30% to 36%,
   and steady reports that tell one went from about 40% to 4% to 17%; with the period a
   twentieth off a round's length, from 14% to 21% at five and nine frames to 1.5% to 4.3%; with a
   period of two rounds, from 20% to 28% to 0% to 3%. **It is worse at a period of 1.2
   rounds**, where the second round's order told nothing (0%) and the order taken tells 3.2%
   and 2.9% at five and nine frames (0.1% at sixteen); there, though, the second round's order
   hid a real 10% cost entirely at five and nine frames, and the order taken tells it two
   times in three. On the rows of the main table: for noise that does not depend on time
   (even, skewed, frames late each on its own) the order changes nothing in law, and the
   differences (up to 0.3 points a feature, 0.7 a report) are what another draw gives; under
   bursts the false rate is the same or lower and real costs are told far more often (a 5%
   cost with half the frames late in short bursts: 21% of the time at nine frames before, 63%
   now); **a slow drift is the one row that moves the wrong way, by 0.1 point at five and seven
   frames** (0.0% to 0.1% a feature; at five frames 0.8% of steady reports, from 0.1%), while a
   drifting machine's costs are told far more often. The order was taken on those numbers.

   *A whole report* (the fifth finding): on even noise of 1% a steady report tells some
   costless feature a cost in 3.4% of reports at five frames, 1.5% at nine and 0.3% at sixteen
   (the second round's order: 3.1%, 1.3%, 0.2%: the same, by another draw). The rates per
   feature in the tables are for one feature of eight.

   The main table (`the_noise_table_in_light_md`), the rule now in the order taken, at 5, 7, 9
   and 16 frames as before:

   | Noise | 5 frames | 7 frames | 9 frames | 16 frames |
   |---|---|---|---|---|
   | even, 1% | 0.9 (0.0) 4.8/18.8/69.6/98.3 | 0.8 (0.0) 5.8/22.8/83.7/99.9 | 0.4 (0.0) 5.2/27.6/94.9/100.0 | 0.1 (0.0) 3.7/38.4/99.9/100.0 |
   | skewed | 0.7 (0.0) 3.0/12.2/53.6/91.0 | 0.5 (0.0) 4.2/15.5/67.1/97.3 | 0.2 (0.0) 2.9/18.2/81.2/99.7 | 0.0 (0.0) 1.9/25.3/97.0/100.0 |
   | a third late by a fifth | 0.5 (98.9) 6.0/11.6/16.9/18.9 | 0.4 (99.7) 9.6/21.6/27.0/30.9 | 0.2 (99.8) 7.7/17.2/20.0/22.8 | 0.0 (100.0) 10.2/18.1/23.2/28.8 |
   | half late by a fifth | 0.7 (100.0) 2.1/3.2/5.3/8.6 | 0.7 (100.0) 2.1/5.3/7.5/13.3 | 0.3 (100.0) 1.3/2.2/4.5/8.6 | 0.0 (100.0) 0.3/0.9/2.6/7.2 |
   | bursts, a third of the time, ~8 renders | 0.1 (73.6) 39.5/59.2/61.6/63.1 | 0.1 (80.1) 49.8/74.8/80.8/81.1 | 0.0 (81.6) 61.4/77.8/78.2/79.9 | 0.0 (88.1) 90.1/94.7/95.2/94.7 |
   | bursts, half the time, ~8 renders | 0.3 (95.8) 26.7/42.3/50.1/49.5 | 0.1 (98.0) 33.6/57.5/64.3/67.2 | 0.0 (98.9) 43.8/62.5/62.6/65.6 | 0.0 (99.9) 75.4/85.4/85.0/86.7 |
   | bursts, half the time, ~40 renders | 0.0 (56.8) 56.6/81.7/85.5/85.1 | 0.0 (68.9) 71.3/96.2/97.0/96.5 | 0.0 (75.1) 86.2/95.9/96.4/96.2 | 0.0 (87.9) 99.4/99.8/99.9/99.8 |
   | a slow drift of a tenth | 0.1 (75.4) 14.7/53.3/99.3/100.0 | 0.1 (100.0) 17.9/65.2/100.0/100.0 | 0.0 (100.0) 41.7/94.6/100.0/100.0 | 0.0 (100.0) 83.7/100.0/100.0/100.0 |

4. **Uneven frames are no longer said to be another program.** `load.what` and the note for no
   advice now say the frames were uneven by so much, which may be another program on the GPU, a
   clock that moved, or the machine's own unevenness, and that the frames alone do not say
   which; still no advice there. The brief's "a longer run may tell" is not said, because by
   the table it is not true: even noise of 3% is called not steady in 58% of reports at five
   frames, 92% at nine and 99% at sixteen (the share of late frames is the machine's, and more
   frames only measure it better). Telling bursts from even unevenness by the frames' own
   scatter was not tried: the one measure the job has, the share of late frames, is the same
   kind of number under both, and a rule written for it would be a guess.
5. **Sentences**: "What it does" 3 now says how a round is ordered; the sentence about
   `sight_frame`'s old wait in "What was mended, second round" says what it did (streaming,
   then the models, and nothing more while a model was building); the whole report's false
   rate is under 3 above.
6. **A picture that failed to load** is now warned of: the job's first render (the view as it
   is) takes `sight_frame`'s own warnings of the view as it was drawn
   (`Engine::drawn_view_warnings`, which is `capture_warnings` with this view: 2D problems,
   models not drawn or failed, terrain stand-ins, lights left out, the camera's warning). The
   job's own words for lights left out and the camera's warning are gone in favour of those.
   *Test*: two missing pictures in the sprite test; once their loads have failed the job's wait
   ends and both are in `warnings` as "texture sprites/...: ..." (not "still loading").
7. **A render still on the GPU: it spun, and now waits.** By reading: `run_steps` goes round its
   queue while there is credit; a job alone that answers `Step::More` is taken again at once;
   `land` looks at the fence without waiting; so a job whose render was not done looked at
   the fence again and again until the step credit was spent (up to 12 ms a pass; with a
   window, about a quarter of every frame). By running: with `More` put back, the step
   that `step_once` takes right after a submit found the render still on the GPU, and the tests
   that step a whole report failed (the seeded fault below). Now it answers `Step::Wait`: it goes to the back, the jobs behind
   it get their slices, and the pass ends once every queued job waits; it looks again a pass
   later. Headless, passes go back to back while step jobs are queued, so a report is not
   slower. **With a window this was wrong** (the fourth review): a pass is once a frame, so
   exactly one render landed a frame, where the spin had landed nearly four a frame at 20
   frames a second (this sentence said "could land two"), and a report with a slow window took
   3.7 times as long. Mended in the fourth round, below. `timeline_preview`'s job
   (`timeline_tools.rs`) has the same pattern and was not changed in this round.
8. **Not checked by the review, checked now.** *A session that closes while the job waits for
   a view still loading*: the job is queued, waiting, and is dropped with `session_closed`
   when its session is seen gone (`Engine::drop_closed_steps`, the part of `pump_steps` that
   did it, now a function so that a test can call it; behaviour unchanged). *A model still
   building when the report is asked for*: the job waits for it as `sight_frame` does, its
   wait ends `ready`, and the view is measured with it built and no warning
   (`a_view_whose_model_is_building_waits_for_it`, over Sepulcra's tombstone).

*Faults seeded back, each once, and what caught it*: the call's-time cut taken out (the sprite
test); the late line at 7% and at 3% (the hand case; at 3% the simulation test too); the order
of the second round put back (the order's test, and the locked load in the simulation test);
the view's warnings dropped (the sprite test); `More` for a render still in flight (`step_once`
in the GPU test and the model test); closed sessions never dropped (the sprite test). Every one
was caught.

**What remains.** A load in step with the rounds still tells a costless feature a cost 1% to
4% of the time per feature, and 4% to 17% of steady reports name one; at a period of 1.2
rounds it is now about 3% where it was none. Those are means over the load's phase (where in
a round it starts). **The order is the same in every report**, so a load locked to the rounds
at one phase would fall on the same places, and so on the same feature, report after report:
the fourth review simulated one (+4% over half of each round, nine frames, the phase held)
and at some phases one costless feature was told in three reports of four, and at others none.
Such a load would have to keep step with the report's own rounds; none was found (in about
2,300 raw frames an adapter no render's time depended on its place or on the render before
it). The order is not changed for it. A slow drift is told one time in a thousand at
five and seven frames. A machine that is merely uneven is still not advised on. Not run: an
engine with a window, and a release build.

## Its fourth review

A fourth model (2026-11-20) reviewed the third round's mends alone, with engines running
with and without a window on both adapters, and said **not to merge them, for one finding**
(mended: "What was mended, fourth round", below).

1. **Major: with a slow window, a report takes about 3.7 times as long as it did.** With a
   window, step jobs are stepped once a frame. A job whose render is still on the graphics
   card now waits for the next frame, so exactly one render lands a frame, where the old
   spin landed nearly four. At a window held to 20 frames a second: nine frames a variant
   took 1.07 to 1.64 s before and 3.95 to 4.04 s after; sixteen took 1.67 to 2.75 s and
   6.71 to 6.83 s. At 60 frames a second there is no slowdown (1.29 and 1.31 s after
   against 1.34 and 1.37 s), and headless none (252 and 355 ms against 346 and 420 ms).
   The window held its rate in both builds. No call ran out of time in a run; by
   arithmetic the largest report (187 renders) cannot end inside its call's 22 seconds
   once a window falls under about eight and a half frames a second. "What was mended,
   third round" above says the spin "could land two" renders a pass and says nothing of
   this cost.
2. *Minor:* the order of a round's places is the same in every report, so a load locked to
   the rounds at one phase would hit the same feature every time (simulated: one costless
   feature told in three reports of four at some phases; the 1% to 4% above is the mean
   over phases). The old order did that at most phases. No such load was found.
3. *Minor:* a late frame is measured against its variant's quickest third, **and never
   fewer than two frames**, which the page does not say (it matters at five frames).

Found sound: the orders' table, by a simulation of the reviewer's own written from this
page; other loads it tried (a period of half, two and four rounds; a render that is slower
after the view as it is; heat that builds and falls) raise nothing above even noise; the
order is a pure function of the round, the place and the count; in about 2,300 raw frames
an adapter a frame's time does not depend on its place in the round or on the render before
it; the costs told agree with the third review's on both adapters; a waiting job is still
dropped at its call's deadline; the warnings are `sight_frame`'s word for word; every
seeded fault was caught; no validation message. A note it left: the timeline's preview job
still spins as this one did (on this job's copy of that code the engine thread polled for
four fifths of a headless report's time).
