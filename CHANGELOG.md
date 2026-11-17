# Changelog

All notable changes to Specula are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — 0.3.0: native engine, M0–M5

Specula is now a zero-dependency, agent-native game engine (Rust `std` + OS/driver FFI only).
See [docs/ENGINE_PLAN.md](docs/ENGINE_PLAN.md). Item-by-item detail, with the reasoning for each, is in
[docs/ROADMAP.md](docs/ROADMAP.md); the timeline of decisions is under [Project history](#project-history) below.

### M6, under way (from 2026-11-16)

The proto-1.0. Its scope was talked through with the owner on 2026-11-16 (the answers are at the top of
[HANDOFF.md](HANDOFF.md); the fourteen parts, their order and their likely sizes are in
[docs/ROADMAP.md](docs/ROADMAP.md), M6).

- **Part 1, verifying faster:**
  - **The Rust version is pinned** (`rust-toolchain.toml`, 1.100.0), is the declared minimum in place of
    1.85, and is what CI installs, by number; a test holds the three together. A weekly job tries the newest
    stable from nothing and blocks nothing. Declaring the pinned version as the minimum switched two clippy
    lints on at 153 places in code nobody had touched (`is_multiple_of`; `as_chunks` for `chunks_exact`
    with a constant size): all taken.
  - **The gate and the live checks' runner are Rust** (`tools/examen`, `specula-examen`), the first of the
    repository's tooling to leave Python (owner, 2026-11-16: "The only programming language used here should
    be Rust"). Each was proved beside its Python original before that went: the same reading of every kept
    log, the same report of a failure put in on purpose, the same plan for all 70 runs of the live checks.
  - **The gate is sized to what changed**: documents that no test reads take seconds, tests and tooling take
    the tests once and clippy, and the engine, a game, or any path the tool does not know takes everything.
    `--all` runs everything.
  - **CI** runs the document guards on every push and the builds only when something else changed, keeps
    its build folder between runs, and builds without debug information (measured on the development
    laptop: a workspace build from nothing in 337 s and 2.6 GB, against at least 442 s and 9.7 GB with it).
    None of this has run on CI yet.
  - **The live audit through the Rust runner: 70 of 70** (2026-11-16).
- **Part 3, the engine pays from the first hour** ([docs/design/FIRST_HOUR.md](docs/design/FIRST_HOUR.md)):
  - **A project gets its SDK from the engine** (`specula sdk`; the engine carries the sources and trusts
    them by content), and **a module builds in a folder of any depth** (the link's path length is known
    before the build, and a short build folder is taken when it would not fit).
  - **A game is a file.** `game.json` (`specula-ludus`) says the title, the modules, the settings, when a
    run is won or lost and what its score is. A project with one, and no executable of its own, is hosted
    by **the project shell**: a title, pause, an outcome, a score, screens and words that are the project's
    data, strings by language, settings a player keeps. `game_state` reads it and `game_act` presses a
    button's word for an agent.
  - **A project remembers** (`specula-memoria`): notes with a kind and the paths they are about
    (`memory_note`, `memory_get`, `memory_list`, `memory_why`), one replacing another, and a brief that an
    agent's first call answers with, said to be the project's words and never the engine's.
  - **`specula new` makes a project that builds and plays**: a sample game, its module, its screens and
    words, a check, and notes that say how it is built and checked (`--bare` for an empty one).
    **`specula play NAME`** (`--run`) loads a project's modules at start, as an owner or a granted lead
    last built them; sources changed since wait for a build (`--run-as-is` runs what is on disk).
  - **The first live check written in Rust**, `new_project_check`, and **the comparison's harness in Rust**
    (`examen leverage`; `scripts/leverage.py` is gone).
  - **Reviewed by two models that did not write it; both said not to merge, and why** (`--run` built a
    builder's unreviewed code; an agent could save, load and change the player's settings file through
    `game_act`; a save of the sample game could not be loaded; two notes written at once could replace each
    other; the gate could record a commit it had not measured). Each is fixed with a test that holds it.
  - **Reviewed again, and again not to merge.** The fix for `--run` covered the module's folder alone: a
    builder still got code loaded through a helper crate beside it, and a project root's `.cargo/` ran a
    program under it. `--run` now loads nothing compiled from outside a module's folder and holds the root's
    build configuration to what a lead built with. A ring of notes could still be written by an undo, an edit
    on disk or a padded front matter: the notes' reader no longer follows a replacement that closes a ring,
    so no way of writing hides a note. `time_control`'s step said queued ticks were dropped. And a test of
    M5's that failed once beside three compilers asserted how many of two steps fit `ui_act`'s 20 ms: it
    runs them by as many calls as that takes now.
  - **A third round, narrower.** The earlier fixes held against the reviewers' own attacks. Found: `--run`
    took a library from the build folder inside the project, which a project can bring with it (it builds
    in the operator's own folder now; an owner's own build still trusts the project's cache, said in
    SECURITY.md and on the roadmap); a thousand files that are not notes pushed every note out of the list
    (only notes count now); a second test with the two-steps-in-20-ms flaw; a compiled file in a dot-folder
    that no digest covered.
  - **A fourth pass: both reviewers say merge.** Its last findings were small and of this branch: a module's
    own `Cargo.lock` could be included without being digested; the check at a note's write could be blinded
    by a thousand files that only began like notes (it reads notes exactly as their reader does now: one
    reading of the front matter for both); and my helper for the two timing tests gave a wait too few calls.
  - **The comparison, run again: the engine arm did not get cheaper** (2026-11-16;
    [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), "Results (2026-11-16)";
    [perf/leverage/2026-11-16](perf/leverage/2026-11-16/report.md)). Twelve builds on `2a0c5669`, all
    judged, checks passed alike. Through the engine: 6.8 times the plain arm's tokens on the small job
    (M5: 5.2), 4.0 on the medium one (3.1), 2.2 on the change (1.7). With two runs a cell only the medium
    rise is beyond what two runs alike have shown. The builders used what part 3 gave them (the game
    file, the shell's screens, a tick of their own) and took more requests doing it; no builder called a
    memory tool or wrote a note; the games with menus were checked by scripts that step the engine call
    by call. What follows from it is a roadmap item ("The first hour, measured again"), not built.
  - **Every token of those runs, laid out** (`examen leverage trace RUN`; the owner, on the result:
    "Note down every token spend. Assess"): each request's context, what it wrote, its cost and what
    it left every later request to carry, with its steps and files; both ledgers are in each run's
    exported file. The assessment is in LEVERAGE.md ("Where every token went"): a third to two fifths
    of an engine build was its builder writing and debugging a headless-check script of its own.
  - **The second pass: `specula run`.** One command builds a project's modules as they are on disk,
    plays its game with no window by a script of inputs (a word at a tick: a press on the interface,
    a module's action held until the next input, or `none`), prints the state as the game file's new
    `report` shapes it, writes a frame through the game file's new `view`, and ends. The game's clock
    ticks only while the game has it running, and no time of the machine becomes a tick: the same
    seed and inputs print the same state and the same frame. `--settings FOLDER` keeps the run's
    player settings. Left and Right step a setting on its button. `notes/how.md` in a new project is
    one page that answers what the builders looked up. Found on the way: a job owned by the
    operator's in-process session was dropped as a closed session's, one run in four (a run's frame).
  - **Reviewed by a model that did not write it: not to merge, and why.** An agent that reached a
    run's port could resume the clock, and the run printed a state its script had not played, with
    no word of it; an action that is not a button failed a run half way. Both fixed (`time_control`
    refuses all but `info` under a script, and a run that loses ticks fails; an action takes a
    value, `move=1/0`, and words joined by `+` are held together), with the seven smaller findings:
    a seed too large for a JSON number, an input after the last tick, the scratch folder's name,
    a project open in another engine, a wrong example in the guide, and what the live check did
    not hold (two runs' frames are compared byte for byte now).
    A second look: merge, with one more finding, fixed: two runs of one project begun in the same
    moment raced for its lock, and one failed (it waits for the other and starts again now).
  - **The second pass, measured** (2026-11-17, on `bbe9fa36`;
    [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), "Results (2026-11-17: the second pass)";
    [perf/leverage/2026-11-17-pass2](perf/leverage/2026-11-17-pass2/report.md)). Fourteen builds, all
    judged. Through the engine the small game costs 1.7 times the plain build (6.8 the day before; M5:
    5.2) and the medium one 2.6 times (4.0; 3.1), for the same checks passed: 16 requests a small build
    where there were 36 and 68, and no builder wrote a check script of its own. The change to an
    existing game got dearer, 3.9 times (2.2; 1.7), with 8, 8 and 7 of 9 checks against 9 and 9. Every
    request is laid out again, and the third pass is built from what they still spent.
  - **The third pass: a run that says what went wrong, and a player that reacts**
    ([docs/design/FIRST_HOUR.md](docs/design/FIRST_HOUR.md), section 10). From the second pass's
    ledgers: two of three medium builds had a module whose tick failed every time, and `specula run`
    printed the state of a game whose rules did nothing; their builders spent fifteen and thirty-four
    requests finding one line the engine had. A run whose module failed now answers `module_failed`
    (the first failure whole, with its tick, and how many) and never a state; what the rules log is
    said beside the state; a run's answer is one line whatever it is. `specula run --follow` reads a
    step a line from standard input and prints the state after each, so a small program that reads
    the state and answers it plays a whole game in one run (one builder had started 347 engines to
    do that). The guide says what was looked up: the sources most games bind, what the SDK's
    calls give back, how things that come and go are drawn. And a choice setting's value is shown
    in the player's language (`setting.NAME.text` reads the line `setting.NAME.VALUE`): the sample
    game's difficulty has its button and its words. Reviewed twice by a model that did not write
    it, not to merge both times: a refused step let go of a held input, a module's own prints
    landed among a followed run's states (they travel on a channel of their own now), a failure's
    tick was the game's clock and not the script's, a line of any length was read whole; each
    fixed and held by a test or the live check. From the second pass's judges: a run with no seed
    is the run with seed 1, the title's name is a line of the strings, and a game's menu shows
    which button is chosen from its first frame.

  - **The third pass, measured** (2026-11-17, on `6cfff79b`;
    [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), "Results (2026-11-17: the third pass)";
    [perf/leverage/2026-11-17-pass3](perf/leverage/2026-11-17-pass3/report.md)). Fourteen builds, all
    judged. The medium game costs the same through the engine as without it (219,007 against 217,869
    weighted tokens; 2.6 times the pass before), and every small and medium game of both arms passed
    every check. The small game costs 1.5 times (1.7) and the change 1.5 times (3.9), each engine
    change build failing the one check that all three failed the pass before. No build met a module
    that failed; seven of eight engine builds played their game by reaction through `--follow`; the
    change builds took minutes where they had taken half an hour.
  - **The fourth pass: a game's own checks, every run in one command**
    ([docs/design/FIRST_HOUR.md](docs/design/FIRST_HOUR.md), section 11). What the third pass's ledgers
    leave is checking: once its code is written a builder runs its game one run a request, a third to
    nearly half of a medium build. `specula check FOLDER` plays every run the project's
    `checks/runs.json` lists (ticks, inputs, a seed, a frame, settings that checks share by name) and
    holds each state against what the check expects (a value, or one of `ne lt lte gt gte has len`): a
    line a check, then a count, exit code 1 when one did not hold; a check that expects nothing prints
    its state. `specula new` writes the sample's own seven. Reviewed by a model that did not write it:
    not to merge (a frame could be written through a link out of the project or over a file of it; a
    player's own settings reached checks that named none; a misspelt key made a check that could not
    fail), then merge.

  - **The fourth pass, measured: no build cheaper** (2026-11-17, on `d18e1131`;
    [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), "Results (2026-11-17: the fourth pass)";
    [perf/leverage/2026-11-17-pass4](perf/leverage/2026-11-17-pass4/report.md)). Fourteen builds, all
    judged. The medium game 207,057 weighted tokens against 189,808 (1.1 times; 19 of 19 checks in
    each engine build, 17 and 18 in the plain ones), the change 145,347 against 97,842 (1.5 times),
    the small game 208,860 against 94,670 (2.2 times, from 1.5). Every engine build left a checks
    file and ran it, after playing its game a run a request as before: the checks came on top, and
    on a one-screen game they are the rise. The change builds ran the checks that came with their
    game and added to them. With nothing changed for it the plain arm moved by 1%, 13% and 20%
    between the two passes.
  - **A range in a check** (`e8097c2a`). A builder of the fourth pass wrote
    `"paddle_x": {"gt": 8.49, "lt": 8.51}` and the check said 8.500000000000009 did not hold: an
    object of two comparisons was read as a value to equal. Several comparisons of one key must now
    all hold; the guide gives the range (the builder had ended by pinning the drifted number).
    Reviewed by a model that did not write it: merge, with six small points, all taken (`648cb757`). The
    guide said numbers are equal within a rounding where only a value and `ne` are (`lt lte gt gte`
    take the number as it is: a range leaves a margin). An object that mixes a comparison with
    another key (`{"gt": 8.4, "max": 8.6}`) was still read as a value to equal: it is refused when
    the file is read, with the key. A key written twice in an expected object held against the
    first of the two: refused. One new test line could not fail, and one branch could not be
    reached.

  - **The fifth pass: a subtraction** ([docs/design/FIRST_HOUR.md](docs/design/FIRST_HOUR.md), section 12;
    owner, 2026-11-17: "if Pass 4 didn't help, how do we fix this?"). The fourth pass's ledgers put the
    engine arm's extra cost in requests and the context each reads again, at every size; not in code
    (a quarter less than plain on the medium game) and not in thinking. So nothing is added. The guide
    no longer asks a first build for a checks file or proposes a reacting player; it names what is
    read with it, and `specula new` answers the same; a setting the rules read is said as its four
    places, and where a player's settings are kept is said. In the comparison: the engine arm's brief
    is shorter and gives one read; a change builder is no longer told the finished game it is handed
    is "a small sample game" to replace; and both arms are told to write files with their file tool,
    since every build of every pass had its first shell here-document refused and wrote its first
    file twice, which cost the plain arm more than the engine arm and flattered every ratio so far.
    A `large` job is written as data and not run: a game made of modules cannot yet keep a best
    score or a saved run for its player.
    Reviewed by a model that did not write it: not to merge. A game that never touched `checks/`
    would hand on the sample's seven checks of a tap game, and the guide's new line sent the next
    builder to run them first: the guide's first lines and the brief now say to delete them or make
    them the game's. Also: the `setting` event comes at a run's start and on a change during one
    (not on any change), and the large job's brief said too little in six places.

  - **The fifth pass, measured: the first measurement in a clean shell** (2026-11-17, on `9450e0f4`;
    [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), "Results (2026-11-17: the fifth pass)";
    [perf/leverage/2026-11-17-pass5](perf/leverage/2026-11-17-pass5/report.md)). Fourteen builds, all
    judged; no build wrote a file twice. The small game 132,122 weighted tokens through the engine
    against 71,322 (1.85 times); the medium game 196,431 against 153,572 (1.28); the change 141,703
    against 113,478 (1.25; two builds of three level with plain). Without the shell's fault the
    third pass comes to about 1.7, 1.1 and 1.5 and the fourth to 2.7, 1.26 and 1.5, by their
    ledgers: so the subtraction took back the fourth pass's rise on the small game, helped the
    change, and did not move the medium game. An engine build has spent 49,000 to 74,000 before its
    first file and a plain build about 6,000; after it the engine arm is the cheaper one on the
    medium game, by the means.
  - **The sixth pass, measured** (2026-11-17, on `57f515d0`;
    [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), "Results (2026-11-17: the sixth pass, the
    first meeting)"; [perf/leverage/2026-11-17-pass6](perf/leverage/2026-11-17-pass6/report.md)).
    Fourteen builds, all judged, in the fifth pass's clean shell. Through the engine: the small game
    111,938 weighted tokens against 69,612 (1.61 times; it was 1.85), the medium game 180,729 against
    148,684 (1.22; 1.28), the change 140,412 against 95,452 (1.47; 1.25). The engine builds are 15%
    and 8% cheaper on the two new games and unmoved on the change; that ratio rose on its plain
    builds, which cost 16% less with nothing changed for them. Before its first file an engine build
    spends 40,000 to 57,000 (49,000 to 74,000 before), and three of five write it a request sooner.
    All five new-game builds used `cx.random_restart`, none wrote a generator, none wrote a checks
    file, nobody looked up the volume. All three engine change builds passed all nine checks, for
    the first time in five measurements. Still there: a second look at the sample's other four
    screens in every build, and one medium build that drew its pieces at its background's depth
    and shipped frames that do not show them.
  - **The sixth pass: the first meeting** ([docs/design/FIRST_HOUR.md](docs/design/FIRST_HOUR.md),
    section 13; owner, 2026-11-17, shown where the gap is: "That gap needs to be looked at and fixed
    up"). An engine build spent 49,000 to 74,000 weighted tokens before its first file, in four
    requests, three of them reading. So: the guide, `specula new` and the comparison's brief name
    one read of six files and say it is the reading done; **`cx.random_restart("stream")` puts a
    random stream back at its beginning** (the request `rng_restart`, undone with a call that
    fails), which a restart "with the same seed" needed and the SDK could not say, so that seven of
    the eight one-screen builds of four measurements had written a generator of their own; the built-in
    volume's range is said; the checks' description is a note of its own and no longer in the
    first read; the sample's rules file lost a header that repeated the guide. **The SDK
    changed: a project made by an earlier engine runs `specula sdk` once** (its next build says
    `sdk_differs` until it does). Reviewed by a model that did not write it: merge, with eight
    small points, all taken (the judge's brief pointed at the guide for how a check is written;
    a change builder's one read took in the look's 1,450 tokens; the guide described a file it
    had just said to delete, and said "every call" where it shows the ones a game of this size
    needs).
  - **Part 4, slice 2, in progress: round form in a few rows, and a first person**
    (2026-11-17; [docs/design/FIGURE.md](docs/design/FIGURE.md),
    [docs/design/PROCEDURAL_MODELLING.md](docs/design/PROCEDURAL_MODELLING.md), tier 3). Not
    finished: no rig, no review yet, and the figure does not look right yet.
    - **For any sculpt:** `round_cone` and `chain` (a line of points with radii: a limb, a tail, a
      horn), `column` (sections of a breadth and a depth along a line, eased from one to the next),
      `stretch` (a scale for each axis), and a near-union (only what is within the smoothing of the
      nearest takes part).
    - **The mesher does the work that is near.** For each block of samples, then each half and each
      quarter of it, the tree is pruned of what is too far to matter there, exactly (a test compares
      9,600 points bit for bit, colours included, and a cut foot besides). A figure's body costs
      2,197 work units a sample where every bone at every sample is 7,135. Measured on the way: an
      evaluation takes about a quarter of its weight in nanoseconds on this machine; the weights
      were left as they are.
    - **`figure`:** a part of a person (body, head, hands, feet, or the whole on one coarse grid)
      from a sex, a stature, a build or a weight, an age and traits. Sizes are the survey's; a
      finger's bones are in the proportions of a radiograph study, cited as facts; what lies
      between the measured places is the engine's own and is judged by looking.
    - **Eight looks in a day, the bad ones kept** (the owner, the same day: "Record the creation
      of bodies, hands, faces... if they look bad, then try to fix it please"): tube mannequins
      with a broken cone for a hand; six parts with real hands and feet and a stepped barrel of a
      trunk; the first that reads as a person; two tries at the shoulders that made a shelf with a
      pipe on it; a muscle from the neck to each shoulder; the ankle's ledge, found by measuring
      pixels after blaming the light. A model that did not take the captures did not pass the
      sixth as it had been described, and the record says so.
    - **Eight bodies in a row** (the eighth look): slender, heavy, older, athletic, long-legged,
      broad, beside the two typical ones, each from the same shape with other numbers, and each a
      different person. The body is now cut where a fine part begins, so no surface is made twice.
    - **The distance-field weights are halved**, after three measurements on the reference laptop
      (a sphere 23 to 34 ns where its weight said 107); each is still one and a half times the
      slowest measurement. A figure of six parts had sat at the build's limit.
    - **Asked of the owner's researchers:** reference pictures to hold the figures against
      ([docs/research/REFERENCE_IMAGES_BRIEF.md](docs/research/REFERENCE_IMAGES_BRIEF.md)).
    - **They came the same evening**: 110 pictures, all from Wikimedia Commons (88 public domain,
      3 CC0, 19 CC BY), indexed in
      [docs/research/library/reference-images/](docs/research/library/reference-images/README.md).
      The files are in a public repository of their own,
      [specula-imagines](https://github.com/IronAdamant/specula-imagines), with the unclothed plates kept apart there; what to gather
      next is [docs/research/REFERENCE_ROUND2_BRIEF.md](docs/research/REFERENCE_ROUND2_BRIEF.md).
    - **The figure is measured against its body.** A test builds the body's mesh, cuts it with a
      level plane at each height the survey measured people at, and compares breadth, depth and a
      tight tape's girth with the body's own rows. It found what eight looks had not: chest and
      waist 5 to 7% shallow, a man's hips 6% wide, a thigh up to 12% thin, the legs parting 5 to
      8 cm low. Mended: every breadth and depth is now within 3%, and the girths of the buttocks,
      thigh, calf and neck within 5%. Not mended, and held loosely with the reason: a tape round
      the waist reads 6% short and round the chest a tenth to a fifth short (the trunk's sections
      are too round).
    - **A column's sizes follow a monotone cubic through its sections.** They eased in and out of
      every section, which is level at each: every limb of the first figure had a ring at every
      section. A column's weight goes from 78 to 86 (54 to 56 ns a sample, measured three times).
    - **A thigh's top is an ellipse centred on its flesh**: two lie side by side within the hips'
      breadth. A wider join of legs and trunk, tried first, looked better and measured worse (it
      filled the crotch in by 10 cm) and was taken out.
    - **A figure has a skeleton, by construction.** A recipe's rig can be the measured body
      (`"rig": {"figure": {...}}`): eighteen bones under the names the motion tools look for,
      at the body's own joints, inside the surface (a test holds it). A walk generated from the
      bones' names alone moves the figure. Its first steps showed that the pelvis tears (a
      vertex binds to the nearest bone, and the hips' sides are nearer a thigh): not mended.
    - **Fixed: a rig with no anims file of its own was filmed in its bind pose.** `anim_filmstrip`
      of a recipe drew every tile alike while the same clip, written into the world, moved the
      model there. A model a caller poses is now drawn skinned whenever its meshes carry skin.
    - **A man's chest is the plain column**: a third try at a swell for the muscle read as one
      mound to an independent check of the eleventh look, which did not pass it. What it listed
      is in [docs/design/FIGURE.md](docs/design/FIGURE.md).
  - **Part 4, slice 1: the body as numbers** (2026-11-17; `specula-corpus`,
    [docs/design/CORPUS.md](docs/design/CORPUS.md)). The owner's rule for part 4 was tables first,
    from published measurements, "looked up, not written from an agent's memory".
    - **Computed, not typed.** `examen corpus tables FEMALE.csv MALE.csv` reads the two public
      data files of the US Army's survey of 2010 to 2012 (1,986 women and 4,082 men on 93
      dimensions; public domain; fetched by whoever runs it, never in the repository) and prints
      the crate's table: for each of 114 dimensions and each sex the mean, spread and percentiles,
      its line on stature, its plane on stature and weight, and what each trait adds. A test
      holds 22 dimensions against the survey's printed report (means and spreads to its last
      digit, percentiles within a third of a millimetre), and 8 fitted lines against the
      library research, which had computed them separately.
    - **Fourteen named traits** tell bodies of one stature and weight apart: age, legs, arms,
      shoulders, hips, chest, waist, neck, biceps, thighs, calves, head, hands, feet. Each is
      what is left of one dimension after stature, weight and the traits before it, so they are
      uncorrelated in the people measured, and each moves every dimension as it does in them
      (long legs shorten the trunk; big hands come with big feet; age widens the waist).
      Whatever adds up on a person adds up on a body.
    - **Joint centres and masses** are de Leva's (1996), every number read on the image of the
      printed page: where each centre lies from its landmark, what share of the body each of
      sixteen parts is, and where its weight sits.
    - **Two checks over a frame of 29 joints**, for a body made here or any figure's: its
      lengths against people of its own stature, in the survey's spreads; and how far a still
      pose's centre of mass is inside what its planted feet cover.
    - **What a test found while it was written**: the first limits (so many standard deviations
      of stature and of weight) allowed a man of 1.48 m and 21 kg. A body's limits are now the
      shortest and the tallest people actually measured, and for weight the lowest and the
      highest body mass index among them.
    - **What its review found** (a model that did not write it; it reproduced all 228 rows by a
      route of its own and held every typed digit to the page images, and none was wrong). Do
      not merge, for three things, all fixed. Bodies inside the limits could have the navel
      level with the ribs or over them when many traits were pushed at once (893 of 73,728
      corners for men): a body is now checked as it is made and refused by the part that gave
      way, and a test proves that within 4 standard deviations together it cannot happen, by
      finding each gap's least over the whole range and not by sampling it. The checks changed when a hand or a foot bent:
      they are measured by way of the knuckle and the ball. And the typed mass table was held
      by one sum, so that six mutations (two of the table, four of the code and the constants
      that read it) passed every test: it is now held against the research's own typing of it,
      and against the paper's second way of saying five of its rows. Its second look: merge,
      with nothing returned folded out of 589,824 corner bodies and 400,000 random ones, all six
      mutations caught, and five small points, all taken (an age can fold a body and no test
      said so; traits set one by one are checked one by one, so `with_traits` checks once;
      three sentences said more than the data).
    - **Said plainly**: adults of 17 to 58 only; the frame is flat (no spine curves); the hip
      joints' spacing rests on a paper that was not opened (Bell and others 1989, as the
      motion-capture literature gives it), and the ankle's place along the foot on one
      hand-measured survey. Neither is hidden: both are in the design note's table of sources.
  - **The library research is in** (2026-11-17; [docs/research/library/](docs/research/library/README.md)).
    What the brief of 2026-11-16 asked for, researched by Grok at the owner's request over two days:
    45 topics (the body, faces and hair, animals, machines, plants, terrain, water, motion, prior
    art), each a file of tables with every number's source, page and link, and each with a second
    checker's report row by row. Kept as delivered. Claude's review of it
    ([REVIEW.md](docs/research/library/REVIEW.md), seven readers, about forty numbers fetched again
    from their sources, none wrong): which tables to build from first, which are thin, and what the
    licences ask (facts cited, no table copied whole from a non-commercial source, no game's list
    of controls mirrored, several named models and datasets never used). The owner: "Feel free to
    advance into Part 4 and onwards now." The privacy guard learned that a UUID inside a link or a
    DOI in that folder is a citation (23 files cite a university's repository or a database so);
    every other private pattern is refused there as everywhere, and a link to claude.ai is never a
    citation.
  - **The first push of M6, and what only CI could show** (2026-11-17; `origin/main` = `808f54b9`, a
    fast-forward of forty commits: parts 1 and 3 with the five passes, pushed at the owner's "at your
    discretion" after the full live audit passed 70 of 70). Part 1's workflows ran for the first time
    and held: the documents' job, the commit-message guard, the sizing, both Windows jobs and the
    Android check passed. The Ubuntu job failed one test of part 3
    (`a_module_too_deep_to_link_under_the_cache_builds_in_the_short_folder`): it wrote its folders as
    Windows paths, which are one name on Linux, so the module's build folder was named after the
    whole text. The function under test is the same on both systems; the test now makes its folders
    of parts. (No machine here runs Linux: CI's Ubuntu job is the only run of that side.)
  - **The launcher's live check is Rust** (2026-11-17; `launcher_check`,
    `tools/examen/src/native/launch.rs`). The live audit after the passes passed 69 of 70: the
    launcher's check failed on an expectation that had gone stale in part 3 (a project made from the
    start screen now opens on its own title screen; the check expected a window with no text). Its
    script was Python, and the owner's word on changing one line of it was "I prefer all to be Rust
    rather than having Python": a script that needs any change is ported first. The port tries every
    expectation the script tried (84 with the three it adds), runs on to its end when one does not
    hold, and ends every engine listed in its own per-user folders whatever happens. For the engine's
    own window it has `specula_platform::probe`: a window found by its process, pictured as the
    window rendered itself (never the screen), and sent keys as posted messages, with nothing that
    moves the pointer or takes the keyboard; `gdi32` gets its one owner (AGENTS rule 2). Proved by a
    seeded fault (an opened project put last in the list: 8 of 84 fail) and its pictures checked by
    a model that did not take them. `launch_check.py` stays, unchanged and not run, because
    `m5_exit.py` takes a class from it.
    Reviewed by a model that did not write it: not to merge. **No guard held the hands-off rule
    over the new code**: the guard read Python scripts only, so a call that moves the pointer or
    takes the keyboard could have been added to the port and every test would still pass. The
    guard now reads the tooling's Rust and the probe, and holds the probe's system calls to a
    list. Also taken: an engine is not started on a port something else serves; the engine
    started by hand is ended by an interrupt again (`specula_sys::interrupt::ask`: the script's
    Ctrl+Break, which the first port had replaced by `specula stop`); a capture's packet must
    have been read before "names no project" holds; an empty list is exactly that; the release
    executable, as the script ran; pictures are waited for, bounded, where fixed sleeps were; a
    folder used before is refused. 85 expectations.

### M5, done (2026-11-06 to 2026-11-15)

Built by one agent, lane by lane, each lane reviewed by a model that did not write it and passed through
`scripts/gate.py` (owner, 2026-11-12: one builder; 2026-11-12: a gate must not pass by luck).

- **Exit proof** (`scripts/m5_exit.py`): assets are made once, saved, and are there to be used again, 2D and 3D,
  in a project that is not this repository. A project is made from a terminal; agents put a prop, a rigged
  creature, a tile map, sprites and a sound into it as project files; a second engine finds all of it, loads the
  saved world to the same hash and the same pictures, and the assets are reused, copied and changed, undone,
  edited by a person on disk, and written as a `.glb` that reads back. On the Intel iGPU and the RTX 4090.
- **Measured leverage** (`scripts/leverage.py`, [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md)): the same game
  jobs built through the engine and without it by the same model, judged against one list. At the sizes tested
  the engine costs more for the same result (about 5, 3.1 and 1.7 times the tokens for a small game, a medium one
  and a change to it), and why is measured: more requests, each reading the context again, and more written.
  It is the baseline M6 has to move.
- **Audio:** a hand-written Ogg/Vorbis decoder, streaming music, music as data, loudness metering, `audio_*`
- **Text:** bundled OFL fonts, GPU text, in-world text and editing, window typing with IME and clipboard, CJK
  fallback (`specula-text`)
- **UI sight tools** (`ui_tree`, `ui_frame`, `ui_act`, `ui_check`) and **timelines** with camera rails (`timeline_*`)
- **Localisation** (`specula-lingua`, `l10n_*`): string tables as project data, CLDR plural rules, a review gate;
  Sepulcra in English and German
- **Accessibility and Sepulcra's shell:** a title screen, settings saved per player, rebinding, gamepad pause,
  interface scale, colour-vision themes, reduced motion, subtitles and captions
- **The human editor on the agents' protocol** (F7): outliner, inspector, selection, gizmos, undo; the draw tools
  (sculpt, splat, paint, sketched paths); the Swarm tab (journal timeline, claims, board, review queue, costs)
  and "highlight and ask"; a window a person and an agent can share
- **Gameplay modules** as SDK examples (`inventarium`, `facultas`, `colloquium`, `quaestio`, `exemplum`)
- **The input monitor** (`security_*`): every way an agent could reach the human's keyboard, window or native
  code is classed, frozen by identity and shown at the window; lead authority comes only from `--lead NAME`
- **Tool kits by job** (`tools_catalog {kits, kit, tag}`): 21 kits; a kit lists about 13 KB where a role listed 85
- **The launcher:** a start screen with the projects worked on, and the engine found and called from a terminal
  (`specula projects`, `open`, `new`, `list`, `tools`, `call`, `stop`; `call TOOL @FILE`, `--as NAME --role ROLE`)
- **Asset export** (`model_export`): any recipe as a `.glb` with its textures, rig and clips
- **Map 1 ships the 33 approved props**, with a per-prop `blocks_build` flag
- **Rendering follow-ups:** alpha-cutout foliage and wind, real grass, render graphs prepared ahead per size
- **Reliability and performance:** GPU work, recipe writes, studio sheets, motion checks and world loads off the
  engine thread; a build request no longer reads the disk under the model library's lock; a call sent while the
  engine starts waits for the start; a stall says whether the engine worked or waited (`cpu_ms`, `lock_ms`) and
  a slow frame says what it did (`top`); the pipeline cache is the adapter's own
- **The gate** (`scripts/gate.py`): the workspace's tests, the engine's library tests five more times, once
  with every worker starting late, the speed budgets alone, clippy for Windows and Linux, both builds; a test
  that fails in some runs is FLAKY and fails it
- **Cloud swarm foundations** ([docs/design/CLOUD_SWARMS.md](docs/design/CLOUD_SWARMS.md)): the design for
  per-agent tokens and encryption from the OS, ready to build in M6
- **The final audit** (`scripts/live_checks.py`): every live check, one after the other, built first, each
  told its own ports. It found thirteen checks that had gone stale since they last ran (current again), and
  one defect of the engine that had been there since M3: **a capture that forces a level
  (`sight_frame {lod: 0}`) from a far camera now gets it** (streaming uploaded levels by size on screen
  only, so the capture drew what was on the GPU and counted the rest as coarser). Every script reads and
  writes its text, and reads its processes, as UTF-8 (a guard holds it). It also found that the scripts' own
  client waited 10 to 26 ms in about one call in seven (Python's connect with a timeout, on Windows), which
  checks read as the engine's time: the scripts connect first and set the timeout after
- **Module builds on Rust 1.100** (found by CI on the push that ended M5: this machine's Rust was one release
  older). Its Cargo keeps what rustc writes in `build/<package>/<hash>/out/` where `deps/` was; the build
  service found no dep-info there and refused every module. The dep-info it checks is now the one beside the
  build's own copy of the library, told by the library's bytes, in either layout (every copy with those bytes
  is checked, and a copy without dep-info refuses the build): in a target folder an older
  toolchain also built into, the older build's dep-info is never taken for the new library's (it was found by
  name, and before that by date). Stale build units of the new layout are dropped after a build as the old
  layout's files are. `scripts/gate.py` says which Rust it runs with and fails when a newer stable is out
  (`--older-rust`), since CI installs the newest
- **A call that is running when the server stops gets its answer** (`specula-protocol`). The server shut
  every connection's socket and then waited for the calls on them, so a call that was applied, or refused,
  had nowhere to say so. A call refused because the engine's start failed lost its refusal whenever its
  thread ran later than the engine's stop: one failure in one gate, of a test that had passed every run
  before. The server now waits for running calls and the writing of their answers (a caller that does not
  read is given 5 s), and begins nothing that arrives after the stop; the test makes the refused call late
  on purpose
- **A module build in a folder too deep for the linker says so** (`logic_build`: `build_path_too_long`, with
  the file, its length and the limit). On Windows rustc hands the linker its arguments in a file inside the
  build, and the toolchain's linker opens a path of at most 259 characters; at 260 it read none of them and
  the build failed with `liblto_plugin.dll not found`. Rust 1.100's build folders are 29 characters and the
  package's name deeper than the ones before. Found as one failed live check out of 70 on the final tree,
  in an output folder one character deeper than the last audit's: the runner refuses such an `--out`
  before it starts
- **Text files are LF in every checkout** (`.gitattributes`): the tree a change is gated in holds the bytes
  everyone else gets. A guard of the final audit matched a line with its line ending and passed only in the
  tree it was written in. In an LF tree one live check (`trace_check.py`) handed a module's source back with
  CRLF, having read and restored it as text: it handles bytes, a guard holds that form for every script
  that edits a source, and `scripts/live_checks.py` fails a check that leaves the checkout changed
- **A locale's system face is one of its own or none** (`specula_text::load_cjk_face`,
  `system_cjk_candidates`, `CjkScript`). The fallback files for a locale end with a symbol face, and "the
  first that loads" was taken as the locale's: on a machine with no CJK font (CI's Linux image, the first
  to run these tests off Windows) a table's `system:ja` slot got the symbol face, with every character
  missing and nothing said. The slot is now a problem that is said, and in-world text looks in the
  family's own files (on such a machine it has no fallback face where it had the symbol face; its CJK
  characters report as missing either way). Linux had no file for Traditional Chinese (it looked for the Simplified faces first):
  two are named. The tests say the order of families, which holds on every platform, and take a face
  that has their characters
- **CI runs every test binary and the gate says what a failed test said.** The fix for the first red run
  got one package further on Linux and met the next failure, which the first had kept from running
  (`cargo test` stops at the first binary that fails): CI passes `--no-fail-fast`, and its later steps run
  whatever the tests did. On Windows one launcher test failed once in three runs and the gate gave its
  name only, with the logs gone with the machine: a failed gate now ends with each failed test's own
  message. The test took a hand-over file between its making and its writing, which the engine's watcher
  never does (`open_waits`); with a delay put between the two it failed every time, in the 64 s CI's run
  took, and it now waits as the watcher does. The run that showed everything showed one more on Linux, of
  the whole workspace: a trace test took "the second event's time selects one event" for the filter's
  doing, and on that machine both events had one microsecond (it had passed there twice that day). It
  says what a time range takes and leaves, of the events' own times and of times that are given
- **A file written again within the clock tick it was read in is seen** (the model library: recipes, their
  folder, styles, anim sets). File times move in steps, and the library compared times alone: a second
  write inside the step of the first was never read. An outside review (Grok, on a Linux machine,
  2026-11-15) had `changed_anim_sets_apply_at_the_poll` fail in one run of two; on Windows a copied file
  keeps its source's old time, which is why it never failed here. Given a rewrite that keeps the file's
  time, the test fails every time. A time is now compared with only once it is more than two seconds
  old (`models::stamp_of`): a text read sooner than that after its file's time is read again by every scan
  and compared by what it says, until its time is old; a file read again that says the same changes
  nothing (a style read again used to forget every recipe under it). Still on size and time alone, and on
  the roadmap: a glTF asset's key and the project store's watcher
- **An interface's system face has its digits and Latin letters** (`specula_text::load_cjk_face_with`,
  `INTERFACE_ASCII`). A table's `system:ja` slot is the one face for all of that slot's text. The same
  review's machine had a CJK fallback face with no Latin as its first Japanese candidate: it was taken, and
  every digit and English word of the HUD was a missing glyph. The slot now takes the first of the
  locale's faces that has ASCII too, or says that there is none
- **Tidied after that review:** two test lines needed Rust 1.87 where `Cargo.toml` declares 1.85 (the
  reviewer's build on 1.85.1 failed on those two lines alone; they are rewritten, and nothing here has
  built on 1.85 since, so that is not shown); every ignored test says why (16 gave no reason); a dependency only tests use and two that only
  Windows code uses are declared so (Cargo 1.100 warned); the roadmap's release order and platform order
  follow the owner's decision of 2026-11-14; stale "where things are" lines in ENGINE_PLAN, HANDOFF, this
  file's title and AGENTS' line for `specula-text`

### M4, done (2026-10-28 to 2026-10-30)

- **Exit proof:** a swarm (a director, four builders, an art director, a sight-skeptic and a debugger) built 33
  graveyard props that pass `style.toml`; one contact sheet went to the review queue; the owner approved it at the
  window, and they were promoted into the game world, which still won on the iGPU within budget (putting them in
  a fresh Sepulcra's map 1 is an M5 step)
- **Galleries and the owner's pick flow** (`specula-gustus`, `gallery_*`, F8): choose, don't describe; pick-driven
  tuning, the preference store (private), reference boards, style onboarding
- **Baked GI** (`specula-lux`, `gi_*`): deterministic CPU-baked probe volumes, tiers low and high, terrain and water
  included
- **Terrain and stylised water** (`terrain_*`, the `water` component)
- **Disk footprint budget and binary assets** (`disk_*`, `project_upload`)
- **Scoped tool lists** (`tools_*`): agents start with 6 tools (about 5 KB) instead of every tool (about 255 KB); the lead
  picks each agent's tools
- **Built-in tracing and an autonomous debugger role** (`trace_*`, incidents, watchdogs, `--strict-budget`)
- **Authority, jobs and provenance:** operator grants, an access table for every tool, one commit per call, drawn
  facts from a frame record
- **Bounds and workers:** schema limits, answer ceilings, a worker pool and step jobs; asynchronous capture readback;
  the remaining engine-thread stalls removed (worst window frame under heavy agent calls far lower; captures faster
  than before)
- **Fidelity T3** sheet for the five benchmark subjects
- **The final M4 audit and its fixes** (three review rounds): gallery calls wait for their answer (a dropped reply now says the outcome is unknown); every taste write (gallery apply and decide, suggestions, the configurator) is one commit, and a deferred job's commits count toward the call that started it; filmstrips measure brightness in the subject's own box; `sight_frame` leaves far outliers out of its auto framing; unchanged writes check claims; gallery `choice` parameters only on presets. The authority drill holds 119 probes

- **Asset studio:** `specula-studio` measures assets against `style.toml` (budgets, scale, palette families by exact
  Oklab search) and runs taste checks (silhouettes, thumbnail solidity, pixel heights, edge contrast, flat light)
  on an offscreen stage with light rigs and scale references. Contact sheets, line-ups, before/after marks and
  shot sets. Calls run on the caller's thread, and renders go to the engine round-robin within a budget. Six tools
- **Procedural modelling tiers 2-4:** CSG with bevels, SDF sculpts, terrain with erosion, scatter, a building
  generator and texture graphs, all under one deterministic work budget. Sepulcra's graveyard gains procedural
  headstones, a crypt and ruins. `style.toml` gains building, ruin, terrain and set categories (Claude's proposal)
- **Review queue, critique queue, merges and director tools:** review submissions with batch decisions and a modal
  review screen in the window; critiques linked to the board; three-way merge offers; `task_plan` and
  `task_progress`. Promotion and revert go through the write policy
- **Particles, effects and decals:** `specula-scintilla` effects as data; one instanced particle draw; deferred-style
  decals. Photosensitivity (WCAG 2.3.1) is checked per effect, flashing instances are never drawn, and a runtime
  flash watch measures the final frame (warn-only). Sepulcra gets fire, embers, wraith trails, impacts and dust
- **One write policy** (the owner's top process change after M3): every change to the live world goes through
  `write_policy`. Writer code gets `&mut dyn WorldWrite` (recorded writes only) and never `&mut World`; the live
  journal is written only with a key that nothing outside the policy can make; exemptions are named variants with
  reasons. A runtime backstop catches unrecorded writes, puts back engine settings and never loses project files.
  Claim holders get notices when module or live-Lex ticks write inside their claims. `logic_build` no longer picks
  clippy's dep-info file. Behaviour is unchanged: physics, joints and playtest end hashes are identical, and the swarm
  drill holds

### M3, done (2026-10-18 to 2026-10-22)

- **M3 exit on the iGPU:** `scripts/m3_exit.py` plays a whole Sepulcra game windowed on the Intel iGPU and holds
  every frame-level budget, with everyday desktop apps open (GPU p95 about 6 ms against 14). The scene pass is
  cheaper: point lights out of range are skipped, the shadow disc stops early where its rim taps agree, and the sky
  ray is computed per vertex. FXAA takes its luma from the tonemap pass. An optional window frame cap
  (`--max-fps`, `perf.toml [window] max_fps`) is off by default
- **M3 integration audit fixes:**
  - module RNG streams are capped (`rng_full`), so saves always load, and their hash is kept incrementally;
  - `lex_get` answers are capped near 64 KB, and big `lex_export`s are written to files;
  - `world_load` is refused while a playtest runs;
  - playtest replays hash live Lex runs;
  - reserved-data rules apply on every write path (games, modules, live Lex);
  - `world_load` lists and stops live Lex runs;
  - files that agents name are confined to the engine's own folders (`safe_path`: `lex_import`, `desync_compare`);
  - `anim_filmstrip` shares the world's shadow map, so it no longer makes the GPU wait idle;
  - `specula-model` is now a simulation crate

- **Swarm communication:** a task board (`task_*`: dependencies, owners, blockers, questions, a change feed; saved as
  `board.json` beside the world, with a 24 MiB budget), intents and notes (`intent_*`, `note_*`; overlaps flagged
  before any edit), conflict help on refused writes (`conflict {base, theirs, yours, overlap, rebase}`), roll-ups
  (`world_rollup`), resource leases (`lease_*`: ports, advisory GPU time, scratch folders), the window's frame floor
  under a swarm as a setting (`perf.toml [swarm]`, `swarm_settings`, lead only, with a hardware hint), and a desync
  detector (`desync_trace`, `desync_compare`, `Game::state_hashes`, `scripts/desync_check.py`). `specula-sys` gains
  `power()` and `net::tcp_listening_ports()` (runtime-loaded `iphlpapi`); `specula-gpu` gains `Gpu::memory_stats`
- **Module host:** game logic as hot-reloaded Rust `cdylib`s. New crates `specula-sdk` (what a module depends on:
  deterministic `Cx`, versioned C ABI, panics caught at the boundary) and `specula-logic` (loading, a build service
  that merges duplicate requests, Cargo diagnostics as data with machine-applicable fixes). Tools `logic_build`,
  `logic_reload`, `logic_unload`, `logic_status`, `logic_input`, `logic_hook`. Builds are vetted (path crates only;
  no build scripts, proc-macros, Cargo config or toolchain files) and run offline with `panic = "unwind"`. Example
  module `games/ambulo`; edit to reload about 1 s. Engine crates build at `opt-level = 2` in dev
- **Lex, gameplay as data:** new crate `specula-lex` (std only): rule sets as JSON with 13 item kinds (params,
  resources, paths, sites, archetypes, effects, machines, waves, timers, rules, commands, outcomes, natives), exact
  integer units, a typed expression language, GAS-like effects with stacking policies and tags, a fixed phase order,
  structured diagnostics with suggestions, and bounded work, memory and replies. Tools `lex_schema`, `lex_list`,
  `lex_get`, `lex_edit`, `lex_validate`, `lex_simulate`, `lex_import`, `lex_export`, `lex_session`; rule sets live as
  `/lex/<set>/<item>` entities (only the Lex tools write `/lex`). A slice of Sepulcra in Lex matches the hand-written
  sim tick for tick across seeds
- **Animation A3 and agent-authored motion:** graphs as data (parameters, 1D/2D blends, additive and external nodes,
  layered state machines with masks, transitions with conditions and exit phases), plays whose pose is a pure
  function of time, two-bone IK (feet on the ground with the pelvis following) and look-at; motion generators
  (`keys`, `pose`, `recoil`, `breathe`, `chain`, auto gaits); motion checks with exemptions from `style.toml`; tools
  `anim_get`, `anim_set`, `anim_state`, `anim_check`, `anim_filmstrip`. Sepulcra's creatures run on graphs with new
  clips (climb, glide, dive, pound, flinches, shuffle, jog), and the checks' fixes (a runner that hovered, colossus
  hands in the ground)
- **Locus and navigation:** new crates `specula-locus` (`world_place`: objects placed by relations, collision-checked,
  with the nearest valid alternative when a relation can't hold), `specula-via` (tiled navmeshes rebuilt per tile,
  paths, off-mesh links, area costs, crowds) and `specula-mens` (behaviour trees and utility AI as data); tools
  `nav_build`, `nav_path`, `nav_point`, `nav_agent`, `nav_info`, `ai_check`, `ai_set`, `ai_status`, `model_collider`;
  `navmesh` and `paths` overlays; a `nav` hash in `world_hash` and desync traces; nav state in save games, saved
  without changing the run
- **Live Lex runs in save games:** a `lex_live` section (state, journal tail, presentation, budgets) that loads by
  adopting the shown entities and is checked by hash
- **Sepulcra fully in Lex, and live Lex:** the whole rule set (flow fields, walls, climbing, lane jumps, flyers, roots,
  the colossus) as Lex data, tick-identical to the Rust sim; new general Lex features (data items, triggers, refusal
  details, ordered archetypes, grouped spawns); `lex_live` runs a set on the engine's tick with presentation, input
  bindings, deterministic work budgets, resumable halts and module hooks as natives; `sepulcra --rules lex` lets Lex
  decide the game, stopping with a dialog on any desync; Lex state (snapshot v2) in save games
- **Streaming and HLOD:** model and texture uploads go through a per-frame budget (`perf.toml [streaming]`), with no
  waiting on the GPU in the frame; residency follows on-screen size under a memory budget; distant cells draw as
  merged HLOD proxies built on a worker; captures wait for what they show. Tools `stream_state`, `stream_settings`,
  `stream_fly`; `perf_frame.streaming`; sight packets mark `drawn_as: hlod | not_resident`
- **Save games:** `.specsave` files hold the world, the hosted game, physics (with warm starts, sleep state and SAT
  memory) and module state, so a loaded game continues tick-identical; versioned with migrations, checksummed,
  bounded and fuzzed; decoded on a worker so loads never stall the engine. Tools `save_write`, `save_read`, `save_list`,
  `save_delete`. Sepulcra gains a Saved games screen (three slots, two rotating autosaves, pad/mouse/touch/keys)
- **Balance lab:** new crate `specula-balance` (targets as data, deterministic multi-threaded sweeps of Lex rules, a
  seeded Gaussian-process tuner, several play styles at once) and tools `balance_targets`, `balance_sweep`,
  `balance_tune`; `scripts/balance_playtests.py` cross-checks tuned points with real playtests
- **Visual history gallery:** `docs/gallery/` (55 entries from the Bevy era to today, by subject and by date, with
  honest dates and notes), `scripts/gallery.py` (add, check, dupes, drop), the specula-core `thumb` example (zero-dep
  downscaling to JPEG) and a JPEG known-answer test against an independent decoder
- **Headless playtests and swarms:** playtests as data for any game (`playtest_run`, `playtest_report`: steps that call
  any tool at a tick, expectations, `until`, replays verified by checkpoint hashes of the world, physics and the
  game's subsystems), `--paused`, `scripts/playtest_swarm.py` and `scripts/playtest_suite.py` (26 Sepulcra playtests
  across parallel engines, every replay verified)
- **Tool ergonomics:** heavy tools answer concisely unless asked (`response_format`, per call, per agent or per
  engine with `--response-format`), every answer is compact JSON and every error is `{error, message}`; audit in
  `docs/design/TOOLS.md`
- **Physics P1-P3:** new crate `specula-physics` (std only): spheres, capsules, boxes, convex hulls, static meshes
  and heightfields; a two-tree broadphase; GJK and SAT with per-pair feature memory; raycasts, shape casts, overlaps
  and triggers; a character controller (steps, slopes, ground snap, moving platforms, pushing); Soft Step rigid
  bodies with sleeping islands, a tunnelling guard and a 2D mode. Engine: collider and body components simulated on
  the fixed tick and journaled, `world_env {physics}`, tools `physics_raycast`, `physics_cast`, `physics_overlap`,
  `physics_contacts`, `physics_events`, `physics_info`, `physics_explain`, `physics_check`, `physics_impulse`, sight
  overlays `colliders` and `contacts`, a physics hash in `world_hash` and the desync trace. Existing worlds keep
  their files and hashes. `specula-model` gains `collider::fit_limited` (moving hulls are capped at 16 points)
- **Determinism hardening:** module loads, unloads and their build hashes are journaled (`Kind::Module`) and
  checked on replay and in saves; the build hash now covers the module's sources exactly, so rebuilding unchanged
  code gives the same hash (saves and replays written before 2026-10-22 report another build for the same code).
  The FP environment (MXCSR on x86, FPCR on aarch64) is checked and restored around module calls and before every
  agent command (`specula_sys::fpenv`). Determinism lints: one `clippy.toml` in each of the 10 simulation crates
  (no `HashMap`/`HashSet`, platform float maths or rounding, wall-clock time or threads), the same check on modules
  at `logic_build` (`determinism::*` diagnostics, bounded), `forbid(unsafe_code)` in every crate without FFI and
  workspace lints for SAFETY comments; `scripts/determinism_check.py`. The audit moved the remaining platform
  rounding in simulation and world-state code to `dmath`
- **Streaming fix:** a frame's first upload is always tried, and captures taken while no frame runs get their own
  budget, so a built HLOD proxy can no longer wait for its upload forever
- **Joints and ragdolls (P4, A4):** ball, hinge, distance and fixed joints with limits and motors; limp and powered
  ragdolls with hulls fitted from the rig, blending to and from animation, with a get-up; Sepulcra's shambler dies as
  a ragdoll. Ragdolls and world-anchored joints save and load bit for bit. Joint rules apply on every write path
  through a joint index. World scale is composed through each level's rotation. Restored entities never reuse a
  version, which fixes stale physics bodies after a force-undo; the record is pruned to what the journal can undo. An
  opt-in deterministic parallel physics solver (`--physics-threads`)
- **Project data through the protocol:** `project_*` tools give recipes, UI screens and themes, sounds, style files
  and module sources the claims, versions, journal (`Change::File`), undo and conflict help that entities have, with
  validation by kind and atomic disk sync (outside edits imported as `disk`, unreadable ones held). Mounts accept only
  real modules; manifests and compile-time paths are checked, the latter from rustc's dep-info after every build.
  `logic_build {fix: true}` commits fixes as the caller. Model hot reload now rebuilds every handle of a changed
  recipe (only the first was rebuilt)
- **Agent evals:** 21 tasks with outcomes the engine verifies (15 public, 6 held out), each with a reference and a
  broken solution plus 23 known cheats; `world_diff`, transcript, geometry, world-size and series verifiers;
  `scripts/evals.py` runs them across parallel engines, or with a real agent behind a proxy that enforces the task's
  tools and call limit. Results are scrubbed of private paths. Playtest matchers compare objects by key in any order
- **Privacy:** `crates/specula/tests/private_paths.rs` fails on any home-folder, temp-folder or session path, or a
  UUID, in a tracked file; `save_check.py` writes `<work>` instead of its temp folder
- **Streaming, memory ahead of need:** GPU memory blocks are allocated ahead on a worker thread and adopted at a
  flush, so a load no longer allocates on the frame (up to 164 ms before); the end of a pass is sliced into two steps
- **Streaming during loading:** the spatial index is refiled a slice at a time while the old cells serve (a big load
  used to force a 3-6 ms one-frame rebuild); upload batches use a fresh transient pool per slot, reset whole, with one
  barrier; mesh blocks grow 16-64 MB. Stalls inside the Intel driver's command recording are zoned and reported apart
  by `stream_check.py`. Alternating A/B on the iGPU: loading overruns lower than the earlier build in every pair
- **Streaming pump:** the eviction queue is scanned across frames into heaps instead of being rebuilt and sorted in
  the frame after each pass (2-3.5 ms spikes on the iGPU); a running resident total; profile zones for every pump
  stage. Alternating A/B on the iGPU: in-flight stutter 0 frames on every run, against 3-9 on the earlier build

### Pre-M3 cleanup (2026-10-17)

- Agent calls are served between presents (`schedule.rs`), not once per frame: one windowed agent gets 500-1,270
  calls/s instead of about 60-230; under a backlog the window keeps 60 fps. `perf_frame.serve`, `call_cost_ms`,
  `scripts/call_rate.py`, `swarm_drill.py --windowed`; `specula-platform` gains `timer::FineTimer` (winmm) and
  `refresh_rate_of`
- Sight: with `include_ui`, opaque HUD panels hide what's under them (`Gpu::render_with_ids_occluded`,
  `id_buffer.ui_occluders`, `MAX_ID_OCCLUDERS` / `Limit::IdOccluders`)
- The sky eases into the horizon instead of meeting it with a crisp line
- Sepulcra: selection and range-ring highlights use saturated rune cyan (the old pale mark rendered near-white)
- Docs: current README, HANDOFF and CHANGELOG; one open-owner-decisions list; tests for Markdown links and the MCP
  revision pin; CI rejects session links in new commit messages and runs clippy on Linux

### Added in M2 (2026-10-16): rendering, true sight, animation, first procedural shapes

- Render graph that owns barriers; PBR-lite and stylised materials; directional and point lights; shadow maps;
  post (tone mapping, FXAA, LUTs); sky; GPU instancing, frustum culling and LODs; `perf.toml` budgets
- 2D: sprites, tilemaps, vector shapes, orthographic cameras
- glTF/GLB import (meshes, materials, textures, skins, animations); `specula-anim` stages A1–A2 (GPU skinning, clips)
- `specula-model`: procedural modelling tier 1 (recipes, `model_build` / `model_reload`)
- Sight: ID buffer (`visible_px`, `occluded`, `screen_box_exact`), persistent named cameras, `sight_orbit`,
  `sight_compare`, `sight_query`, debug overlays in captures
- `specula-text`, `specula-ui`, `specula-input`, `specula-audio`
- *Sepulcra* vertical slice hosted through the `Game` hook, with `play_*` tools

### Added in M1 (2026-10-13): world model, multi-agent and determinism

- Text world files with crash-safe save/load and autosave; hierarchy and paths; tags, props, reserved physics
  components
- `world_batch`, `world_query`, per-entity versions (`expect_version`), claims (leases on subtrees and regions)
- Agent identity (`agent_hello`), agent-attributed journal with `world_history` and `world_undo`, `world_changes`
- Fixed-tick clock, seeded RNG streams, deterministic maths (`dmath`), `world_hash`, journal-replay test in CI
- `perf_frame` (GPU timestamps per pass), `time_control`; HTTP 503 with `Retry-After` at the connection limit;
  `scripts/swarm_drill.py` at 16 and 64 agents

### Added in M0 (2026-10-12)

- `specula-core`: JSON, math (Vulkan conventions), base64, CRC-32/Adler-32, DEFLATE/zlib, PNG encode/decode
- `specula-sys`: sole owner of `kernel32` imports; runtime library loader (`dynlib`)
- `specula-platform`: Win32 windows and input events
- `specula-gpu`: runtime-loaded Vulkan 1.3 renderer (dynamic rendering, synchronization2), headless targets +
  readback, window swapchain, validation capture
- `specula-protocol`: HTTP/1.1 server and MCP (Streamable HTTP, revision 2025-11-25) for agents
- `specula` world server: tools `world_spawn`, `world_list`, `world_delete`, `sight_frame`, `engine_info`
- CI on Windows and Linux; `Cargo.lock` must contain workspace crates only
- `world_set` (change an entity in place); `sight_frame` `max_subjects`, `distance_m`, camera `near_m`/`far_m`
- `scripts/window_check.py`: window-path regression check on every GPU
- Engine: window on its own thread; per-run capture folders (`--captures`); Ctrl+C / console close shut down cleanly

### Fixed (M0 adversarial review)

- Top-down cameras, large worlds and partly-behind-camera subjects in sight; unbounded target cache; stale timed-out
  commands; capture overwrites; unvalidated huge inputs; session expiry (see docs/ENGINE_PLAN.md)

### Removed

- All Bevy-era crates, templates, demo games and skills (preserved at tag `bevy-tooling-final`; docs in
  `docs/archive/bevy-era/`)

## [0.2.x] — Bevy era (as Grok-Bevy)

### Added

- **MCP query aliases:** `Name` / `Transform` / `GlobalTransform` → Bevy 0.19 FQNs (`component_paths`)
- **`bevy_env_check`** stamps `specula_version`, `server_binary`, `reload_hint` (stale install detection)
- Launch spawn message includes **child `pid=`**
- send_keys movement smoke recipe in agent-loop + TROUBLESHOOTING

### Changed

- Capture tool description is 2D/3D neutral (primary window)
- Default `bevy_brp_query` components: Name + Transform (via aliases)

### Previously unreleased (prior)

- **G6 install ergonomics:** templates embedded in `specula` via `include_dir`; scaffold falls back to cache extract without monorepo / `SPECULA_TEMPLATE_ROOT`
- **Optional `physics` feature** on 2d/3d templates and demos (`avian2d` / `avian3d` **0.7**); default features unchanged

## [0.2.0] — 2026-07-19

### Added

- **External greenfield dogfood fixes (Wave A/B):**
  - Template/demo `AssetPlugin` dual-mode: debug `CARGO_MANIFEST_DIR/assets`, release relative `assets/`, env `BEVY_ASSET_ROOT`
  - Loading fail-forward timeout (~12s) so stuck asset loads do not block BRP forever
  - MCP **`bevy_wait_brp`**; **`bevy_launch_app`** non-blocking by default (`wait_secs=0`), sets package cwd
  - `docs/PHYSICS.md` + skill pins: **avian2d/avian3d 0.7** for Bevy 0.19 (kits remain transform-based)
  - TROUBLESHOOTING: BRP method table, launch/wait, B0001, template root, Loading stuck
  - Scaffold/template `AGENTS.md`: asset root + B0001 notes
  - `mcp-config` prints `SPECULA_TEMPLATE_ROOT` when templates are discoverable

- **Game factory (v0.3 / alpha) G1–G4:**
  - `docs/GAME_DOD.md`, `docs/ROADMAP.md`, `docs/PACKAGING.md`
  - Skills `bevy-demo-game`, `bevy-package`; MCP demo/package workflows
  - Templates `game-2d` / `game-3d` are short demos (collect + hazard + Victory/GameOver)
  - In-repo dogfood: `games/demo-2d`, `games/demo-3d` (`cargo run -p demo_2d|demo_3d --features remote,capture`)
  - `scripts/package-demo.sh` — release binary + `assets/` → `dist/` (+ zip)

- **Production games (v0.2):** skill pack, templates, multi-kind scaffold, ship docs, MCP routing
  - `.grok/skills/bevy-production` / `bevy-2d-game` / `bevy-3d-game` / `bevy-agent-loop`
  - `templates/game-2d` and `templates/game-3d` playable vertical slices (states, movement, disk assets)
  - `specula scaffold --kind 2d|3d|demo` copies templates (source of truth) + project `AGENTS.md`
  - `docs/ASSET_CONVENTIONS.md`, `docs/SHIPPING.md`, `docs/PRODUCTION_GAMES.md`
  - MCP `initialize.instructions` covers production skills and scaffold kinds
- MCP **prompts**: `start_2d_game`, `start_3d_game`, `iterate_scene`, `prepare_ship` (`prompts/list` + `prompts/get`)
- MCP tool **`bevy_workflow`**: goal → ordered skills + tools/CLI steps (`new_2d`, `new_3d`, `verify_scene`, `ship`, `add_sprite`)
- Root **`AGENTS.md`**; README production section; PROGRESS v0.2 checklist

### Changed

- Workspace crate version **0.2.0** (aligns package with PROGRESS v0.2/v0.3 narrative)
- Default scaffold kind is **`2d`** (production); `demo` is the BRP cube fixture (`templates/sample-app`)
- Positioning: `templates/sample-app` is BRP **integration fixture**, not a production game template
- `bevy_workflow` documented as **router, not autopilot**

## [0.1.0] — 2026-07-17

### Added

- **`specula` CLI** with:
  - `doctor` / `env-check` — cross-platform Bevy readiness (Rust/Cargo, OS guidance, optional compile probe)
  - `mcp` — stdio MCP server for Grok Build agents
  - `mcp --delegate-brp-mcp` — exec into installed `bevy_brp_mcp`
  - `scaffold` — generate a BRP-enabled Bevy sample
  - `brp` — discover / query / mutate / screenshot / wait helpers
  - `compat` and `mcp-config` — version matrix and Grok registration snippets
- **`specula-env`** — injectable, unit-tested environment detection library
- **`specula-brp`** — BRP HTTP client, named targets, PNG capture → MCP image adapter
- **`templates/sample-app`** — Bevy 0.19 scene with `remote` / `capture` features and headless BRP smoke binary
- Docs: README fast-start, CONTRIBUTING, troubleshooting, multi-platform CI
- Dual MIT / Apache-2.0 licensing (matching Bevy)

### Compatibility

| Bevy | bevy_brp_mcp | bevy_brp_extras |
|------|--------------|-----------------|
| 0.19 | 0.22.1       | 0.22.1          |

## Project history

How the project got here, in decisions (moved from HANDOFF.md, which now covers only the current state).

| When | What |
|---|---|
| Before 2026-10 | The owner built **Grok-Bevy**: Bevy-based tooling that gave AI agents "eyes" on 3D scenes. The many sight plans and findings from that era are in `docs/archive/bevy-era/`. |
| 2026-10-12 | Claude assessed the repo. It was renamed **Specula** (Latin: *watchtower*). |
| 2026-10-12 | **Pivot.** The owner wanted the engine fully native and zero-dependency, with no Bevy. The Bevy code is preserved at tag `bevy-tooling-final`. |
| 2026-10-12 | Choices made: **Vulkan**, cross-platform with Windows first, **hot-reloaded Rust modules** for game logic, **2D and 3D** from the start, and the Specula repo becomes the engine. |
| 2026-10-12 | **M0 built** by 4 parallel subagents (one per crate, in git worktrees), with Claude integrating. Then two full audits and three adversarial review rounds; every finding was fixed. |
| 2026-10-12 | **M0 complete** at commit `badb857`. CI is green on windows-msvc, windows-gnu and ubuntu. |
| 2026-10-13 | ROADMAP.md and HANDOFF.md written. |
| 2026-10-13 | Engine survey and UI design written. The roadmap gained a **path to production**: v1.0 = a commercial PC game on Steam for Windows and Steam Deck; Phase A M1 → Phase B playable slice (lanes built in parallel by subagent swarms) → Phase C production → Phase D ship. |
| 2026-10-13 | The owner accepted **every** Claude proposal: physics, skeletal animation, headless playtests, batch edits, procedural modelling, review queue, swarm track. The swarm track became primary, foundations F1–F5 were added, and Claude researched and wrote the four design docs in `docs/design/`. Next: the owner's game project, then M1. |
| 2026-10-13 | **Lava-lamp fidelity benchmark** (the owner's first asset challenge): three lamps from basic to "as real as possible", built in the engine. It added per-frame GPU data (buffer device address), materials, point lights, glass, blob shadows, lathe meshes, tone mapping and a ray-marched lava lamp. Run it with `python scripts/lava_lamps.py`. The NVIDIA and Intel captures match within 3/255. |
| 2026-10-13 | **Swarm session (owner: "advance through M1 and M2 while building the TD game").** Wave 1, all reviewed then merged: capture speed (parallel deterministic PNG, ~25x faster), GPU (frame stats, instancing, culling, pipeline warm-up and cache), World (hierarchy, versions, batch, query, crash-safe text saves), determinism (`dmath`, correctly rounded f32; Android CI check), Sepulcra's headless simulation. Then the engine got a `Game` hook, a fixed-tick clock, `perf_frame` and `time_control`. Wave 2 lanes: render (shadows, post, sky, overlay), text, procedural modelling, input, the Sepulcra app, journal/claims. |
| 2026-10-13 | **First game chosen:** the tower defence *Sepulcra* (over a cozy castle builder and a squad RPG). Platforms: Windows + Steam Deck; mobile later, so foundation **F6 (portable by design)** was added. A **Fidelity track** was added to the roadmap: benchmark assets rebuilt at each tier, from greybox up to Unreal-class realism. |
| 2026-10-13 | **M1 complete:** world model, claims, journal and determinism; the exit proof had 4+ agents building separate districts at once. |
| 2026-10-16 | **M2 complete:** rendering, the ID buffer and sight tools, 2D, glTF, animation A1-A2, procedural modelling tier 1, and the Sepulcra slice integrated. |
| 2026-10-17 | M3 planned from the agent game-making research (Lex, Locus, animation A3-A4, physics P1-P4 and more), waiting for the owner's go-ahead. Pre-M3 cleanup approved from an outside review. |
