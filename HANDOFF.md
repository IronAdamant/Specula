# Handoff: read this first

Notes from Claude (Fable 5.2 since 2026-11-12) to whoever picks up Specula next, most likely a new
Claude conversation with no memory of the earlier ones. This page is **current state and what's
next** only. Last updated **2026-11-22**.

**The full record is [HANDOFF_HISTORY.md](HANDOFF_HISTORY.md)**: this page as it stood on
2026-11-21 before it was cut down (the owner's words by date, every review round of M6 part 4,
how M3 to M5 ran, the measured baselines). It is not loaded into a session; open the part you
need. Older history is in [CHANGELOG.md](CHANGELOG.md). The page was cut because it is read in
full at the start of every session and had grown to 228 KB, most of it the history of review
rounds.

## Right now (2026-11-22)

- **By hand only, and lean.** The owner: "Only by hand working", then "For now we'll stick to 1
  agent". No subagent, and no paid measurement (`examen leverage` builders and judges) beyond what they ask for, until they
  say. When subagents come back: one or two at a time, and always with `model` passed (Opus for
  reviews and builders, Sonnet for picture checks; never Fable).
- **One line, gated.** The working branch `m6-first-hour` holds everything: the shape library
  (stages 1a to 5a), the mesher's mend, the sculpt's box mend, the light and faceting rounds, and
  the fixes of `m6-usus`. **The full gate passed on `2da88f08`** (3,584 tests, repeats, slow
  workers, speed budgets, clippy for both targets, both builds): the first full gate since
  `bec48409`. **The live checks: 71 of 72 passed on it** (2026-11-21, `C:/lv/live1`), the
  window's own among them. **`m3_exit` failed once and its cause is not proved**: on the
  integrated GPU the boss wave's frame pacing was 23.7 ms at the 95th percentile against a
  budget of 20.9. Run alone straight after, it passed with 7.6 ms (`C:/lv/live2`). In the run
  that failed the GPU's own time for the same scene was 1.7 times higher (9.8 ms a frame
  against 6.0) and a system process (`csrss`) was drawing on that GPU, which it was not in the
  run that passed: that points at the machine and not at the window's loop, and it is a
  reading, not a proof. Run the checks whole again on a quiet machine before calling the
  window's loop clean.
