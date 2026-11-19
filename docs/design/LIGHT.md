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

`perf_light {width, height, camera | eye, target | view_2d, include_ui, frames, budget_ms}` (every
argument optional; the view is `sight_frame`'s, by the same parser, `include_ui` with it):

1. Draws the view offscreen **as the world has it**. That first render says what the view pays
   for: which of the world's features are on, whether baked GI was drawn, how many point lights
   the frame was given.
2. For each feature that is on, draws the view **with that one feature left out**, and once
   more with all of them out (when more than one is on).
3. Does that in **alternating rounds**, `frames` of them (5 to 16, 9 unless given), after one
   round that is drawn and not counted, so that a machine that warms up or is busy for a moment
   moves every variant alike.
4. Takes each render's GPU time from **the adapter's own timestamps** (first pass start to last
   pass end; the ID buffer is not drawn), and answers the median of each variant.

It changes nothing in the world and writes nothing: a feature is left out of the measuring render
alone (`light_tools::strip`), never out of the world's settings. It is a step job: a render is
submitted in one step and landed in a later one, between frames, so the GPU's work never holds
the engine thread ([WORKERS.md](WORKERS.md)); a view carries its own timings from the moment it
settles (`Rendered::gpu`), because another offscreen render would have replaced the GPU's last
record before the job asked for it. **A measuring render lands its timings alone**: its image is
not copied back, its subjects are not projected, and it takes no place in the flash watch.

Before its first render it **waits for what the view is still loading** (models building,
streamed cells, textures), as `sight_frame` does, for up to ten seconds, and then measures the
view as it is and says so in `warnings`. And **a job whose call's time is nearly out answers with
the rounds it has** (three seconds before the end, at the start of a round), saying how many: a
timed-out call would answer nothing.

## What it answers

| Field | What |
|---|---|
| `adapter`, `size`, `camera` | which GPU, at what size, through which view |
| `budget_ms`, `budget_from`, `holds_budget` | the GPU time a frame may take: the call's, or this adapter's `gpu_ms` from `perf.toml`; and whether the view as it is holds it |
| `as_it_is` | the view as the world has it: `gpu_ms` (the median), `min_ms`, `max_ms`, `frames` |
| `without_all` | every feature that is on left out: what is left is the scene's own drawing |
| `features` | one row for each of `shadows`, `ssao`, `bloom`, `fxaa`, `fog`, `sky`, `grading`, `gi`, `point_lights`, the costliest first: `on`; and when it is on, `cost_ms` (as it is, less the view without it), `share` of the frame, `without_ms`, `noise_ms`, `told_from_noise`, and `to_change` (the call that switches it off or turns it down) |
| `passes` | the view's dearest passes as it is, by name (`shadow`, `scene`, `ssao`, `post_bloom`, ...) |
| `to_hold_budget` | only when the budget is not held: which features to `give_up`, costliest first, the `estimate_ms` that leaves, whether that is `enough`, and a `note` |
| `settings` | what the features are set to (shadow resolution and cascades, ssao samples, bloom levels, the GI's tier, the lights in view) |
| `engine_light` | what the engine's light `has` and `has_not`, whatever the machine |
| `measured`, `renders`, `took_ms`, `warnings` | how it was measured, how many renders it took and how long, and anything to know (lights left out of the frame, GI that failed to bind) |

**What is and is not said:**

- **A cost inside the noise is not a cost.** `noise_ms` is the scatter of the noisier of the two
  variants: some times the middle of its frames' distances from their median (and never under
  a two-hundredth of the frame), which one late frame cannot spoil. How many times goes by the
  frames there are, so that two variants that cost the same are called apart about once in two
  hundred: three for nine frames or more, four for seven or eight, six for five or six (by
  simulation; three times at five frames called one costless feature in twenty a cost). A cost smaller than that has
  `told_from_noise: false`, and is never named in `give_up`. Other programs use the same GPU: on
  a busy machine small costs are not told from nothing, and the answer says so in place of a
  number to trust. (The least frame to the most was tried first: one frame of nine 0.3 ms late
  hid every cost under 0.3 ms.)
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
(90 renders, under a second in all). These are this laptop's numbers and nobody else's.

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
  (costs, their order, what is told from the noise, what to give up for a budget, and that a
  budget under the scene's own cost is said to be not light's doing).
- With one (skips cleanly without): a small world with shadows, ambient occlusion, bloom and
  FXAA on is measured; the report has those four on and the other five off, each measured alone
  and all together, in the rounds asked for and one more; and **the world is as it was** (the
  same revision, the same settings) and **no measuring render compiled a pipeline**. What each
  costs is the machine's to say: the test holds what was drawn and counted, never a time. Two
  jobs of two views stepped in turn both finish with their own timings; and a job given three
  seconds answers early with the rounds it has.

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
of what the engine's light has and has not, against the renderer. **These mends are not
reviewed yet.**
