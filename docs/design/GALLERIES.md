# Design: galleries, the pick flow, the preference store, tuning, reference boards, onboarding

Status: designed and built 2026-10-29 (M4 lane "taste direction"). Covers the ROADMAP M4 items **Galleries: "choose,
don't describe"** (with the owner's pick flow of 2026-10-22), **Pick-driven tuning**, **Preference store**,
**Reference boards** and **Style configurator and first-run onboarding**. Builds on the asset studio
([ASSET_STUDIO.md](ASSET_STUDIO.md): the offscreen stage, its render budget, `style.toml`) and the review lane
([REVIEW.md](REVIEW.md): the queue and the window's modal screen). Code: `crates/specula-gustus` (pure: spaces,
spreads, the optimiser, galleries as data, preferences, board measures, presets), `crates/specula/src/taste.rs`
(engine state and persistence), `taste_tools.rs` (arguments and schemas), `studio/gallery.rs` (rendering rounds
through the studio), `gallery_screen.rs` (the window). Proof: `scripts/gallery_check.py`.

## Why

The research ([AGENT_GAMEMAKING.md](../research/AGENT_GAMEMAKING.md) §1) says three things. Models can't judge
beauty alone (26.5% against experts' 68.9% at picking best and worst). People compare far more consistently than
they score. And people show taste by **pointing, not describing** (Design Galleries; Sequential Gallery; Midjourney's
style references). So the engine generates variants, a human picks, and the picks are kept: every click is the
project's style data, and agents consult it before they propose anything.

The owner's flow (2026-10-22) is short on purpose: **select a variant**, then **"This one"** or **"This one, with
edits"**. Clicking and a few words replace long prompts, so people who can't say exactly what they want still get
there, and less of their frustration reaches the agents.

## A gallery, as data

A gallery is a **subject**, a **parameter space** and a list of **rounds**. It is a project file
(`assets/taste/galleries/<id>.json`, format `specula.gallery.v1`), so it has versions and the journal like every
project file ([PROJECT_DATA.md](PROJECT_DATA.md)), and the file is the only truth: the engine's in-memory gallery is
parsed from it again after any change (an undo included; a gallery just saved is cached as written, every other
changed file is parsed again). Points are stored at full precision,
so a gallery parsed after a restart plans exactly the round it would have planned before.

**Taste state is reserved data** (AGENTS rule 13; a review probe forged the human's pick with `project_write`, and
`gallery_apply` then wrote it into style.toml). Everything under `assets/taste/` (project-file kind `taste`:
galleries, suggestions, boards, onboarding; compared case-folded, so `assets/Taste/` is the same folder) is written
only under `Rule::Taste`, which only the savers of galleries, suggestions, boards and onboarding pass to
`Engine::project_put` (never derived from a path); the project tools' rule is refused there (`reserved`), and
`Rule::Taste` writes nothing else. A gallery's `apply.file` may not point into `taste/` (in any spelling). Review 2
closed three more ways in: **no agent claims taste state** (`claim_take` there is `reserved`) and a claim on what
holds it (`/project/assets`) doesn't apply to the taste rule, so a builder can't lock the human's picks out (F5);
**nobody undoes another's taste entry, even by force** (`not_yours`), and force never reaches past later changes to
taste state; and taste state has **its own budget** (1024 files, 64 MiB of text) apart from the project store's, so
filling it never stops a `project_write`. `gallery_delete` removes a gallery: its maker's, or any as the owner (their
private history) or an opener; journaled, undoable by the deleter. Rounds are written by the reserved writer `taste`, decisions by `window:owner` or the deciding agent
(granted owner). Approved results live outside it: `style.toml`, `ui/theme.json` and `assets/style/` (kinds
`style_data` for `priors.json`, `style_notes` for `notes.md`). They are written with the taste state that records
them in **one** commit under `Rule::TasteResult` (`Engine::project_put_all`: claims apply to the result files, never
to the taste state), so a call that applies a decision commits once (AGENTS rule 14; the M4 final audit found
`gallery_apply` and `taste_suggest {apply}` committing twice). A human editing taste files on disk is imported as
`disk`, like any outside edit.

**Subjects** (what each variant shows):