- **The pause of 2026-11-21 is lifted** (the owner, after a second reader's reports on the
  comparison: "Pause is now off. Build a plan around our findings and what you'd do, then
  execute it."). The reading both agreed on: the engine's cost is the number of requests after
  the first code, each reading the whole context again; the bridge's "fault" was a brief asking
  more than friction gives; the world page was wrong on bounce and friction; the guide
  prescribed "edit, run, read".
- **Built since, by hand, gated, not reviewed** (FIRST_HOUR.md, sections 14 and 15): the guide
  says "write the game whole, then run it"; the world page corrected; a slab that carries
  (`Spawn::carries`, snapshot version 4); a run that speaks a brief's own input words
  (game.json `inputs`) and is read on its way (`specula run --at T,T --keys a,b`), with the
  world's places and the sounds in the report; a number the rules read from the run
  (`Cx::param`, `--set NAME=v,v`); `specula tune` (a number bisected to a condition) and
  `specula find` (a script of inputs searched over the game's saves, played again from the
  start before it is answered; 90 s unless told); `specula run --sounds DIR`; the live check
  `search_check`. The owner's rule for the searches: whether a builder uses one is decided per
  task by the lead, who monitors the builder and may decide in its brief before it works (a
  job's `lead` lines in `evals/`; the report marks such runs as directed).
- **Three faults of the engine found on the way, all mended:** a test that passed by the
  machine's speed (`261e8d60`, the cause of CI's red on `349c9e64`); a save that lost the events
  the rules had still to be told, so a game put back from a save of its start ran a tick apart
  (found by the comparison's first builder: a script `find` called winning did not win); a
  carrier that gave nothing to what came to rest on it while it moved (found by a medium3d
  builder; mended after the runs, **not measured**).
- **The second six-job comparison (2026-11-22; the owner asked for it; the paid runs are
  done, 38.65 USD at list prices).** Engine over plain, weighted tokens, with the figure of
  2026-11-21 in brackets: small 1.69 (1.77), medium 1.44 (1.22), change 1.87 (1.18); small3d
  1.19 (1.62), medium3d 0.88 (1.71), change3d 0.80 (1.24); checks passed level on every job
  ([docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), the results of 2026-11-22). **In 3D the
  engine came level with a plain build; in 2D it did not move.** Two runs a cell: the 3D
  ranges of the two arms lie over each other, and the plain arm alone moved by up to 29%
  between the two passes, so "level" is what the 3D figures support, not "cheaper". The engine
  builders were directed by the lead, not cold. In 2D, more ways to check brought more
  checking (twice the requests), and a plain build of that size is five to eleven requests: a
  project that must be read first cannot be cheaper than that. `specula find` found the change
  job's new board and did not find a winning script for the bridge board (in 90 s or 500 s).
  **Nothing further is paid for until the owner says.** Not built: hooks. Not measured: a
  cold engine builder on the mended engine, the carrying mend, a job larger than medium3d.
- **A second reader's bug list, mended (2026-11-22, by hand, not reviewed)**
  ([FIRST_HOUR.md](docs/design/FIRST_HOUR.md), section 16): a damaged save could hang the
  engine (a snapshot's settings are range-checked now, and a fuzzing test holds it); a
  `--set` name no rule reads is said; a check takes `set`; `Cx::take_events`;
  `run.sounds_heard`; every public word of the SDK and a sound recipe's fields on the two
  pages a builder reads. Found on the way by `search_check`, and mended: the sounds a run
  reports were no part of a save.
- **The owner's direction (2026-11-22):** "We're still paused on shape, recipe, template, and
  sfx, part 4 and onwards plan until we get this fixed. We're debugging until this engine
  makes sense to be used by you and other agents." Part 4 and everything after it wait; the
  work is faults and the cost of use, until a comparison shows the engine pays.
- **The owner's standing authority (2026-11-22) for one more full comparison** after these
  mends ("Cost are acceptable. Not a issue."). The design: **cold builders** (the jobs' lead
  lines left out: `"lead": false` in both settings files), five runs a cell on the six jobs,
  and the 2D `large` job (written 2026-11-17, never measured) at three. About 135 USD at
  list prices, said to the owner first. After that one pass, paid runs need their word again.
- **Linux is the second reader's side** (the owner, 2026-11-22: it works on a Linux machine,
  builds what the engine needs to start there, and its work comes to `main` by the owner's
  word; this checkout pulls when the owner says). [docs/design/LINUX.md](docs/design/LINUX.md)
  says what runs there, what does not, and where a Linux machine finds faults this one
  cannot. **`origin/main` may move without this session: fetch and look before any push.**
- **The cold comparison is done (2026-11-22; 66 builds, none failed to run; 110.35 USD at
  list prices, of which the judging is 76.91)**
  ([LEVERAGE.md](docs/design/LEVERAGE.md), the last results). Engine over plain, weighted
  tokens, five runs a cell: small 1.70, medium 1.65, change 1.29; small3d 1.38, medium3d
  1.01 (with fewer checks passed), **change3d 0.68** (four of five engine runs under every
  plain run: the one job the engine is cheaper on). **The 2D `large` job, measured for the
  first time, three runs: 5.92 times a plain build, and three checks fewer.** Its three
  engine builders spent 29, 47 and 23 requests reading the engine's documents and source
  for what no page says and, in one case, the engine cannot do: **a game has no way to keep
  anything of its own between two runs of the program** (a best score, a saved run; all
  three failed both items). So "the engine pays where a game is larger" is an aim and not a
  finding. Next by hand, from the ledgers: a game's own kept data; screens of the game's
  own opened, closed and named; a run's lines read by key without being in the report
  (`--keys run.sounds_heard`: four of five medium3d engine games failed "silent at volume
  0" because no builder wrote the line into the report). Then the large job again, engine
  arm only. The owner allowed a second full pass while away ("Full second pass is
  permitted"); none has been started. **Owed to the owner when they are back: the reminder
  on hooks that no harness owns** (they asked for it if the cold pass showed no savings;
  it showed one job in seven).
- **What counts as success, agreed with the owner on 2026-11-22** ("That's also my current
  definition of success"), set before the next measurement: (1) a game that outgrows one
  sitting costs no more through the engine than without, with as many checks passed: the 2D
  `large` job from 5.92 to 1.0 or under; above 1.5 after the mends means the design is wrong,
  not that more holes need filling; (2) both change jobs under 0.8 (cold: 3D 0.68, 2D 1.29);
  (3) the advantage grows with size, shown on a second large job and one larger still,
  written before they are measured; (4) never measured: three agents building one larger
  game finish for fewer tokens, or far less time, than one agent and than three sharing a
  plain folder; (5) no cold builder opens the engine's source (cold: 14 of 33 did). **Not a
  target:** a one-screen game (a plain build is six requests). Every report gives the
  builders' cost and the builders' with the judges' side by side.
- **A second reader's three passes over the cold comparison (2026-11-22)** recomputed every
  figure from the raw streams (all held) and corrected the lead's reading: the engine is
  never dearer a request, it loses by their number; the holes are about half the large
  job's loss (about 2.6 to 3 times is left without them, by subtraction, not measured);
  screens cost as much reading as kept data; the lead's aids went unused (`play` 353 times,
  `check` 9, `tune` 0 in 33 runs); builders still opened the SDK's source for types.
- **The plan, four stages, each ending in a measurement** (the owner: "Start the steps"):
  **1, the holes** (built, FIRST_HOUR.md section 17: a game's kept data, screens the rules
  put up, a key read by itself, `ui.texts` and `ui.focus`, what the engine does not do),
  then the large job again, engine arm, five runs; **2, the shape of a build**: a much
  smaller project to start from, an index of the SDK's signatures, one command that plays
  every check of a brief and says what changed (results handed over without a request, in
  a form no harness owns), then large and small again, both arms; **3**: a second large job
  and a larger one, and three agents on one game; **4**: back to part 4. The second reader
  reviews each stage from a bundle before its measurement.
- **Stage 1 measured (2026-11-22; 22.90 USD): the large job is 5.61 times a plain build, from
  5.92.** The mends fixed what was delivered (30.2 of 31 checks, from 28.3; the saved run and
  the best score pass in all five) and not what it cost: all five builders still read the
  engine's source, 20 to 41 requests each, now for the next things (a title button that
  begins a run another way, what `start` does, `keep` and `show` beyond the guide's lines,
  numbers in a text in three languages). **A hole closed uncovers the next**
  ([LEVERAGE.md](docs/design/LEVERAGE.md), the last results). Stage 2 as planned may bring
  it to three or four; it will not bring it to one. **The question put to the owner, not
  answered yet:** at this size the cost is that the project shell decides things a builder
  must find out, where a plain builder decides everything in code it wrote. Two ways on:
  (a) the rules may drive everything themselves (put any screen up, begin and end a run,
  hear every button and key), so that a builder who wants the shell's help takes it and one
  who does not never has to learn it; (b) the engine answers questions about itself in one
  command (what does this action do, what happens at a start), so that nobody reads its
  source. The lead's view: (a) first, then stage 2, then the large job again.
- **The large job is at 1.92 times a plain build (2026-11-23; 48.35 USD), from 5.61.** A
  second reader's review of stage 1 found faults (mended in `ff4b1298`: a settings file that
  was not read is never written over; Escape over the rules' own screen is the rules'; a
  guide that was wrong on Escape and silent on a title or menu of the game's own) and asked
  for two cheap measurements. Three cells, five cold runs each: the engine brief unchanged
  on the mended commit **1.92** (35 requests, 30.6 of 31 checks; the builders read the
  engine's source a fifth as often); with no path to the repository in the brief 1.98; told
  to check by one command 2.39 ([LEVERAGE.md](docs/design/LEVERAGE.md), the last results).
  **What moved it was what that commit put right, mostly about twenty lines of the guide**;
  the lead's reading of the day before ("the shell decides things a builder must find out",
  so the design must change) drew the wrong conclusion from a true half. The bar is 1.0,
  and 1.5 after stage 2: every engine run still costs more than every plain one. What is
  left is about 19 runs of the game a build at a context twice a plain build's. **Next:**
  the `resume` trap (below); then stage 2 as planned (a smaller project to start from,
  signatures, what changed handed over without a request); a larger job written before it
  is measured, since a plain build of this one is a four-minute sitting.
- **Two measurements, started 2026-11-23 on `3bb629b5`, and the rule written before either
  number** (the owner: "Yes. Go for it"; about 65 USD). **Night Market again**, the job
  unchanged, five engine and five fresh plain runs: a test of one mend (a word given again
  is pressed again) on the job that showed the fault, and of nothing more. **Day Market**,
  as the clean agent wrote it, five and five: a job nobody who knows the engine shaped. The
  lead has not read its text; the second reader has not yet re-assessed it (its bundle is
  with the owner), so a fault it finds in the job afterwards counts against the run, not
  for it. **The rule, for each job by itself:** engine over plain in the builders' weighted
  tokens at or under 2.2, the engine holds on that job; above 3.0, it does not; between, the
  runs are doubled before anything is said. The builders' cost and the builders' with the
  judges' are both reported, with the count of builders that opened the engine's source.
  For Night Market the figure to hold against is 7.39.
- **Both measured (2026-11-23; about 105 USD): by the rule the engine holds on neither job.**
  Night Market again: **3.60** (interval 2.74 to 4.41), from 7.39, checks level at 33.8 of
  34: the one mend halved the engine arm's cost. Day Market, doubled by the rule to ten and
  ten: **3.01** (2.49 to 3.67), 54.2 against 59.8 of 61 checks; all ten engine builders read
  the repository. With the judges counted: 2.13 and 1.84
  ([LEVERAGE.md](docs/design/LEVERAGE.md), the last results). A plain build of Day Market is
  still one sitting (11 to 17 requests), so the first bar is still untested. **Found, not
  mended, frozen until the second reader has read the runs:** a text a module keeps for a
  screen is dropped without a word past 256 characters and the warning says "nothing keeps"
  it (five of ten games failed an item so); the rules cannot end a headless run; judges
  reach fewer states in an engine game than in a plain one, so the checks understate the
  engine arm by an unknown amount; a builder stopped the harness by stopping `examen` by
  name (its run was recorded by hand from its stream). The job the guide was fitted to
  stands at 1.9; one it was not at 7.4, then 3.6 after one mend; a new one at 3.0. Both
  intervals reach into the band between 2.2 and 3.0. **The second reader confirmed the
  figures and the three faults (a long text dropped, quit, several words at one tick) and
  corrected four statements; the owner: "Fix all".** The mends are being made; then Day
  Market again, and its games judged again from one shared pack of scripts.
- **The second large job (2026-11-23; 25.83 USD): engine over plain 7.39. By the rule set
  before the number, 1.92 was fitted to one brief.** Night Market, written by the second
  reader and loaded as given; the engine and its guide frozen at `3207626c`; five engine and
  five plain runs the same afternoon. Plain: 148,378 weighted tokens, seven or eight
  requests, two minutes, 33.8 of 34 checks. Engine: 1,096,486, 61 requests, 32.2 of 34; all
  five builders in the engine's source ([LEVERAGE.md](docs/design/LEVERAGE.md), the last
  results). What they looked for was this job's own: **a word that acts once each time it is
  given** (the engine's scripts hold an action until the next input, so the same word twice
  is one press: three of five games failed the items it decides, and no line of a guide
  mends that), keys that do things with no button for them, a grid with a cursor.
  **The reading:** with a guide that answers a job's questions the engine arm costs about
  twice a plain build; with one that does not, five to seven times; a new game asks new
  questions. The first bar has not been tested at all: neither large job outgrows one
  sitting for a plain build (the second reader expected 22 requests for a plain Night
  Market; it took 7). The one job of eight where the engine is cheaper is still the 3D
  change. **The second reader's assessment corrected the lead on four statements** (the write-up's last part: the headline is 7.39 for builders and 3.64 with the judges; two builders did get a repeated press by a trick; "five to seven times" is two jobs; "stop mending 2D" went beyond one job). **Agreed with the owner next:** three faults of input mended in the engine (the same word twice is one press; a key reaches the rules only through a button; the rules cannot change a setting); Night Market again, unchanged, with a fresh plain arm, as a test of those mends only; a third job, Day Market, written for want of a third author by a clean agent (Opus 5.5) that was given the two job files as patterns and nothing of the engine or its guide, the lead only watching; **frozen before the mends are made**, sized by a pilot of its plain arm alone, re-assessed by the second reader before any engine run. 3D and several agents run beside this line. The owner: cost parity first, then savings.
- **The engine and its guide are frozen from the commit after `5207664e`** (the `resume` mend, below, and a kept value on a screen from the first frame: FIRST_HOUR.md, section 17) until a second large job is measured. Two reviews by the second reader confirmed 1.92 from the raw files (the runs do not overlap stage 1's; the builders' traces changed in kind) and asked for the one test that says whether it is the engine or a guide fitted to one brief: a different large job, written by the second reader and frozen before it is measured, five engine and five plain runs the same day. **The rule, set before the number:** at or under 2.2 the guide generalises; above 3.0 it was fitted; between, the runs are doubled. The lead does not write the job, does not see which of its requirements the guide leaves unanswered, and does not edit the guide or the engine until its runs are judged. Stage 2 is on hold: by the control's ledgers a smaller start is 6% of a build, and the batch check measured as a loss.
- **Found by those runs, mended since:** on a pause screen of the game's own (a shown screen),
  a button whose action is `resume` does nothing: it is the engine's word, and where the
  engine did not pause the game it answers nothing and the rules never hear it. Five of
  fifteen games failed the pause item so. The same holds for any engine word (`pause`,
  `title`, `start`) on a screen the rules put up.
- **The gate passed on `3bb629b5`** (2026-11-23, everything for an engine change; `search_check` and `new_project_check` passed on it).
- **The gate passed on `3207626c`** (2026-11-23, everything for an engine change), **at the second run.** The first failed on one speed budget in a crate the change does not touch (`specula-studio`, `reading_the_largest_record_is_bounded`: 565 ms against 400). Measured before running again: about 220 ms on a quiet machine, twelve times; about 750 ms with every core busy. So the machine was loaded by something outside the gate during that step; which, is not known. The budget is under twice the quiet time: thin for a laptop that had run comparisons, tests and gates since morning. If it fails again, look at the budget, not at the clock.
- **The gate passed on `ff4b1298`** (2026-11-23, everything for an engine change; the three cells ran on it).
- **The gate passed on `93c46983`** (2026-11-22, everything for an engine change; stage 1's runs were made on it).
- **The gate passed on `74007d71`** (2026-11-22, everything for an engine change; the comparison ran on it).
- **The gate passed on `c40f4732`** (2026-11-22, everything for an engine change: 3,600 tests, repeats, slow workers, speed, clippy for both targets, both builds; of the live checks, `search_check` and `new_project_check` were run and passed). Before it:
- **The gate passed on `8d172ace`** (2026-11-22, everything for an engine change: the workspace, repeats, slow workers, speed, clippy, both builds; the live checks were not run whole, `search_check` passed on the save mend). Before it:
- **The gate passed on `14c0d08c`** (2026-11-21, everything: 3,585 tests, repeats, slow workers, speed, clippy, both builds; the live checks were not run again). In it: `53985a95` (a module pushes a
  body and tells it to bounce: `Cx::push`, `Cx::place_at_rest`, `Spawn::bounce`, `friction`),
  `7f2167f8` (the 3D jobs). The mends are on the engine without the library (`C:/sf-base`,
  `4eb20e9e`), without the push words; that arm is not measured again.
- **Pushed on 2026-11-22 at the owner's word, twice: `origin/main` is `6d822aa0`** (plain fast-forwards; the cold comparison and everything before it). Anything after it is not pushed. Push only when the owner asks.
- **Part 4, the libraries and everything after them wait** behind what the owner set on
  2026-11-20, when a measured comparison showed the shape library made four props no cheaper and
  the cause turned out to be that the builders could not see what they needed ("the builders
  you've been deploying, have been doing so blind"):
  1. *The blindness mended* (done): a build says `closed` and `open_edges`; `model_schema` says a
     recipe's format, each operation and a sculpt's nodes in one answer; a new project has
     `notes/models.md` and `notes/world.md`; `model_build_all` builds every recipe with a line
     each; answers through a pipe are compact. Measured on the small job: 586,400 weighted tokens
     before, 289,637 with the brief unchanged, about 155,000 with a brief that points at the
     project's page ([docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), the results of
     2026-11-20).
  2. *Soft edges on a figure with a point or a notch* (a heart, a crescent, a lune): **both
     routes, not built.** A: the library's raise past where an inset first closes an edge. B:
     rounding through the sculpt, from the plane figures' distance fields (stage 5a is merged, so
     B can be built now). The sculpted heart in `model_schema`'s example has a crease and a
     ragged seam.
  3. *What the measurement does not show* ("Worth setting up before we continue with Part 4"):
     (a) the mends told apart from the library: the mends carried onto the engine without the
     library (`C:/sf-base`, `8b67796d`) so that arm can be measured again: **not done**; (b) more
     than two runs a cell and more than one job: the dozen-props job was done by hand (twelve of
     twelve at the first try) and **not measured**; (c) a job that needs what the engine has: 3D,
     physics and sound were done by hand from a module, cold, and what was dark was mended
     (below); **not measured**.
- **Next by hand, in order** (set before the pause; the owner has not said what follows the second comparison)**:** the mends onto the engine without the library; soft edges, both
  routes; then, when the owner allows spending, the measurements of 3 (a) to (c) with a cost said
  first.

## What is built and not reviewed, and what each review said last

A model that did not write the code reviews it; with subagents off, these wait. "Merged" means in
the working branch.

| Work | State | Owed |
|---|---|---|
| By hand, 2026-11-21 (`m6-usus`): the schema's words, the two project pages, typed `Spawn` words, a run's warnings, what physics tells a module, the contact speed's mend (snapshot version 3) | merged, gated | a first review |
| By hand, 2026-11-21 and 22 (`m6-una`): the carrying slab and its mend, the corrected pages, a run that speaks a brief and is read on its way, `Cx::param`, `specula tune` and `find`, the save that keeps waiting events, `--sounds`, `search_check` | merged, gated | a first review; `search_check` shown to fail on a seeded fault (the engine before the save's mend failed it on a builder's game, which is the evidence so far) |
| Sculpt mesher: a vertex for each sheet through a cell | merged; twelve meshes changed by a few triangles | a first review; close-ups of the changed places |
| Sculpt's box (a reach passed down the tree) | merged; **review said do not merge**: the box also grows on ordinary sculpts with nothing stretched | the mend, stopped mid-work in `C:/sr-k30` (`d8b7a407` and one uncommitted file) |
| Rounded shape's field (`roundness`) | merged; tenth review: the field is sound; "the sure way cannot fail" is false at its 64-step cap, and three rules are held by no test | small mends (`.swarmnotes/m6/part4/reviews/weights-review3.md`) |
| Faceting from the recipe | fifth round merged | its review |
| Shape language, the silhouette measure | seventh version merged | its seventh review; the owner: "Rounds continue" |
| Light report (`perf_light`) | fifth round merged | its review; one run that fell to 27 calls a second is unexplained |
| Shape library, stages 1a to 5a | merged; Claude reviewed each stage | 5b (the form as a sculpt node), stage 4 (more families) |
| A project's catalogue and the orientation an agent is given unasked | builder stopped: 18 uncommitted files in `C:/sf-formae` | finish or restart (brief: `.swarmnotes/m6/part4/formae/brief-orient.md`) |

**Until a reviewer says merge:** give no style a `max_facet_deg`; do not give `roundness` to a
shape that is thin along one axis; no preset asks for a shape language.

Seen in pictures and not looked into: red slivers in clay under `grave_rock`, a ragged fold on
`statue_mourner`'s head, one loose piece of the fidelity skeleton
(`gallery-full/part4/mesher/NOTES.md`).

## Where M6 stands

ROADMAP M6 has the fourteen parts and their order; nothing in it may be cut and no date presses.

- **Parts 1 and 3: done** (pushed with the first five passes at the first hour). Verifying faster (the gate and the live checks' runner in
  Rust, the sizing, the pin); the engine paying from the first hour (a project gets its SDK, a
  game is a file, `specula new`, `run`, `check`, memory). Six measured passes took the cost of
  building through the engine from 6.8, 4.0 and 2.2 times a plain build (small, medium, change)
  to 1.61, 1.22 and 1.47 (LEVERAGE.md).
- **Part 2** (a script is ported to Rust before it is changed, never edited): 81 scripts are
  left in `scripts/`.
- **Part 4, form within an agent's reach: in progress, paused as above.**
  - The body as numbers (`specula-corpus`, [docs/design/CORPUS.md](docs/design/CORPUS.md)):
    built and reviewed.
  - The figure's surface ([docs/design/FIGURE.md](docs/design/FIGURE.md)): built, measured
    against its body, rigged, reviewed; **it does not look right yet** (a trunk like a column,
    tubes for legs, a seam at the neck, a walk that is a lunge). Measure before looking, and look
    before saying. The owner wants a skeleton with every figure, so that a character is a change
    and not a rebuild.
  - The form views (`clay`, `normals`, `curvature`, `stripes`) and the form checks
    ([docs/design/FORM.md](docs/design/FORM.md)): built and reviewed to "merge". Look at any form
    in clay and curvature before saying what it is like.
  - Shape language from the recipe: reviewed to "merge". The rest is in the table above.
  - The light report ([docs/design/LIGHT.md](docs/design/LIGHT.md)): the engine has no quality
    preset for a machine it has never seen; it measures, and whoever builds the game decides.
  - The engine's own proofs of the sculpt's mathematics
    ([docs/design/DISTANCE_FIELDS.md](docs/design/DISTANCE_FIELDS.md)): found sound by a second
    reviewer.
  - The shape library (`specula-formae`, [docs/design/FORMAE.md](docs/design/FORMAE.md)): 65
    plane figures, 82 solids and 12 curves by name, in 2D, 2.5D and 3D; `formae_list` and
    `formae_get`; a recipe takes a form by name. It raises a figure only until an inset first
    closes an edge.
- **The research behind parts 4 and 5** is in
  [docs/research/library/](docs/research/library/README.md), with a second checker's report on
  each file; a number is still checked against its source as it is typed in. The reference
  pictures are in a public repository of their own,
  [specula-imagines](https://github.com/IronAdamant/specula-imagines); Specula links to it and
  never depends on it.

## The owner's standing decisions (their words are in the history file, by date)

- **Scope and time:** no deadline, nothing cut ("I need this done right, not rushed"). M6 is the
  proto-1.0, released under a 0.x number for **Windows and macOS** together; Linux follows. The
  shader language comes before macOS; the Mac is last, Apple Silicon only, and is the work of a
  separate instance on the Mac. The crates and the announcement go with the first release; an
  adversarial review comes before any marketing.
- **Rust only, zero dependencies** (AGENTS rules 1 and 16). No new Python, shell or PowerShell.
- **Who builds:** one builder; subagents only if necessary, and at present none (above).
- **A failure nobody can explain is investigated before the work goes on** (a fresh agent when
  agents are allowed; by hand now). When a number does not move after a fix, read the runs
  request by request. Verify use, not only correctness: do a job cold by hand before paying to
  measure it.
- **Permissions:** a permission refused to Claude is surfaced to the owner and never worked
  around. A subagent that was refused something and asks the lead to do it is refused. Removals
  of old data happen with the owner at the machine.
- **Recordings:** live runs worth showing are recorded, of the engine's window only, failures
  kept; nothing is published before the release discussion. Every asset's making is captured as
  it first comes out, looked at in the engine, mended, and captured again with its numbers.
- **Reference pictures:** whoever places a delivery looks at every picture first. No unclothed
  picture of anyone under 18 or who cannot be shown to be an adult; unclothed adults are kept
  apart; Muybridge's plates are held against his 1887 catalogue of models. Deleting a
  repository is the owner's to run.
- **The libraries** (part 5 and on): models, terrain, assets, water, grass, people with faces and
  hair, all by code, as base templates an agent starts from by name. The shape library must be
  found cheaply by an agent and its forms become sculpt primitives; a project's own recipes get
  a catalogue; the engine tells an agent where everything is, unasked.
- **For M6 and noted, not designed:** a storyline tree, reputation, an RPG set-up, captions,
  quests, music, all optional data; speech only as the project really has it; Sepulcra ships
  with its three maps, and its balance stays risky; remote agents get no native code, window
  input or module source unless a token names them; light is vendor-neutral, never dependent on
  ray-tracing hardware.
- **Kept for the release discussion:** the raw recordings of 2026-11-12. The eval numbers are
  public as they are.

## On this machine

- `D:/Coding Projects/Specula`: the main checkout, branch `m6-first-hour`.
- `C:/sf-una`: the merged line's worktree (`m6-una`), where the gate of `2da88f08` passed.
- `C:/sf-harness`: `m6-usus` (merged); its release engine was used by the comparison's `found`
  and `guided` arms and has been rebuilt since. `C:/sf-base`: the engine without the library
  (`8b67796d`), the `none` arm.
- `C:/sf-formae` and `C:/sr-k30`: stopped work, uncommitted (above). `C:/sp-audit`: the audit
  worktree. `C:/sr-k29`: the mesher's, merged, removable when the owner is at the machine.
- `C:/lev-props*`: the comparison's runs. `C:/lv/`: scratch projects of the by-hand work.
- Notes that are not tracked: `.swarmnotes/m6/part4/` (briefs, review reports,
  `blind/FINDINGS.md`, `STUMBLES.md`, `FIXES.md`). Pictures: `gallery-full/part4/`.
- **A build can fail here for memory, with words about something else** ("crate `specula`
  required to be available in rlib format", missing methods, internal compiler errors). The
  cause is one line: "The paging file is too small" (os error 1455; the page file is 4.7 GB).
  `CARGO_BUILD_JOBS=6`, or fewer programs open, and it builds.
- The comparison's harness takes ports 7681 to 7699 (7680 is held by a Windows service).

## Where things are

- **How work runs:** as **lanes**: one subagent per crate/area in a git worktree, each attacked by a
  `lane-reviewer` before merge (`.claude/agents/`), and visual claims checked by `sight-skeptic`. Check `git log`
  and the roadmap ticks for where things stand.
- **Lessons:** reviews keep finding real bugs (determinism leaks, unbounded work, device loss, stale caches), so
  keep the rounds; after round 3-4 the integrator verifies narrow final fixes. Measure with the game running, not
  only static worlds (the first draw cache was slower in play). Several sessions share the GPU and ports 7460-7499;
  stop engines by PID only. Usage limits interrupt lanes: resume each with SendMessage.
- **Game docs:** [DESIGN.md](games/sepulcra/DESIGN.md) (sim rules, numbers), [STYLE.md](games/sepulcra/STYLE.md)
  (Claude's style bible draft), in-game test plan [TESTPLAN.md](games/sepulcra/TESTPLAN.md). Capture:
  `docs/fidelity/sepulcra_slice_hud.png`. How the game was chosen: [docs/GAME_SELECTION.md](docs/GAME_SELECTION.md).
- **Measuring:** `python scripts/perf_baseline.py` (see [perf/README.md](perf/README.md)); the `perf_frame` tool
  in a running engine; `cargo run --release -p specula-gpu --example bench`; Sepulcra's headless sweep
  (`cargo run -p sepulcra --example headless --release -- --layout all --sweep 1000`); `scripts/swarm_drill.py`
  for agent throughput.
- **Scripts keep their hands off the machine's human** (owner, 2026-11-13: "I can see the agent moving my
  cursor around while I'm reading"). A script never moves the mouse pointer, presses keys or buttons for the
  person, or takes the keyboard from the window they are in: it posts messages to the engine's own window
  and uses the protocol; a pointer over the engine's window is handled by moving that window
  (`specula_mcp.clear_of_pointer`); script-started engines open with `--no-activate`.
  `crates/specula/tests/script_settings_guard.rs` holds it.
- **Recordings show the engine's window only.** `scripts/window_recorder.py` records one window as an
  animated PNG (it asks Windows for that window's contents, so the desktop and other programs can't be in the
  picture); `python scripts/eddraw_check.py --record` uses it. Never record the screen.
- **Visual history (owner, 2026-10-18, "for historical review"):** [docs/gallery](docs/gallery/README.md). After
  every live run (tests, playtests, assets, animation, physics, navigation), add 2-6 representative captures with
  `python scripts/gallery.py add ...` (`gallery.py subjects` lists the subjects; `dupes` and `drop` keep it lean). Thumbnails
  are tracked; full-size originals go to the main checkout's gitignored `gallery-full/`.
- **The plan with checkboxes:** [docs/ROADMAP.md](docs/ROADMAP.md).
- **Decisions and architecture:** [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md).
- **Hard rules:** [AGENTS.md](AGENTS.md).
- **Open owner decisions:** the one list is [below](#open-owner-decisions).
- **Design research:** [docs/design/](docs/design/) (determinism, physics, animation, procedural modelling, UI).
- **Engine survey:** [docs/research/ENGINE_SURVEY.md](docs/research/ENGINE_SURVEY.md) compares Unreal, Unity and
  Godot with Specula, lists the gaps (now in the roadmap, marked ➕), covers Unreal's stutter problems, and explains
  where Specula can actually win.
- **Rust at scale:** [docs/research/RUST_AT_SCALE.md](docs/research/RUST_AT_SCALE.md) covers how compile times,
  unsafe code and the language's strictness become strengths (Rust track in the roadmap). Measured on 2026-10-13:
  clean build 8.2 s, incremental 1.2–2 s.

## The vision, in the owner's words

> "AI agents are meant to work through this engine, building multiple parts of a world/game in the real time."
>
> "I'd like to enable myself, as well as smaller studios to compete with entire larger companies via AI agent swarms."
>
> "The swarm track is the primary. This is meant to allow games to be iterated rapidly via AI agent swarms."
>
> "For devs that just want AI agents such as yourself and your swarm to complete entire game... This is it."
>
> "I'd like it to make it easier to make games that are actually optimised to run on lower platforms, as too many games
> are made through Unreal Engine and it's caused a lot of efficiency issues."
>
> "The reason you're working on this project is because unlike earlier generations, you have taste baked in and that's
> valuable to this project."
>
> "Figure out how we can bring this engine to full production stage as soon as possible."
>
> "What they got were tacked on, and I think this will be difficult, even with AI agents to convert it from original
> human tools that have a lot of issues in them, so getting there a-fresh from the slate is the key here. And Rust is
> fundamentally as efficient as C++ and safer even for games to be made that way."
>
> "Nothing is left out. I accept how long it will take it to push to live production."

Specula is a **zero-dependency, agent-native game engine and editor in Rust**. Agents are the primary users, and the
human editor is built on the same protocol they use. The owner (GitHub **IronAdamant**) is building an original game on
it with Claude, starting now.

In practice that means six **foundations** that apply to every change from M1 onwards (details in ROADMAP.md):
- **F1 Swarm-first.** The swarm track is the primary track.
- **F2 Determinism.**
- **F3 Low-end performance.** The Intel iGPU is the reference device.
- **F4 Physics and animation designed in** from the start.
- **F5 Humans optional, never locked out.** Devs can click, point, draw and drag in a Rust editor UI if they want to,
  but agents must be able to build the whole game without it.
- **F6 Portable by design.** Mobile comes after v1.0 (the owner isn't familiar with it yet), but nothing may assume a
  desktop, x86 or a keyboard, so the port stays cheap.

## What works today

- **Run it:** `cargo run -p specula` opens a window and serves MCP at `http://127.0.0.1:7471/mcp`; Claude Code opened
  in this folder connects through `.mcp.json`. With no argument the window shows the **start screen** (the
  projects worked on, the last one first); from a shell, `specula projects`, `open --last`, `new NAME`, `list`,
  `tools`, `call TOOL '{json}'` and `stop` do the same and find a running engine
  ([docs/design/LAUNCH.md](docs/design/LAUNCH.md)). Sepulcra: `cargo run -p sepulcra-app --release -- --port 7486 --
  --seed 7` (see its TESTPLAN.md).
- **Flags:** `--port N`, `--headless`, `--adapter discrete|integrated|<index>`, `--size WxH`, `--captures DIR`,
  `--worlds DIR`, `--world NAME` (autosaves to `worlds/NAME/`), `--cache DIR`, `--assets DIR`, `--style FILE`,
  `--perf FILE`, `--settings DIR|off` (a hosted game's player settings: per user with a window, none headless;
  scripts pass `off`), `--paused` (start with the clock stopped, for replays), `--response-format concise|detailed`,
  `--tools-default core|all` (agents list 6 core tools unless they ask; `.mcp.json` and the scripts ask for all),
  `--owner NAME` / `--lead NAME` / `--reviewer NAME` (operator grants: roles are never self-declared),
  `--trace DIR`, `--trace-file on|off`, `--strict-budget` (an engine-thread stall fails the run), `--terrain on|off`,
  `--no-activate` (the window opens behind the others without taking the keyboard from the window you are in:
  the scripts pass it), `--project DIR` (the engine starts in that folder, as a project), `--run` (a project's
  modules are loaded at start, as an owner or a granted lead last built them; `--run-as-is`: whatever is on
  disk now), `--script FILE` (a headless run by a script, then the engine ends: `specula run` writes the file
  and reads the answer), `--launcher` (the start screen, as with no argument), and `-- GAME ARGS` for a hosted game. Agent
  names are plain ASCII.
- **Answers are concise by default** on the heavy tools; pass `response_format: "detailed"` (per call, or once in
  `agent_hello`) for everything. Repo scripts use `scripts/specula_mcp.py` or `specula_mcp.detailed()`.
- **Tools:** `specula tools` from a shell, or `tools_catalog` in a running engine, lists them by family with
  a line each; every tool's schema is in `tools/list`. (The list by family that stood here is in the
  history file.)
- **Rendering:** Vulkan render graph, PBR-lite and stylised materials, directional and point lights, shadow maps,
  post (tone mapping, FXAA, LUTs), sky, glass, instancing, culling, LODs, GPU skinning, 2D sprites, tilemaps and
  vector shapes, a UI overlay. Reversed Z.
- **Simulation:** fixed-tick clock, seeded RNG streams, deterministic maths (`dmath`), ordered containers, a
  journal-replay test that reproduces `world_hash` in CI.
- **Game-side crates:** `specula-anim` (A1-A3: clips, graphs, IK, motion generators and checks), `specula-audio`
  (WASAPI, procedural 3D audio, Ogg/Vorbis, streamed music), `specula-input` (keyboard, mouse, XInput gamepads),
  `specula-text` (fonts, GPU text, editing), `specula-ui`, `specula-lingua` (string tables, plural rules),
  `specula-tempus` (timelines and camera rails), `specula-model` (procedural modelling), `specula-gltf` (import
  and export).
- **Size:** about 396k lines of Rust (2026-11-14): 370k in 27 engine crates and 26k in 8 game crates (Sepulcra's
  rules and app, the Ambulo example module and five gameplay modules); 3,049 tests in the gate (2026-11-16). **Zero
  third-party crates.** The Rust toolchain's own linker; Vulkan, XInput and AVRT are loaded at runtime.

**What it can't do yet:**
- **Pay for itself at small sizes.** A medium game (menus, settings, two languages, waves) built through the
  engine costs about a quarter more tokens than the same game built without it by the same model, with the
  same checks passed; a change to an existing game about a half more; a one-screen game about three fifths
  more (1.22, 1.47 and 1.61 times, measured 2026-11-17 after six passes at the first hour; the measurement
  before, the first without a fault of the builders' shell that had flattered the engine, was 1.28, 1.25
  and 1.85, and the plain builds alone move by a sixth between two measurements; it was 4.0, 2.2 and 6.8
  the day before). What is left is the first meeting: the guide and the sample read before a line is written, which
  these sizes do not pay back ([docs/design/LEVERAGE.md](docs/design/LEVERAGE.md)). M6's work, still.
- **Tell a story for a game.** A storyline tree, reputation, an RPG set-up, captions with voices and music
  packs are M6's, noted on 2026-11-16 and not designed. (Win and loss conditions, a score, and notes kept with a
  project were built in M6 part 3.) Round and human form still costs an agent far more than boxes do (the five
  form items, also M6's).
- **Give a module-built game more than the basics of an interface.** A project with a game file has screens,
  strings by language, saved settings, a title, pause, an outcome and a score without an executable of its own
  (the project shell). Not yet: key rebinding screens, colour-vision themes, captions, and conditions that read
  the world (a condition reads what the modules keep, the settings and the run).
- Sepulcra uses physics only for its shamblers' ragdoll deaths (gameplay is still rules, not physics). It runs on its
  Rust rules by default (`--rules lex` plays it from Lex). Modules can't load off Windows yet (no `dlopen`), and a
  module stuck in a loop that makes no requests can't be preempted.
- **Windows only** at runtime: other platforms compile but report "unsupported" (Vulkan loading isn't implemented
  off Windows yet). CI has no GPU, so rendering is checked by hand with `scripts/window_check.py`.
- **Localhost only:** no per-agent auth tokens yet, so the server must never bind beyond loopback.
- **The window's own auto camera** (no camera set) still fits every object, so one far-away object shrinks it; `sight_frame` leaves outliers out and names them.

## How the owner likes to work

- **Subagents:** they asked for subagents on milestone builds, with **adversarial review before advancing** a
  milestone. They want nothing left open.
- **Pushing:** push only when asked. They usually say "push please".
- **Honesty:** they value directness and corrections. When they believed Bevy was "the first editor that works with
  Rust", a gentle correction was welcome: Bevy is code-first, and Fyrox and Godot-rs exist.
- **Their own builds:** they asked for things built by us rather than borrowed ("full control"), for example their own
  linker. The agreed compromise: the toolchain linker plus raw-dylib already removes third parties, so the custom
  linker is parked as a long-term option.
- **Naming:** Latin-style names.
- **Test steps:** in-game test plans as `- [ ]` checklists (they use Obsidian).
- **Other work:** Fallout 4 mods in `D:\Coding Projects\AgentCreationKit`. A peer Claude session there once committed
  work by mistake, so check `git log` before committing.

## On the development laptop: what never to do

These held all through M5 and were written down nowhere in the repository until 2026-11-16.

- **Push only when the owner asks**, and only what that go-ahead named. A plain push of `main`; never a
  forced one.
- **Never run `rustup update` or change the default toolchain.** It is the owner's (1.99.1 when this was
  written). The repository's `rust-toolchain.toml` pins its own version (1.100.0), which rustup installs
  beside the default and uses in this checkout only.
- **Never delete** `target/m4-exit-approved/` and `target/m3-exit/` (the exit evidence the owner
  approved), `gallery-full/` (the gallery's full-size originals, and the raw recordings of 2026-11-12
  the owner has yet to decide on), or `.specula-cache/`. A `cargo clean` in the main checkout takes the
  first two with it: build into another folder with `CARGO_TARGET_DIR` instead of cleaning.
- **Nothing private in a tracked file or a commit message:** the owner's Windows user name, e-mail
  address, home folder paths and where their diary is kept. The repository is public.
  `tools/examen/tests/private_paths.rs` holds part of this; read a commit before it is pushed. No
  session or share links (AGENTS rule 11).
- **Hands off the person at the machine:** never move their pointer, press keys for them, take the
  keyboard's focus, raise a window over theirs or touch their clipboard. Never record or capture the
  screen: the engine's own window only.
- **An engine you start for a probe** gets `--settings DIR` or `--settings off`, and with a window
  `--no-activate`. Never start the executable with no arguments in a probe: that is the start screen,
  with the person's own list of projects, and that list never goes into a tool's answer, a capture or a
  trace.
- **Loopback only.** Never bind `0.0.0.0`, even to try a port.
- **Stop engines by their process id** (or `specula stop --settings DIR`), never by name: other sessions
  run engines too, on ports 7460 to 7499 and 7471. Take a range of your own (the live checks start at
  7720).
- **A probe that writes into assets works on a copy** of them in a scratch folder.
- **Leave the owner's other repositories alone** (their seven agent tools and the rest beside this
  folder): read them, never change them.
- **Never a bare `git stash`:** the stash is shared by every worktree and session. Set work aside with a
  commit on a branch.

## Gotchas that cost time

- **`RUSTFLAGS`:** an environment `RUSTFLAGS` *replaces* the config's flags. Use
  `RUSTFLAGS="-Dwarnings -C link-self-contained=yes"`.
- **One crate per DLL:** on windows-gnu, two crates importing the same DLL via raw-dylib → the second one's imports
  silently vanish.
- **Linker on PATH:** the LLVM-MinGW gcc first on PATH lacks libgcc (the bundled linker avoids it). In Git Bash, `link`
  is coreutils, not MSVC.
- **Shaders:** compile with `--target-env=vulkan1.3`, or you'll chase phantom shader mismatches. The SDK is at
  `C:\VulkanSDK\1.4.363.0`, and `VULKAN_SDK` isn't in the shell environment.
- **Python:** `-I` ignores `PYTHONIOENCODING`, so keep script output ASCII. Scripts that launch the engine must set its
  working directory or pass `--captures`.
- **Live check:** CI has no GPU. After GPU or window changes, run `python scripts/window_check.py` and **open the PNGs**.
- **MCP tool names:** no dots, because Claude Code rejects them.
- **Disk:** drive D: is nearly full; lane worktrees each grew 8-15 GB of `target/`. Build into C: with
  `CARGO_TARGET_DIR` (lanes use one folder each), and delete a merged worktree's `target/`.
- **An executable keeps the folder it was built from.** A game finds its assets there. A target folder moved
  to another worktree still holds the old worktree's executables until they are built again: M5's final audit
  ran 17 checks into failures on such a build (no recipes, no timelines, no grass) before it built its own.
  The live checks' runner (`examen live`) builds first; do the same before any live check in a new worktree.
- **Two style files, two jobs.** `style.json` is what models are built under (materials and palette for
  recipes): the nearest one from a recipe's folder up to the assets root, or the file `--style` names. A
  recipe's `model_build` is refused until there is one, and a new project has none yet (ROADMAP M6).
  `style.toml` is the studio's measurable rules (budgets, scale, palette distance, naming, motion
  thresholds) that `style_check`, `studio_check` and the galleries read. Sepulcra has both in
  `games/sepulcra/assets/`. Writing one where the other is wanted gives an error about a file that
  looks present.
- **A file's time says less than it seems.** It moves in steps of the clock (1 to 16 ms on Linux and
  Windows, a second on some file systems), and a copy carries the copying's time on Linux and its
  source's on Windows. A cache or a test that tells two writes apart by time alone works on one system
  and fails now and then on the other. The model library waits until a time is two seconds old before it
  compares with it; tests give files their times (`models::tests::set_time`) and hold the clock still
  (`NOW`) where "fresh" and "old" matter.
- **On Windows a module's build folder can be too deep to link.** rustc hands the linker a long list of
  arguments in a file inside the build (`<cache>/modules/<name>-<hash>/debug/build/<package>/<hash>/out/
  rustcXXXXXX/linker-arguments`), and the Rust toolchain's linker opens a path of at most 259 characters. One
  past that it reads none of its arguments and fails with words about something else (`liblto_plugin.dll not
  found`). For a module named `ambulo` that is a project folder deeper than about 150 characters: rare for a
  person's project, common for an agent's scratch folder. The engine names it (`logic_build` answers
  `build_path_too_long` with the file, its length and the limit); `--cache DIR` in a shorter folder, or a
  shallower project, builds. M5's last audit met it as one failed check out of 70, in an output folder
  one character deeper than the audit before it; the runner (`examen live`) refuses an `--out` that deep
  before it starts. Give an audit a short folder.
- **A newer Rust is a change, so the version is pinned** (`rust-toolchain.toml`, 1.100.0 since 2026-11-16;
  AGENTS rule 16). Why: the push that ended M5 passed the gate here on 1.99.1 and failed CI on 1.100.0,
  released six days before: its Cargo lays the target folder out differently (`build/<package>/<hash>/out/`
  where `deps/` was), the module build service looked for a module's dep-info in the old place, and every
  module build was refused. The engine reads both layouts now. CI used to install whatever stable was
  newest; it now installs the pinned version, and a weekly job tries the newest and blocks nothing.
- **The declared minimum Rust changes what clippy asks for.** Clippy leaves out a suggestion that needs a
  newer Rust than `rust-version` declares. Moving that from 1.85 to the pinned 1.100.0 switched two lints
  on at 153 places (`is_multiple_of`; `as_chunks` for `chunks_exact` with a constant size), in code nobody
  had touched: 134 fixed by `cargo clippy --fix`, 21 by hand. Expect the same at every move of the pin.
- **Clippy 1.99 flags thread-locals that are already `const` when the standard library's sources are
  installed.** On windows-gnu, with the `rust-src` component in the toolchain (an editor's rust-analyzer
  brings it), `cargo clippy` on 1.99.1 fails `specula-core` with "initializer for `thread_local` value can
  be made `const`" on initialisers written `const { .. }`. For the Linux target, and without `rust-src`, the
  same clippy passes. The last 1.99.1 gate after M5 met it on files nobody had changed: the component had
  been added to this machine's default toolchain that evening, by something other than the gate. One
  variable shows it: with a copy of the toolchain's `lib/rustlib/x86_64-pc-windows-gnu` as the sysroot
  (`SYSROOT=<copy>`) clippy passes, and with `lib/rustlib/src` copied beside it clippy fails. Not tried:
  1.100's clippy with `rust-src` (that toolchain has none here, and CI installs none). When clippy fails on
  code that did not change, read `<toolchain>/lib/rustlib/components` before the code.
- **Text files are LF in every checkout** (`.gitattributes`, held by `crates/specula/tests/line_endings.rs`).
  Before it, a file written on this machine kept its LF until git next touched it and was CRLF in every fresh
  checkout: a guard that matched a line with its `\n` passed the gate in the lane's tree and failed in a
  fresh one. A checkout made before 2026-11-15 keeps its CRLF files until git rewrites them; tests must pass
  with either. A script that edits a tracked source and puts it back handles bytes: Python writes text as
  CRLF here, and `trace_check.py` handed `games/ambulo/src/lib.rs` back changed while its own comparison,
  made as text, passed. The runner now fails a check that leaves the checkout changed (git's status
  after each one).
- **Python on Windows reads text in the system's code page.** Every script names `encoding="utf-8"`, for
  files and for processes read as text (`script_settings_guard.rs` holds it).
- **Python's connect with a timeout, on Windows, often takes 10 to 26 ms** (a connect takes 0.1 ms). Through
  `urllib.request.urlopen(req, timeout=...)`, a connection a call, about one call in seven waited, and a
  check that timed a call read it as the engine's. Scripts reach an engine through `specula_mcp`'s `urlopen`,
  `connect` and `Connection`, which set the timeout after the connect (the same guard). When a call looks
  slow from a script, read the engine's own record of it first (`trace_call`): it said 0.1 ms.
- **Firewall:** never bind `0.0.0.0`, even to test a port: Windows asks about every new binary that listens beyond
  loopback (leases read the OS listener table instead).

## Open owner decisions

This is the **one list** of decisions waiting for the owner (other docs link here instead of keeping their own;
`tools/examen/tests/docs_links.rs` checks it isn't duplicated). Remove an item once it's decided and record the
answer where it applies. What was decided is recorded by date in [HANDOFF_HISTORY.md](HANDOFF_HISTORY.md).
Not decisions of this kind, and also with the owner: the seven items of
[docs/M5_CHECKLIST.md](docs/M5_CHECKLIST.md) that keep a part for them (none has failed), the raw
recordings of 2026-11-12 (kept, for the release discussion), an Apple Developer account (theirs to talk
through with the instance on the Mac), and a Steamworks app ID (later).

Nothing is open (2026-11-20).

(Whether the sculpt mesher may be mended where a sheet is thinner than a cell, though it
moves the meshes of three approved props (`grave_rock`, `statue_angel`, `statue_mourner`)
and three of the fidelity set by a few triangles each, was decided on 2026-11-20: "You can
mend the mesher. Yes it'll take long but the fix needs to be now." A fresh agent is making
the mend (`.swarmnotes/m6/part4/mesher/brief.md`), with the six shown before and after in
the engine and checked by a model that did not take the pictures.)

(An empty folder a reviewer made by mistake, `C:\sr-k2t`, was removed on 2026-11-20 at the
owner's word. The reviewer had been refused the permission and asked the lead to do it; the
lead does not do for an agent what the agent was refused, so it waited for the owner.)

(How long the silhouette measure of shape language is chased was decided on 2026-11-20: the
rounds go on, recorded at the top of this page.)
(Whether the two reviews an interrupt stopped are run again was decided on 2026-11-20: they
are, recorded at the top of this page with the owner's word on removals.)
(What shape language should become was decided on 2026-11-19: both a fourth silhouette
measure and the measure from the recipe, recorded at the top of this page.)
(The reference pictures' place was decided on 2026-11-17: a public repository of
their own, [specula-imagines](https://github.com/IronAdamant/specula-imagines); the three before it were answered on 2026-11-16. Both are
recorded at the top of this page.)

## Claude's opinions and thoughts

**What's strong:**
- **Zero-dep suits agents.** Every line is in this repo, so an agent can read and fix anything, with no version churn
  and no hidden behaviour in dependencies.
- **Sight from the engine's own data.** Agents check images against the engine's packets instead of trusting their
  own perception. That carried over from the Grok-Bevy research and is the project's most distinctive idea.
- **The base is solid.** Validation-clean on two GPU vendors, structured errors throughout, and three milestones
  that each survived several adversarial review rounds.

**What worries me (honestly):**
- **Scope.** Zero-dep means writing a TrueType rasteriser, a Vorbis decoder, physics, skeletal animation, a UI toolkit
  and three platform backends. Agents make that feasible, but it's still years of work. The owner knows and accepts
  this. Keep every feature in the plan, but **let the game decide the order within each milestone**, and never break
  a foundation (F1–F6) to go faster.
- **Coordination is the real bottleneck for swarms, not rendering.** That's why M1 (claims, versions, journal) matters
  more than pretty pixels, and why I added a swarm track to the roadmap.
- **Content is where small studios lose.** The biggest leverage is M4: procedural modelling tools, `style.toml` hard
  checks and contact sheets. These let agents produce on-style assets while the owner approves in batches.
- **The human stays the director.** Swarms multiply output, not taste. The engine should make the owner's judgement
  cheap to apply: style rules, review queues, and a journal showing which agent built what.
- **Competition is real.** Unity and Unreal shipped MCP servers in 2026, so "agents can drive the engine" is no
  longer unique. Specula wins only on what bolt-ons can't retrofit: swarm concurrency, verification as data,
  deterministic headless playtests, an engine small enough for agents to read, and efficiency by default. Guard
  those six advantages (engine survey §1).
- **Tokens are a cost.** Every tool response should stay bounded. `max_subjects` was the first example of this rule.

**How M4 ran (2026-10-28 to 2026-10-30), and what to keep for M5:**
- **The process changes held:** the one write policy, an integration check after every 3-4 merges, every reviewer
  probe becoming a permanent test, at least one independent review per lane, and dogfooding in the real game.
- **Recurring bug classes were fixed structurally,** not one at a time. Investigators reproduced each class, then a
  structural lane removed it: authority and jobs (AGENTS rule 14), bounds, workers and traces (rule 15). The
  checklists in the lane brief come from them.
- **The lead picks each agent's tools** (`tools_scope`). Agents start with the 6 core tools.
- **Traces and incidents first:** a debugger agent watches `trace_incidents` beside builders, and files critiques.
- **The final audit still found bugs between lanes** (a reply read without waiting, a call that made two commits, a
  field read after it moved). Keep the final audit.
- **Disk:** delete a lane's target folder when it merges, and build with `CARGO_INCREMENTAL=0` when space is short.

**How M5 ran (2026-11-06 to 2026-11-15), and what to keep for M6:**
- **One builder, reviewed by models that did not write the code,** held up: no lane merged on its first
  review, and the last three reviews each found something a test suite of 3,000 could not (a lockout the
  builder's own new error message led into, judges that disagreed, a write-up that said more than its data).
- **Let an agent that has never seen the engine use it, early.** A pilot and fourteen runs by cold builders
  found in a day what months of use by an agent that knew the engine had not: a tool's arguments could not
  be read from a shell, a granted lead could not lead from a terminal, a project could not get its SDK.
- **Measure before tuning, and keep the measurement.** `examen leverage where` says where a builder's tokens
  went; run it again after every change meant to make the engine cheaper to use.
- **Read exit codes, not the end of a log.** Twice a commit went in over a failing guard because the last
  lines looked fine; commits are chained on the exit codes of the suite, the guards and clippy.
- **A check's own figure may not mean what its name says.** The harness's token count for a subagent was its
  context's size; a subagent's "no other context" was untrue. Look at the record.
- **What only CI runs is found by CI, one push at a time, unless it shows everything at once.** M5 was
  pushed green on this machine and was red three times on CI: first a newer Rust; then what that failure
  had kept from running (three text tests that had never run on Linux, behind a package that failed first)
  and a test that fails once in three runs on a busy machine, named without its message; then, in the
  first run that showed the whole workspace on Linux, one test that took two events' times to differ
  (they were one microsecond there, in one run of three). CI now runs every test binary (`--no-fail-fast`)
  and the gate ends with what each failed test said. No machine here runs Linux: until one does, CI's
  Ubuntu job is the only run of the `cfg(not(windows))` side, and it has no CJK font and no GPU. **A test
  that passes here can still depend on this machine's clock or speed:** read two times as possibly equal,
  and a file another thread makes as possibly not yet written.

**Recommendations for the next session:**
1. **M6 is under way** (the scope talk of 2026-11-16 is at the top of this page). Work in ROADMAP M6's
   order; nothing in it may be cut, and no date presses. The owner described M6 (2026-11-14) as
   finalising and tuning for the first release, with testing and adversarial checks, the experience
   judged for agents as well as people, on Windows and macOS.
2. **Still owed before the release:** the adversarial review before any marketing, and the release
   discussion, where the raw recordings of 2026-11-12 are decided. The list of open decisions below was
   emptied on 2026-11-16.
3. **Keep Sepulcra the dogfood, and add a second game built by a cold agent** as the measure: Sepulcra shows
   what the engine can hold, the comparison shows what it costs to start.
4. **Before any non-localhost bind**, build per-agent auth tokens (ROADMAP swarm track). Before CI can check
   rendering, Vulkan loading must work on Linux (Platform track).
5. **Walk the owner through the research that is still unreviewed** when there's a natural pause: the engine
   survey, Rust at scale, and the note on form and light (ROADMAP, "Design research"; the five design docs
   were reviewed by delegation on 2026-10-22).