| Kind | Example | Rendered by |
|---|---|---|
| `recipe` | `{recipe: "recipes/tombstone.json", params, seed}` | the studio's stage: one camera fitted to every variant together, so proportions compare at true scale, with the 1.8 m figure |
| `scene` | `{camera: "overview"}` (a named camera) or `{eye, target, fov_y_deg}` | the live world through that camera, read only (nothing is written) |
| `ui` | `{panel: "#2b3038", lines: [{text: "Not enough dust", color: "$cant_afford"}], camera?}` | the system font on the CPU, over the panel colour (or over the scene from `camera`) |
| `stage` | `{}` | the stage with only its references (figure, cube) and ground: looks for a project with no assets yet |

**Parameters** (at most 8, at most 12 dimensions; each maps to the unit interval per dimension):

```json
{"name": "moon", "target": "light.key_color", "kind": "colour", "base": "#d0d9f0", "spread": {"l": 0.12, "c": 0.05, "h": 50},
 "apply": {"file": "style.toml", "toml": "stage.game.key_color"}}
{"name": "fog", "target": "fog.density", "kind": "number", "min": 0, "max": 0.04, "default": 0.012}
{"name": "height", "target": "scale.y", "kind": "number", "min": 0.75, "max": 1.4, "default": 1}
{"name": "look", "target": "preset", "kind": "choice", "options": ["moonlit", "storybook", "muted"]}
```

- `number`: `min`..`max` (`step` rounds it; `default` is where round 1 starts).
- `colour`: three dimensions in **Oklch** around `base`: lightness ± `l`, chroma ± `c`, hue ± `h` degrees.
  Perceptual axes, so "darker" moves one dimension and a spread is even to the eye.
- `choice`: one dimension cut into equal bins.
- `target` is what the value drives in the render: `light.key_color`, `light.key_intensity`,
  `light.key_elevation_deg`, `light.ambient_color`, `light.ambient_intensity`, `exposure`, `fog.density`,
  `fog.color`, `backdrop`, `ground`, `scale.x|y|z|all`, `recipe.<param>`, `ui.<name>` (a `$name` colour of a ui
  subject), or `preset` (a base style). Unknown targets are refused (`bad_target`) with the list.
- `apply` (optional) says where the value lives in the project: `{file, toml: "table.key"}` or
  `{file, json: "path"}`. The window shows it ("Applies to style.toml stage.game.key_color"), so the human knows
  what "This one" decides. Values are written only by `gallery_apply` (or `apply_on_accept`), never silently.

**Rounds:** each holds its variants (index, unit point, values), how it was made (`spread`, `tune`, `edit`, `none`,
`back`, `agent`) and at most one decision. Bounds: 12 variants per round (9 by default, shown in two rows to fill
the window's wide panel), 32 rounds, notes of 280 characters, 512 galleries per project. **A round never shows two
look-alikes**: variants whose displayed values match (numbers within 1% of their range, colours within 0.02 Oklab,
or 0.06 for an interface colour, whose thin text needs the wider gap, as displayed, after gamut mapping and 8-bit quantisation: near-white moons clipped to one colour) are dropped and
the round is filled again with dispersed points. Values can differ and still render alike (near-white moons at full
strength, tone-mapped together), so the renders are compared too: two tiles are alike when fewer than one pixel in
500 differs by more than 6 levels. Every scene tile of a round is drawn at the same moment, so flames, embers and
trails neither tell tiles apart nor make them differ. In a round the engine planned (not an agent's own, a breadcrumb's, or one steered
by chips), the later tile of each alike pair is replaced once by a point far from everything shown and rendered
again (`replaced_alike` in the packet). Pairs still alike are reported as the round's `warning` (`rendered_alike`):
usually a parameter that shows nothing from where the round is seen, such as fog density from a camera above
Sepulcra's mist. Show it from another camera or take it out of the space.

**Interface colours keep to the game's palette rules** (the integrator's review: a "can't afford" spread offered fire
orange and grave green). A `ui.*` colour that reads as a family `style.toml` reserves (`[palette] reserve`) or
forbids to the interface (`forbid = { ui = [...] }`; Sepulcra forbids `enemy`) loses its colour before the round
is rendered: lightness and hue stay, chroma drops to half of `neutral_max_chroma`, and if that makes it a look-alike
of another tile its lightness steps apart. Where the space can't go that grey (a saturated accent's spread), its
hue and chroma turn towards the base's instead. A guarded tile that still finds no room of its own is dropped
(`dropped_alike` in the packet; never the incumbent). In a round the engine planned, it is planned again among
points the rules leave as they are and that look like no tile shown (`refilled`); what can't be refilled stays
out, and the round says so: the round's `fit` and the packet's `fit_note` ("6 options fit the style rules"), shown
on the screen's round line. A round of five that read apart beats nine that don't: Sepulcra's rules leave a "can't afford" spread around a
dim warm grey a wedge of greys with room for five or six labels. The packet's `palette` lists each change. "Reads as" is generous on the interface:
a colour more colourful than `neutral_max_chroma` reads as a family when its hue falls in that family's hue arc
(the members' hues, widened by 30 degrees: at 15, sage and mint accents passed), not only when the family is its nearest palette colour. An olive
nearest the floor's moss is still the dead's green on a label, and a peach nearest nothing is still fire.

## The pick flow

One gallery is open on the window at a time. The human **selects** a tile (click, tap, arrows and Enter, a pad's
d-pad and A, or the digit keys), then:

| Button | Recorded | Next |
|---|---|---|
| **This one** | `accept` of the selected variant | the gallery is done; agents read its values |
| **This one, with edits** | opens the note field and the chips; **Send** records `edit` with the note and chips | a new round around the pick |
| **None of these** | `none` | a new spread, away from everything shown |
| a breadcrumb (`1 · 2 · 3`) | `back` to that round | its variants again, as a new round (history is never rewritten) |
| right-click, Delete, or a pad's X on a tile | a `dislike` of that variant (the point clicked, if any) | the round stays open |

**Edit chips** ("darker", "brighter", "bigger", "smaller", "simpler", "more detail", "warmer", "cooler", "bolder",
"subtler") are one tap each and are offered only when some parameter answers to them: brightness and colour
lightness, `scale.*` and size-like recipe parameters (`height`, `width`, `size`...), detail-like ones (`detail`,
`segments`, `tier`, `runes`...), colour hue and chroma. A parameter can name its own (`chips: {"darker": -1}`).

**The text field** takes typed text when there is a keyboard (Char events, Backspace). **Chips alone finish a
round** (F6: nothing needs a keyboard): Send works with chips and no note, or with neither (refine around the pick).
The engine never interprets the note; it goes into the store and the next round's record, where agents read it and
can steer (`gallery_round`).

**Nothing is picked unseen** (as on the review screen): a press counts once the round's sheet has been drawn for
0.4 s, and focus moves off the decision buttons after a decision. While open the screen is **modal** (the game,
modules and live Lex get no input; under an open review screen the gallery gets none either). Opening and closing:
F8, or LB + RB + X on a pad (pads are polled with or without a hosted game), or `gallery_open` / `gallery_close`
from an agent (after the human closes it, agents can't reopen it for 30 s). Esc or a pad's B while typing a note
leaves the note, not the screen; chips and the breadcrumb page, so every one is reachable by hand; without a UI
font the screen never holds the window.

**The game waits while the human decides** (the integrator's decision): while the gallery or the review screen holds
the window, the hosted game is told (`Game::modal_changed`, journaled like any frame) and pauses itself through its
own pause path; Sepulcra resumes when the screen closes unless the player had paused. Engines without a hosted game
pause nothing.

**Rounds that can't be made are tried again.** Transient failures (the studio busy, a timed-out or dropped render, a
GPU hiccup) back off 2, 4, 8... seconds (at most 60, six attempts); permanent ones (a bad subject, bad points) wait.
The screen shows the failure with **Try again**. An open round whose sheet is gone (a restart on a fresh captures
folder) is rendered again, so the window can always decide it; sheet layouts are cached, never re-read per frame.

## Pick-driven tuning (the optimiser)

Round 1 is a **dispersed spread** (Design Galleries): the start point (`default`, a prior, or the current value)
plus the points of a scrambled Halton set chosen farthest-first, so variants are as different as the space allows.

After a pick, every tuned round runs **preferential Bayesian optimisation** from all the picks so far:

1. Every round's pick is a **choice**: the picked variant over each other one it was shown with (Bradley-Terry-Luce,
   a multinomial logit on a latent goodness `f`). "None of these" makes the incumbent (the last pick, which every
   tuned round shows again) the choice over the new ones.
2. A **Gaussian process** prior on `f` (squared-exponential kernel in the unit cube) and a Laplace approximation:
   damped Newton steps for the MAP of `f` at the points shown, then the posterior mean and variance anywhere.
3. **Expected improvement** over the best mean, scored on seeded candidates inside a **trust region** around the
   incumbent that shrinks as the rounds go (half-width 0.5, falling to 0.08), so a discrete gallery keeps
   resolution where the picks are.
4. `mode: "tune"` (the default) shows the incumbent, then **batch EI picks** (chosen greedily with local
   penalisation) and points of a **shrinking spread** around the incumbent, alternating. `mode: "line"` is
   Sequential Line Search, the 1-D subspace per round of Koyama et al.: points on the line through the incumbent and
   the EI maximiser.

**Chips never go the wrong way.** Every chip dimension moves only the chips' way from the pick (`step_k * d_k >= 0`)
and, while any can, a variant must move along them. When none can (the pick sits at their bound: "darker" on the
darkest moon), the chip dimensions are held and the others vary; the round records `chips_exhausted` and the screen
says so. (A review probe found 6 of 9 lighter tiles at the dark bound, and 9 identical tiles in line mode.)

**Why the mix is the default (measured on what production runs):** `production_tuning_beats_both_baselines` drives
the gallery API exactly as the engine does (first round, picks, `plan_next`, `push_round`) with a simulated picker on
a hidden optimum, 12 seeds, 8 rounds of 9. Mean regret:

| Dimensions | tune (the mix) | line | spread hill-climb | random |
|---|---|---|---|---|
| 2 | 0.005 | 0.093 | 0.003 | 0.053 |
| 4 | 0.019 | 0.495 | 0.016 | 0.346 |
| 6 | 0.096 | 0.618 | 0.123 | 0.692 |
| 8 | 0.283 | 0.718 | 0.316 | 0.820 |

Batch EI alone lost to the simple spread hill-climb in 2-4 dimensions (0.021 and 0.034) and won in 6-8; the mix keeps
the better of both. The line subspace is no better than random in a 9-tile gallery (a line suits a continuous
slider); it stays as `mode: "line"`.

`mode: "spread"` instead spreads around the pick with a radius that shrinks each round (chips shift its centre).

Deterministic: a seed per gallery and round (`specula_core::rng`), `dmath` for every transcendental, ordered
containers, no clocks; `specula-gustus` is a simulation-class crate under the determinism lints. Bounded: at most 288
observed points (duplicates merged), 12 dimensions, 32 rounds, 512 EI candidates; one round's optimisation takes
milliseconds and runs on a worker thread, never the window's.

**Proof (unit tests):** `production_tuning_beats_both_baselines` (above), `tune_beats_random_spreads` (the optimiser
alone, with pick noise), `chips_never_go_the_wrong_way` (every mode, chip set and incumbent at a bound or in the
middle) and `rounds_never_show_look_alikes`.

## The preference store

Every decision is a record in its gallery file: who decided (the window's `user`, or an agent the operator granted owner),
the round, the variant's values, the note, the chips, and the subject. `taste_query` reads them across galleries:
filtered by subject, target, user, kind (`accept`, `pick`, `reject`, `dislike`, `none`), note text; paged and
bounded. **Agents consult it before proposing** (the tools' descriptions say so): what was picked for this target
before, what was rejected, and what the notes said.

**Suggestions, never silent rules.** When one target has **3 or more distinct picks agreeing** (distinct
(gallery, value) pairs: the same incumbent picked again and again in one gallery counts once; numbers within 12% of
their range of the median, colours within 0.04 Oklab, choices the same option) the store proposes:

- a **`style.toml` edit** when the parameter's `apply` names `style.toml` (or any project file): the exact key and
  value;
- otherwise a **prior** (`assets/style/priors.json`): future galleries over that target start there.

Suggestions wait for the human. `taste_suggest {submit}` puts one on the **review queue** (batch
`taste:<suggestion id>`, with the gallery's sheet as the capture and the evidence as checks). Taste batches are the
taste tools' only (`review_submit` refuses them), only the submission they recorded counts, and **only the owner
decides it**: the window's review screen, or an agent the operator granted owner (a reviewer or lead is refused,
`owner_only`). Approving writes it as the decider; turning it down dismisses it. `taste_suggest {apply}` by the owner
does the same as approving. Results are written whole or not at all: a later write that fails takes back the
earlier ones (`rolled_back`).

**Privacy (the integrator's decisions).** Raw picks, notes and gallery history are personal. They live under
`assets/taste/` **per project**, never shared across projects, with no wall-clock times, machine or OS user names
(`user` is a label, "owner" by default), and **public repositories ignore that folder**: the repo's `.gitignore`
ignores the `taste/` folder of every default assets root (`/taste/` for a plain `cargo run -p specula`, whose assets
root is the working directory; `**/assets/taste/` for any game's; Sepulcra's), and a test
(`taste_history_is_never_tracked`) holds it. What the human approved from them is tracked: `style.toml`,
`ui/theme.json`, `assets/style/priors.json` and `assets/style/notes.md`. **notes.md says what was chosen, never the
owner's words**: quoting their notes is an opt-in (`style_onboard {quote_notes: true}`), and the review submission
the owner approves then shows those notes word for word. A private project that wants its history versioned removes
the ignore lines. Sharing across projects is the owner's call (later).

**Who may do what** comes from the engine's one authority ([AUTHORITY.md](AUTHORITY.md): granted by the operator,
never self-declared), mapped once by `taste_authority`: the **owner** (the window, or an agent the operator granted
owner, `--owner NAME`) picks, applies, approves taste results and deletes any gallery; an **opener** (a granted lead
or reviewer: a reviewer already puts work on the owner's screen, so it opens galleries too) opens and closes the
human's screen, replaces and deletes any gallery's round, dismisses suggestions and cancels the configurator; **any
other agent** (`art_director` and the like are job labels) makes galleries and does those things to its own. The
tools' access entries: reads for `gallery_get`, `gallery_list`, `gallery_wait`, `taste_query`, `board_check`; the
taste rule for `gallery_create` and `board_measure`; owned galleries for `gallery_open`, `gallery_close`,
`gallery_round`, `gallery_delete`; the owner for `gallery_decide`; the taste result rule for `gallery_apply` (the values and the gallery that records them, in one commit: one tool call, one commit); and open,
with reasons, for `taste_suggest` and `style_onboard`.

## Reference boards

The human drops images into `assets/taste/boards/<board>/` (PNG now; JPEG through `specula-core`). **Binary assets
through the protocol** are a parallel lane: a board reads its images through one seam (`BoardSource`: a folder on
disk today, the blob store later), so uploads add a source and nothing else changes. Window drag-and-drop waits for
that lane too (it would need a drop target in `specula-platform`).

`board_measure` measures each image (bounded: 32 images, 4096 × 4096 each, shrunk to 256 px for measuring):

| Measure | How |
|---|---|
| value histogram | 16 bins of Oklab lightness, plus mean, p10, p50, p90 |
| saturation | mean and p90 Oklch chroma |
| palette | 6 colours by seeded k-means in Oklab (weights are shares of pixels) |
| contrast | p90 − p10 lightness, and RMS contrast |
| edge density | share of pixels whose lightness gradient (Sobel) passes 0.1 |
| shadow softness | at strong edges, the width in pixels over which lightness goes from 20% to 80% of its step |

The board's **targets** are the images' spread: for each measure, the range from the lowest to the highest image,
widened by a tolerance (10% of the measure's scale). They are written to `taste/boards/<board>.json` with each
image's hash (a changed image makes the measures stale until `board_measure` runs again).

`board_check` measures a scene (a named camera, rendered through the studio) or a capture the engine wrote, and
reports each measure against the board's target **like the studio's taste checks**: measured value, the target
range, the rule (`board:<name>.<measure>`), pass or fail, and a revise hint ("darker than the board: the median
lightness is 0.18, the board's 0.24-0.38"). Palette distance is the mean Oklab distance from each scene palette
colour (weighted) to the board's nearest.

## Style configurator and first-run onboarding

A new project starts from a **base style** (`specula-gustus::presets`): `moonlit` (Sepulcra's night), `storybook`
(bright, saturated, soft shadows), `muted` (low chroma, overcast), `noir` (high contrast, near monochrome) and
`clean` (neutral daylight). Each is a full `[stage.game]` rig, fog, a palette mood and UI colours (`text`, `accent`,
`danger`, `cant_afford`).

`style_onboard {start}` runs a short sequence of galleries, each one round or more:

1. **The base style**: a `choice` over the presets, rendered on the project's own subject (a recipe or a scene
   camera) or on the stage.
2. **Key light**: the moon's or sun's colour and strength around the chosen preset.
3. **Atmosphere**: fog density and colour.
4. **Interface colours**: the accent and **"can't afford"** on the game's panel (`ui` subject).

`style_onboard {status}` says which step is open; each step's accept moves to the next (from the window or the
`owner` role). `finish` builds the result: a `style.toml` (the preset's rig with the accepted values; an existing
file is patched key by key, its other tables kept) and `assets/style/notes.md` (STYLE.md-style notes: what was
chosen and from which round; the human's notes only with `quote_notes`). It goes on the **review queue** with a sheet of the chosen
looks; approving it writes both files as the decider. Nothing is written before that approval.

## Tools

| Tool | Does | Writes |
|---|---|---|
| `gallery_create` | A gallery from a subject and a space; renders round 1 (waits for it, bounded) | its gallery file |
| `gallery_get` | One gallery: rounds, picks, notes, per-variant measures; `image: true` adds the round's sheet | nothing |
| `gallery_list` | Galleries, newest first, paged | nothing |
| `gallery_delete` | Deletes a gallery and its picks (its maker's, or the owner's or an opener's) | its gallery file |
| `gallery_open` / `gallery_close` | Puts a gallery on the human's window (or closes it); `user` labels the picks | nothing |
| `gallery_wait` | Waits (at most 25 s) for the next decision on a gallery, or answers at once if one is newer than `after` | nothing |
| `gallery_decide` | A decision as the owner (an agent the operator granted owner): the same decisions as the window | its gallery file |
| `gallery_round` | An agent's own next round: a new centre, spread or variants (supersedes an undecided round; the gallery's maker, the owner or an opener) | its gallery file |
| `gallery_apply` | Writes an accepted gallery's values to their `apply` targets (not a configurator step's: those go through its reviewed proposal), all or nothing | the target files |
| `taste_query` | The preference store: picks, rejections and notes, filtered and paged; plus the priors | nothing |
| `taste_suggest` | Suggestions from repeated picks: list, `submit` to the review queue, `apply` (owner), `dismiss` | suggestions, then the target files |
| `board_measure` | Measures a reference board's images and writes its targets | the board file |
| `board_check` | A scene or capture against a board, as findings | nothing |
| `style_onboard` | The style configurator: `start`, `status`, `finish`, `cancel` | onboarding state, then style.toml and notes |

Every answer is bounded (galleries list 32 a page, query 64 records a page, 48 KiB), errors are structured
(`{error, message}` plus fields), and every write is a project-file commit through the write policy: taste state
under `Rule::Taste` (no claims apply; undone only by whoever made the entry), results under `Rule::Project`
(claim-checked); all journaled with their author. The engine thread's own taste work is bounded: the pump looks at
4 galleries of each kind a second without cloning any, suggestions are worked out once per files revision, and the
window's screen reads the shown gallery in place. Renders go through the studio's queue (one at a time, round-robin with other agents' studio calls, within
its render budget); the window never waits for them.

**iGPU budget:** a round of 9 variants renders within 1.5 s wall time, each render at most 30 ms of engine-thread
work (spread over frames within the studio's 25% share), and the window keeps its frame pacing (no frame over
50 ms caused by the gallery). `scripts/gallery_check.py --pacing` measures it. Measured 2026-10-29 on the Intel iGPU
(RaptorLake-S) with Sepulcra running a wave: a round of the game's own scene in 585 ms (434 ms of rendering), each
render at most 11.6 ms on the engine thread and 2.2 ms of GPU, window frames p50 8.4 ms and p95 17.2 ms over the
round, none over 50 ms because of the gallery (the one slowest frame, 69 ms, was not the studio's).

## Later

- A small learned preference model from the store (VAB: about 2,000 comparisons transfer).
- 2-D subspaces per round (Sequential Gallery's plane) for spaces with many parameters.
- Window drag-and-drop and protocol uploads for boards (the binary-assets lane).
- Sharing preferences across projects (the owner's decision).
