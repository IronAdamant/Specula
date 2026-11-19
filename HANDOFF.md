# Handoff: read this first

These are notes from Claude (Opus 5.5 through M5's first wave; Fable 5.2 from 2026-11-12) to whoever picks up Specula
next, most likely a new Claude conversation with no memory of the earlier ones. This page is **current state and
what's next** only; history is in [CHANGELOG.md](CHANGELOG.md). Last updated **2026-11-17**.

- **Owner, 2026-11-16: the scope talk. M6 has started.** A fresh instance reviewed these documents and
  put nine questions; the answers are below in the owner's words, with Claude's reading marked where
  one was needed. The sizes and the order that follow from them are in ROADMAP M6.
  - **No deadline, and nothing cut.** "Scope and date doesn't matter. Christmas IS hope but I'm not
    gonna worry about strict deadline." The sizes set the date.
  - **The shader language comes before macOS.** "Shader first. MacOS can be waited on." The rule of
    2026-10-18 holds: no Metal shaders written by hand. *Claude's reading:* the first release is still
    Windows and macOS together (2026-11-14) and waits for macOS; that was not asked again in so many
    words.
  - **The Mac.** Apple Silicon only ("M2 Mac. Intel based macs will not be sufficient"). A Claude
    instance builds there from a clone of its own, as an outside user would ("I expect people
    externally when they use it to pull anyway. So we'll mimic that"), and its work is the native port
    ("Mac is just native"). Apple's linker from the Xcode tools is accepted there ("Zero dependency.
    But on MacOS, I expect it will be fine"). An Apple Developer account: "Not yet but that's for me
    to discuss with your local instance" (the one on the Mac). Without one the app is unsigned and
    macOS warns when it is opened.
  - **The Steam Deck pass is native** ("Native on it"), so it goes with the Linux work, after the
    first release. M6 keeps the parts that need no Deck: a controller-only playthrough, legibility at
    800p and 30 fps on the iGPU. There is no Steamworks app ID yet ("that's for later. We need to
    finish this"): Steam integration can be built, and cannot be tried against Steam until there is
    one.
  - **Usage is not the limit.** "I'm not gonna fuss over the weekly usage for M6. I want it done and I
    will be using my resets to get it done ASAP."
  - **Who builds.** "You may deploy sub agents only if necessary, and you decide WHICH models to use
    for the tasks themselves. Either Opus or Sonnet. At your discretion. You act as a main agent, and
    a monitor." *Claude's reading:* one builder on this laptop, as in M5; a review by a model that did
    not write the code is one of the necessary uses.
  - **The Rust version is pinned** (yes to: one version named in the repository and in CI, moved on
    purpose through the gate, and declared as the minimum in place of 1.85, which nothing tested; the
    owner's default toolchain on this laptop stays untouched).
  - **Verifying faster is M6's first work** (yes to: CI keeps its build between runs, a documents-only
    commit skips the full run, the gate is sized to what changed).
  - **Other vendors' swarms are not ours to test.** "That's for the people outside of it to worry
    about. I can only provide what's present here. Grok and OpenAI swarms are for someone else to
    worry about. Right now, I'm focused on Claude Swar[m]." *Claude's reading:* per-agent tokens stay
    an M6 item and are tested with Claude swarms; whether they are in the first release or follow it
    was not said, and until they exist the engine stays on loopback (rule 12).
  - **The eval set:** "Make M5 base numbers public." They are public already (README,
    [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), `perf/leverage/2026-11-14`), and the release
    says them as they are. *Claude's reading:* no private held-out set is asked for now; a benchmark
    claim beyond the published numbers would still need one.
  - **The raw recordings of 2026-11-12:** "Keep. This will be for release discussion."
  - **The seven checklist items left for the owner:** none has failed ("No, but I'm not gonna worry
    about it. I'm focused on completing everything").
  - **The Mac is last, and the laptop's parts go together** (later the same day): "Put Mac to last. This
    is a separate problem and seperate development and will need to be handled by a separate instance
    based in Mac system. Everything else to be done together." ROADMAP M6's order is renumbered for it:
    twelve parts on the laptop, then macOS as part 13, then the release. The shader language and the
    renderer's inner interface, which only the Mac and the end of GLSL wait for, moved to the end of the
    laptop's list (Claude's order). *Claude's reading:* the release is still Windows and macOS together.
  - **One more part: a library of sample assets, for any game** (the same message; "I'm actually
    thinking on this. But add 1 more part here"): animals, fantasy creatures such as dragons, the
    undead, robots and machines, and realistic people, each animated, all by code, shown in 2D and 3D,
    "separate from [Sepulcra] but ... it's own thing". "The primary reason is to help you and other AI
    agents build stuff and reuse, not just completely re-create. Think of it like a base template as
    I'm widening the path of resistance for you and other AI agents." It is ROADMAP M6's part 5, after
    the form tools it is built with; the item there has Claude's thoughts (body plans and not single
    models; the look comes from the project's style; motion by plan; 2D from the same source; said
    plainly, no fur or feathers until the renderer has them). Not designed.
  - **Widened into the libraries, done right and not rushed** (later again): "Do your best as you can
    in realistic. I'm very much willing to allow M6 to take a long time. But I need this done right,
    not rushed." "The main point is, we get model library, terrain library, assets library, water
    library, grass blades. etc. ALL written in Rust ... Like having unique trees, not copy-pasted
    characters as humans have done. We're in agentic era." People get faces with expressions and hair
    ("Something like Skyrim-like"), with humanoid aliens derived from them; faces are a lane for a
    separate instance (a base male and a base female face, and hairstyles). "Renderer can come later,
    ONCE you have data for this." The owner offered Grok for the research; the list of what to bring
    back, and how, is [docs/research/LIBRARY_RESEARCH_BRIEF.md](docs/research/LIBRARY_RESEARCH_BRIEF.md).
    What comes back is checked against its sources before it is used. Part 5 grows to about 40,000 to
    60,000 lines and the laptop's list runs to February at the earliest; the owner accepts the time.
  - **Recordings of M6's work, failures included** (the owner, the same day, before going away with
    the 49" monitor off): "Feel free to record success and failures in videos. May publish them for X
    and LinkedIn (At release stage. Don't hide failures). We'll be editing this video for the release
    stage." So live runs worth showing are recorded, of the engine's window only
    (`scripts/window_recorder.py`, never the screen), kept in the gitignored `gallery-full/` with what
    each shows, and a run that failed is kept like one that passed. Nothing is published before the
    release discussion.
  - **Every asset's making is recorded, looked at in the engine, and mended** (the owner,
    2026-11-17, as part 4 reached what can be seen): "Record the creation of bodies, hands, faces.
    SAme with animals, same with anything please. Review through Specula engine, if they look bad,
    then try to fix it please. I'm going to turn off my 49" monitor for consistency so you can get
    expected data running. Record their improved looks and also the eval/performance." *Claude's
    reading:* for every asset of parts 4 and 5, it is built through the engine and captured as it
    first comes out, kept even when it is bad; looked at in the engine's own pictures (and checked
    by a model that did not take them) before anyone says it looks right; mended; captured again.
    With each go its numbers (triangles, build time, frame time on both adapters, what the form
    checks say), so that "improved" is a before and an after and not a claim.
  - **Reference pictures are coming** (the owner, the same day, asked whether the figures are
    made from the research and from what a model learned in training: "Do you want me to send you
    the images to compare? I can get Grok to build up a list of those alongside the creatures",
    then "Reference images.. sure. I'll get you those. Please get me a list of what you need").
    The list is [docs/research/REFERENCE_IMAGES_BRIEF.md](docs/research/REFERENCE_IMAGES_BRIEF.md):
    people, hands, feet, heads, motion and creatures by body plan, each with its source and
    licence. They are looked at and compared with captures, never traced.
  - **They came the same evening** ("Reference images supplied. Add them to the library"): 110
    pictures with an index, all with licences that allow keeping them. The index is in the
    library ([docs/research/library/reference-images/](docs/research/library/reference-images/README.md));
    the files are on this machine in `gallery-full/reference-images/`. The owner on why: "it's how human
    artists, in any profession, work. They refer to skeleton images before putting in their
    touches... Even though they know what humans look like, they still use reference and
    measurements, so I'm getting you digital equivalent of those".
  - **The pictures have a public repository of their own** (the owner, the same evening, asked
    whether the files go into this repository or stay beside it: "Could I not have it be a
    separate public repository linked to Specula for future references?", then "Name at your
    discretion, and unclothed plates ... do go in. It should atleast be linked to Specula and the
    basis of it. With citations referred, and links. We can use the documents from Grok alone for
    that"). It is [specula-imagines](https://github.com/IronAdamant/specula-imagines): the 110 pictures, Grok's index as the
    citations, and the 24 unclothed plates and motion studies under `ADULTS-ONLY/` ("I'll take
    responsibilities, because I think it's still important to put gaits in ... Just put it
    somewhere like (ADULTS ONLY) or something"). Specula links to it and never depends on it. Its
    clone on this machine is beside this folder; `gallery-full/reference-images/` is the copy as
    delivered.
  - **More reference is offered, and wanted** ("Want me to also find images for certain things,
    and publicly attributable links you can easily reference, both for looks and animations as
    well as potential physics that you could adjust in any games? I can create several repos for
    it. For 2D/3D space"). The list is
    [docs/research/REFERENCE_ROUND2_BRIEF.md](docs/research/REFERENCE_ROUND2_BRIEF.md): the
    pictures the first round did not find, motion as numbers, physics values a game can adjust,
    and what 2D needs. Pictures and bulky data go to repositories of their own; numbers stay in
    this one's library, where tests are held against them.
  - **The second round came the same evening, and the owner had not looked at either** (asked
    about plates of children: "Hold on, Children's plates? These are clothed hopefully? I don't
    want to get in trouble, as the only purpose is to give AI agents something to reference for
    animation, for building in character skeletons that you can use to turn into real characters
    in the games, in different styles"; then "I hadn't even looked at those images. Grok just gave
    me those and I extracted those, so you could reference them"). So **whoever places a delivery
    is its only check**: every picture is looked at first; unclothed adults are kept apart; no
    unclothed figure of a child goes in, drawn or photographed, and none whose subject cannot be
    shown to be an adult. Of the 161 pictures 159 are published (269 in all). Two that Claude
    had let through on its own judgement (a doll-like chart of proportions by age, and a drawing
    of two unclothed young women whose age nothing shows) were public for about ten minutes;
    at the owner's word ("Please take care of it. Erase them. And take them out from my
    extracted folders and zip files if you can") they were erased from that repository's
    history by a forced push and removed from the delivery's folders and zip files. GitHub
    went on serving the old commit to anyone with its id, so at the owner's word ("delete and
    recreate please") the repository was deleted and made again from the clean copy the same
    night: three commits, 269 pictures, the same address, and the old commits no longer exist
    there (checked by asking for them). The repository's readme says, as
    the owner asked, that it is an agent reference repository kept for the Specula engine. The round's twelve files of
    numbers (running by speed, recorded-motion datasets, friction, bounce, densities, drag,
    vehicles, what people and animals can do, projectiles, water, other worlds) are in
    [docs/research/library/](docs/research/library/README.md) as delivered and **not yet
    checked**. H7 also says where the walking curves already are (H1): the first walk was made
    from guessed numbers and need not have been.
  - **A third list, of every known gap, ahead of the parts that need it** (the owner, the same
    night, asked whether more reference is needed: "Might as well list me the gaps now. So you'll
    have materials. Vehicle physics, air plane physics, space physics would be good to have as
    well as bullet physics, and sword physics"). It is
    [docs/research/REFERENCE_ROUND3_BRIEF.md](docs/research/REFERENCE_ROUND3_BRIEF.md): those
    five kinds of physics, what the first two rounds did not find, and what each later part
    (faces, clothing, animals, machines, plants, terrain, water, sky, light, buildings, sound,
    fire) will want. Nothing in hand waits on it.
  - **Round three came the same night** (91 pictures, 88 published: 357 in the pictures'
    repository now; and 23 files of numbers in the library: fire, light, lightning, blast,
    fracture, drag by shape, vehicles, aircraft, space, bullets, swords, sound, buildings,
    weather, robots, extinct animals). **The 35 library files of rounds two and three have no
    second checker's report.** The owner: "A second checker pass.. for round 2 and 3 will need
    to be dealt with at a later date. I'll need you to log a reminder before we go onto MacOS";
    then, before sleeping, that the pass is being run after all and its files will come with a
    fourth round on the gaps. (They came: the next point. The reminder before the macOS part is
    discharged.)
  - **The second check of rounds two and three came, with a fourth round already checked**
    (2026-11-19: "see new materials and verification"). 1,363 rows over 44 files: 1,249 verified,
    79 to fix, 35 unverifiable. **Every fix is applied** and each file has the checker's summary
    beside it; a value the checker could not confirm is moved under "Unconfirmed" in its file
    and is not to be typed into the engine
    ([docs/research/library/README.md](docs/research/library/README.md), "The second check").
    The corrections were applied by assistants from the checker's findings and are not yet
    re-opened against the sources: a number is still checked against its source as it is typed
    in. Round four is seven more files (the face's parts, joint curves for walking and running
    as data, fabric, engine torque curves as data, bullet drag, sword balance, ion engines and
    re-entry), the delivery's `H9` being `H10` here.
  - **The pictures' repository is meant to be public** (the owner, 2026-11-19, of a delivered
    note that called the research private: "It wasn't meant to be private"). Round four's
    pictures are in it.
  - **Muybridge's own catalogue is the check for his plates** (the owner, the same day, of two
    plates held back: "verify the possible age range of that man"). His *Prospectus and
    Catalogue of Plates* (1887) numbers the model of every plate and says who they were. The
    men who walk, run and jump were students or graduates "aged from eighteen to twenty-four"
    (the man of plates 8 and 9, models 55 and 56, among them); models 27, 28 and 29 were boys
    of thirteen to fifteen (no plate of theirs was ever published here); and of the women,
    models 4 to 13, 15 and 19 were "of ages varying from seventeen to twenty-four", with no
    age given for models 2 and 20. **Nine unclothed plates of those women had been published
    in the pictures' repository** (eight since round two, one for an hour). The owner's rule
    for them (2026-11-19): "If these nine women are adults, that is fine to keep. Otherwise,
    if they're any younger than 18, but if they are clothed, that's fine, but if they are not
    any clothed, then toss those images out". None can be shown to be an adult and all nine
    are unclothed, so they were tossed: out of the repository's files, out of the delivery's
    folders and zip files, and out of its history (the repository was deleted by the owner
    and made again from the clean copy: seven commits, 359 pictures, the old commits no
    longer served). Plate 8 of the man went in on the owner's word ("That man in loincloth is
    not a issue then"). Every Muybridge plate of a person is held against that catalogue
    before it is placed.
  - **A fifth list, for the figure's surface** (the owner, 2026-11-19, told that the pictures
    have helped less so far than the numbers and what would help next: "Get me full list of that
    please. I'll pass this to Grok to investigate"). It is
    [docs/research/REFERENCE_ROUND5_BRIEF.md](docs/research/REFERENCE_ROUND5_BRIEF.md): the
    trunk as a surface (cross-sections most of all), the shoulder and the arm where it joins,
    hands and feet from six sides and in poses, and the numbers that go with them.
  - **It came the same day** ("additional files per your request"): 217 pictures and eight
    files of numbers, with their own checker's report. **Every picture was looked at before
    any was placed** (none shows a child; the 31 unclothed figures are all Richer's adult
    model; two dissections that show the groin were moved to the adults-only folder), and
    all 217 are in the pictures' repository, 576 there now. The numbers are in the library
    as A9 to A16 ([docs/research/library/README.md](docs/research/library/README.md), "Round
    five"). What they give the figure: **a waist and a hip are squarer than an ellipse** (a
    superellipse of exponent about 2.7 to 2.8 at the waist, 3.3 to 3.7 at the hip, by sex
    and build): the one squareness of 3 that the trunk was fitted with lies between them,
    so the next step is a squareness for each section; 86 dimensions of the
    hand and 33 of the foot; joint ranges, and where joint centres lie under the skin's
    landmarks, for the rig. The trunk file's means were held against the engine's own
    table of the same survey and its shape numbers against a solver written here: they
    agree. The rest is as delivered: checked as it is typed in. **Not found:**
    cross-sections at the nipple, waist and navel; a woman's trunk as a surface; builds; a
    foot moving; muscle thickness and attachments as numbers.
  - **A skeleton with every figure, so that a character is a change and not a rebuild** (the same
    message: "At the very least, basic skeletons alongside the figures you've made for me will be
    key to set it up so you can easily mutate or change into individual characters over time
    easily rather than having to keep rebuilding each time"). *Claude's reading:* the figure is
    already a recipe of numbers (a character is the base with other numbers), and what it lacks is
    the rig; so **the rig by construction comes next in part 4's slice 2**, before more work on
    the surface, with every body sharing one skeleton's names so that one motion drives them all.
  - **Rust only.** "Anything Python in here should be converted to Rust. The only programming language
    used here should be Rust. Zero-dependency is non-negotiable. So I'm willing for this to take
    time." This replaces "new checks may still be Python" (2026-11-15): **no new Python from this
    date.** What is tracked and not Rust: 83 scripts in `scripts/` then (81 since the comparison's harness was ported, the same day), two test-data generators in
    Python, one PowerShell generator (`kat_gdi.ps1`), the shaders' `build.sh`, and 79 GLSL shader
    sources, which the shader language replaces.
- **Where M6 stands** (kept current; ROADMAP M6 has the fourteen parts):
  - **Part 1, verifying faster: built, reviewed twice, gated, committed** (2026-11-16, branch
    `m6-verify`). The Rust pin, the gate and the live checks' runner in Rust (`tools/examen`), the
    sizing, CI's skip and its kept build folder. The full live audit through the new runner passed,
    70 of 70. **Not shown yet:** anything in the workflows, which run on GitHub at the first push and
    not before.
  - **Part 3, the engine paying from the first hour: built** (2026-11-16, branch `m6-first-hour`, on
    top of `m6-verify`; [docs/design/FIRST_HOUR.md](docs/design/FIRST_HOUR.md); neither branch is pushed
    or merged). A project gets its SDK from the engine; a module builds at any depth; a game is a file
    (`game.json`) that the project shell hosts with screens, words, settings, an outcome and a score;
    a project remembers (notes, and a brief at an agent's first call); `specula new` makes a project
    that builds and plays, and `specula play` runs it. Two reviewers that did not write it said not
    to merge three times; what they found is fixed (CHANGELOG, part 3), and on the fourth round both
    said merge. The full gate passed on `2a0c5669`.
  - **Part 3's measure went the wrong way** (2026-11-16; [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md),
    "Results (2026-11-16)"; [perf/leverage/2026-11-16](perf/leverage/2026-11-16/report.md)). The
    comparison of 2026-11-14 was run again with the same harness (now `examen leverage`), twelve
    builds, all judged. **Through the engine the same games cost more than they did in M5, not
    less:** 6.8 times the plain arm's tokens on the small job (M5: 5.2), 4.0 on the medium one (3.1),
    2.2 on the change (1.7), for the same checks passed. Two runs a cell: only the medium rise is
    beyond what two runs alike have shown, and no cell got cheaper. Part 3 closed what M5 found
    missing (an SDK for a project, screens and settings for a module-built game, the tick), and the
    builders used it; what it costs is requests, and they took more (50 and 61 on the medium job
    against 31 and 32). What the runs say would lower them is ROADMAP M6's item "The first hour,
    measured again": a whole game's headless check in one call, a clock that stays with the agent
    stepping it, settings an agent can show as kept, and memory that reaches an agent working from
    a shell (no builder called a memory tool or wrote a note). **None of that is built; whether it
    comes before part 4 is the owner's to say.**
  - **Three more passes at the first hour, by the owner's word** (2026-11-16, night, on that
    result): "3 passes. I need you to investigate the root cause of larger tokens than expected.
    Second pass starts now, at first hour, but I need 2 more passes afterwards... Note down every
    token spend. Assess. Pay attention to files and how agents are using the engine. We need to
    trace everything within those 3 passes." Each pass: every request of every run laid out
    (`examen leverage trace RUN`; each run's file under `perf/leverage/` has its ledgers), the
    cause assessed, the engine changed, reviewed and gated, and the comparison run again.
    - **The assessment** ([docs/design/LEVERAGE.md](docs/design/LEVERAGE.md), "Where every token
      went"): cost goes with requests times context. A third to two fifths of every engine build
      was the builder writing and debugging its own headless-check script; a fifth to a quarter
      was discovery carried by every later request (the engine's design document read for the
      game file's format); the rest waiting, engine care, and things looked for that were not
      there.
    - **The second pass, built** ([docs/design/FIRST_HOUR.md](docs/design/FIRST_HOUR.md), section
      9): `specula run FOLDER --ticks N --inputs ...` builds a project's modules, plays its game
      with no window by a script of inputs, prints its state as the game file's `report` shapes
      it and writes a frame (about 4 s; the same seed and inputs print the same state); the game
      file's `view`; Left and Right step a setting; `notes/how.md` is one page that answers what
      the builders looked up.
    - **The second pass, measured** (2026-11-17, on `bbe9fa36`; LEVERAGE.md, "Results
      (2026-11-17: the second pass)"; [perf/leverage/2026-11-17-pass2](perf/leverage/2026-11-17-pass2/report.md)).
      Fourteen builds, all judged. **Through the engine the small game now costs 1.7 times the
      plain build (it was 6.8) and the medium one 2.6 times (4.0), for the same checks passed.
      The change to an existing game got dearer: 3.9 times (2.2), with one or two checks fewer
      passed.** The small builds took 16 requests each (36 and 68 before). What still cost, by
      the ledgers: a module whose tick failed every time while the run printed a state as if
      nothing were wrong (fifteen and thirty-four requests in two of three medium builds, the
      largest single cost); showing that the game can be won, which needs a player that reacts
      (one builder started the engine 347 times); and, in the change round, a setting's value
      as words in three languages, which the engine did not do (ten to twenty-five requests).
    - **The third pass, built** (FIRST_HOUR.md, section 10; commits `80909a82`, `f5b3205e`,
      `6cf79f80`): a run whose module failed answers `module_failed` (which error, at which of
      the script's ticks) and never a state; what the rules log is said beside the state; a
      run's answer is one line; `specula run --follow` reads a step a line and prints the
      state after each, so a small program plays a whole game in one run; a choice setting's
      value is shown in the player's language; the guide says what was looked up. A model
      that did not write it said not to merge it twice, and what it found is fixed (a refused
      step that let go of a held input; a module's prints among the states; the game's tick
      called the run's; an unbounded line).
    - **The third pass, measured** (2026-11-17, on `6cfff79b`; LEVERAGE.md, "Results
      (2026-11-17: the third pass)"; [perf/leverage/2026-11-17-pass3](perf/leverage/2026-11-17-pass3/report.md)).
      Fourteen builds, all judged. **Through the engine the medium game now costs what it costs
      without it (219,007 against 217,869 weighted tokens; 1.0 times, from 2.6), with every
      check passed in both arms. The small game costs 1.5 times (1.7), and the change 1.5 times
      (3.9), where each engine build still fails one check the plain ones pass** (the same one
      in all six engine change builds of two passes: unexplained). What is left, by the
      ledgers: after its code is written a builder runs its game one run a request, a third to
      nearly half of a medium build; and the guide and the sample have to be read first.
    - **The fourth pass, built** (FIRST_HOUR.md, section 11; commits `3c44d1f2`, `e3c07a6e`,
      `e5d247e6`): `specula check FOLDER` plays every run a project's `checks/runs.json` lists
      and holds each state against what the check expects; a line a check, then a count. A
      check that expects nothing prints its state, so the runs a builder wants to see are
      written the same way and read in one request. `specula new` writes the sample's own
      seven. A model that did not write it said not to merge it once (a check's frame could be
      written through a link out of the project; `--settings` leaked a player's settings into
      checks), then merge.
    - **The fourth pass, measured: it made no build cheaper** (2026-11-17, on `d18e1131`;
      LEVERAGE.md, "Results (2026-11-17: the fourth pass)";
      [perf/leverage/2026-11-17-pass4](perf/leverage/2026-11-17-pass4/report.md)). Fourteen
      builds, all judged. **The medium game costs 1.1 times the plain build (207,057 against
      189,808; the three engine builds within 1.5% of each other, and all nineteen checks passed
      where the plain builds passed 17 and 18), the change 1.5 times as before, and the small
      game 2.2 times, up from 1.5.** Every engine build wrote a checks file and ran it, and none
      used it in place of single runs: the checks came on top. On a one-screen game that is the
      rise. Where they were worth something is the change (the builders ran the checks that came
      with the game, and added theirs) and what stays in the folder. The runs also found a
      defect: a range written as two comparisons never held; fixed in `e8097c2a` and `648cb757`
      (reviewed: merge).
    - **The three passes are done.** From 6.8, 4.0 and 2.2 times (small, medium, change) to
      2.2, 1.1 and 1.5; the best of the three measurements was the third (1.5, 1.0, 1.5). What
      is left, by the ledgers, is what a first meeting costs (the guide and the sample read
      before a line is written), which the engine pays back on a medium game by writing less
      and does not on a one-screen one. What comes next is the owner's to say.
    - **A fifth pass, by the owner's word** (2026-11-17, shown the fourth: "if Pass 4 didn't help,
      how do we fix this?"; FIRST_HOUR.md, section 12). A subtraction: the engine arm's extra cost
      is requests and the context each reads again, so the guide stops asking a first build for
      a checks file and a reacting player, first contact is one read, and what was looked up is
      where it is looked for. **Every ratio above flattered the engine a little**: each build's
      first shell here-document was refused and its first file written twice, which cost the
      plain arm more (its first file is the whole game); without it the third pass's medium game
      is about 1.1 times plain and its small game about 1.7 (estimates from the ledgers). From
      the fifth measurement both arms are told to use their file tool. A `large` job is written
      (`evals/leverage/jobs/large.json`) and not run: **a game made of modules cannot keep
      anything for its player between runs but settings** (a best score, a saved run), which a
      larger game needs and the engine has yet to build.
    - **The fifth pass, measured: the first measurement in a clean shell** (2026-11-17, on
      `9450e0f4`; LEVERAGE.md, "Results (2026-11-17: the fifth pass)";
      [perf/leverage/2026-11-17-pass5](perf/leverage/2026-11-17-pass5/report.md)). Fourteen
      builds, all judged, no file written twice. **Through the engine the small game costs 1.85
      times the plain build, the medium game 1.28, the change 1.25** (two change builds of three
      cost what the plain ones cost). It took back the fourth pass's rise on the small game and
      did not move the medium one; "the medium game costs the same" (the third pass) was the
      shell's fault flattering the engine. **The gap is before the first line of code**: an
      engine build has spent 49,000 to 74,000 weighted tokens when it writes its first file (the
      guide, the sample, a look-up: four requests) and a plain build about 6,000; after the
      first file the engine arm is the cheaper one on the medium game, by the means. At these
      sizes that first meeting is not paid back, and no change to the guide alone will make the
      engine cheaper than the plain build: what would is the engine doing more of a game's work,
      which these three jobs do not ask for.
    - **A sixth pass, at that gap** (the owner, 2026-11-17, shown where it is: "That gap needs
      to be looked at and fixed up"; FIRST_HOUR.md, section 13): one read that is enough (six
      files named by the guide, by `specula new` and by the brief); `cx.random_restart`, which
      a restart "with the same seed" needed and the SDK lacked (seven of eight one-screen builds
      had written their own generator); the volume's range; the checks' description out of the
      first read (`notes/checks.md`).
    - **The sixth pass, measured** (2026-11-17, on `57f515d0`; LEVERAGE.md, "Results
      (2026-11-17: the sixth pass, the first meeting)";
      [perf/leverage/2026-11-17-pass6](perf/leverage/2026-11-17-pass6/report.md)). Fourteen
      builds, all judged. **Through the engine the small game costs 1.61 times the plain build
      (1.85 before), the medium game 1.22 (1.28), the change 1.47 (1.25).** Read arm by arm: the
      engine builds got 15% cheaper on the small game, 8% on the medium one and did not move on
      the change; the plain builds, for which nothing changed, got 16% cheaper on the change,
      which is all of that ratio's rise. **Before its first file an engine build now spends
      40,000 to 57,000 weighted tokens (49,000 to 74,000 before)**; a plain build 6,000. Every
      new-game build used `cx.random_restart` and none wrote a generator; none wrote a checks
      file; all three engine change builds passed all nine checks for the first time. What is
      left before the first file is the brief, the one read, and a second look at the sample's
      other four screens, which every build still takes. Not built, and the owner's to say: the
      other screens in the one read, and a line in the guide on depth (one medium build drew its
      pieces at its background's depth and shipped frames that do not show them).
    - **The live audit after the passes** (2026-11-17, on `e5fbe695`): 69 of 70. `launch_check`
      fails on an expectation that went stale in part 3 (a project made from the start screen now
      opens on its own title screen, and the check expects a window with no text). Its script is
      Python, and the owner's word on changing one line of it was: "I prefer all to be Rust
      rather than having Python". So a script that needs any change is ported first, never
      edited. **Ported the same day**: `launcher_check` (`tools/examen/src/native/launch.rs`), with
      a `probe` module in `specula-platform` for another process's window (found, pictured, sent
      keys; the one place `gdi32` is declared). Every expectation of the script has its line in
      the port, the stale one brought up to date; a seeded fault (an opened project put last in
      the list) fails 8 of them; its pictures were checked by a model that did not take them;
      its reviewer found that no guard held the hands-off rule over Rust (the guard read
      scripts only), and `script_settings_guard` now reads the tooling and the probe too.
      The script's file stays, unchanged and no longer run, only because `m5_exit.py` takes a
      class from it.
  - **Part 2** (a script is ported to Rust before it is changed, never edited): the comparison's
    harness and the launcher's check are ported; 81 scripts are left in `scripts/` (79 of them
    Python), of which `launch_check.py` is no longer run (its port is `launcher_check`).
  - **Pushed** (2026-11-17, at the owner's "at your discretion", after the full live audit
    passed 70 of 70): `origin/main` holds parts 1 and 3 with the first five passes. CI's first run
    of part 1's workflows held on every job but Ubuntu's, which failed one test of part 3 that
    wrote its folders as Windows paths; fixed on the branch.
  - **Part 4 has begun, by the owner's word** (2026-11-17: "Feel free to advance into
    Part 4 and onwards now. Grok has completed deep research").
    - **Slice 1, the body as numbers: built** (2026-11-17; `specula-corpus`,
      [docs/design/CORPUS.md](docs/design/CORPUS.md)). The proportion tables are **computed, not
      typed**: `examen corpus tables` reads the two public data files of ANSUR II (6,068 people,
      93 dimensions; never in the repository) and prints the crate's table, and a test holds it
      against the survey's printed report. A body is a sex, a stature, a weight and fourteen named
      traits (legs, shoulders, hips, age and the rest), each moving every dimension that goes with
      it in the people measured; it gives a frame of 29 joints, what its sixteen parts weigh (de
      Leva 1996, read on the page's image), and two checks: proportions against people of the
      figure's stature, and whether a still pose stands. Limits are the people measured (17 to 58
      years; no children, no old age), and the frame is flat (no spine curves). A body whose
      traits together would fold it is refused by the part that gave way; within 4 standard
      deviations together none can be (proved by a test). A model that did not write it said
      not to merge it once: every sourced number held, and three things did not (bodies inside
      the first limits with the navel over the ribs; checks that changed when a hand or a foot
      bent; a typed table too few tests held). All three are fixed.
    - **Slice 2, the figure's surface: in progress, and it does not look right yet**
      (2026-11-17; [docs/design/FIGURE.md](docs/design/FIGURE.md); checkpoints `da99962f` and
      `067011b7`). Built: bone chains (`chain`, `round_cone`), columns of sections (`column`), a
      `stretch` and a near-union in the distance-field modeller, a mesher that leaves out of each
      block what is too far to matter (exact, and what made a finer figure affordable), and the
      shape `figure`: six parts of a person from a measured body, each on its own grid, hands with
      fingers, feet with toes. **Eleven looks in the engine are recorded**, the bad ones kept
      (`gallery-full/part4/figure/NOTES.md` on this machine). Independent checks of the sixth,
      the eighth and the eleventh did not pass any of them as described.
      **The figure is now measured, not only looked at** (2026-11-17, after the reference
      pictures came): a test cuts the built mesh at each height the survey measured people at
      and holds breadth, depth and girth against the body's own rows
      (`the_figure_measures_what_its_body_does`; `figure_measures` prints the table). It found
      in an hour what eight looks had not, and a change that looked better and measured worse.
      **Measure before looking, and look before saying.** Every breadth and depth is within 3%
      and most girths within 5%; with ellipses for sections a tape round the waist read 6%
      short, and the trunk's sections have had a squareness since (3, where an ellipse is
      2), which brought the waist and the hips to within 2%; the chest still reads a
      twentieth to a sixth short (next: the muscles that go from the chest and the back to
      the arms, over which a tape passes). **It has a skeleton and has walked** (the same evening): a recipe's rig can be the
      measured body (`"rig": {"figure": {...}}`, eighteen bones under the names the motion
      tools look for), and a generated gait moves it. The first walk showed that the pelvis
      tears, because a vertex binds to the nearest bone; the body needs weights that know how
      thick each bone's flesh is; it is bound that way now (`Rig::bind_fleshed`), and the tear
      is gone. **Slice 2 has been reviewed by a model that did not write it, which said not to
      merge it**: four major findings (a steep column's field overstated distance; the mesher
      kept a tree a quarter block, up to 742 MB; arms held close cut the thighs out of the
      body; the root bone took the legs when listed), all mended, each case now a test. **A
      second model reviewed the mends and said merge**; it also found why a hand could have a
      pinhole, which the builder had traced and not explained (FIGURE.md, "The review of
      slice 2"), and that is mended too. What else is wrong and what is missing is in
      FIGURE.md: a trunk like a column, tubes for legs, a seam at the neck, a flat back, and a
      walk that is a lunge (the motion library's work, from the measured curves in H1).
    - **Roundness as one number: built** (2026-11-18): `roundness`, 0 to 1, on the modeller's
      box and cylinder and on a rectangular outline (PROCEDURAL_MODELLING.md, tier 3). **Its
      review, with the squarer trunk's, said not to merge**: three major findings (the field was
      not in metres; it was infinite far away; a square column's bounds cut its corners) and
      four minor, all mended, each case now a test. **A second reviewer said not to merge the
      mends**: `round` on a thin rounded board still came out up to five times too far at its
      corners (one touching plane is not enough beside a thin edge); mended by steps toward
      the nearest point. **A third reviewer said not to merge that**: at roundness 0.8 to 1,
      and for a swelling past twice the thinness, it was still up to 4.7 times out, and the
      field had a step where the extra planes were switched on. **Outside the shape the field
      is now the distance itself** (the nearest point of the surface is solved for; held
      against a distance found by search to a few parts in a hundred thousand), at a cost said
      as what builds: a cube of roundness 0.3 at resolution 64, refused at 96; at roundness 1
      and 0.5 a third and a half of the work limit at 96. **A fourth reviewer said not to
      merge that**: the maths held, and a sample far from the shape cost five times its work
      weight (a box with a rounded plate cut out of it ran 3.8 s inside the 1.5 s budget),
      and at roundness 0.05 the solve could stop early with a step in the field. The solve
      now goes by Newton's steps on everything at once, in double precision, with the sure
      way as its fallback. **A fifth reviewer said not to merge that**: the accuracy and the
      continuity held everywhere it looked, and far from a rounded needle the first steps
      gave up (71 steps a sample; a cut that ran 3.6 s). They are now taken on each share to
      the power less one, where every equation is nearly a straight line: 13 to 18 steps a
      sample for a plate, a needle, a cube and a board at six roundnesses (a test), and the
      cuts the reviews found run inside the budget. A roundness of 0.5 is no longer a cheap
      one. **A sixth reviewer said not to merge that**: the distance held everywhere (to 1.3
      parts in ten thousand against a nearest point of its own, on shapes to 1:1000), and a
      recipe within the parser's limits made the solve panic (a rounded box under seven
      scales of a thousandth: a sample too far for numbers), and a foil or a hair cost up to
      1.7 times a plain cut's rate under the one weight. Mended: such a sample has the bound
      for its distance, thin rounded shapes have a weight of their own, and the weights are
      set against the plain primitives' rate and not against a time. **A seventh reviewer
      said merge**, and found the thin weight's line in the wrong place (the solve's steps
      are the same down to a twentieth as thick as long): it is graded by thinness now.
      Owed: the rates measured again on a quiet machine. Two
      limits of the same kind are written down and not mended: inside a rounded shape the
      field is a bound, so a `shell` or a shrinking of one is thinner than asked beside its
      corners; and a column's field is in units of its narrower half size, so `round` and a
      blend's radius reach further along its broader side.
    - **A distance-field node's `rotate` turned its field the wrong way, since the modeller was
      written** (found by that second reviewer, 2026-11-19; PROCEDURAL_MODELLING.md, tier 3).
      Mended, with a test. Seven recipes turn such nodes, four of them Sepulcra props the owner
      approved in M4 (`grave_rock`, `ruined_wall`, `statue_angel`, `statue_mourner`): their
      angles were converted so that each looks as it did, and measured before and after they
      differ by a few triangles in thousands. The asset tests' two pinned hashes moved for it.
      **Looked at in the engine side by side** (2026-11-19: the four props from four sides, in
      the world's light and in clay, before and after; a model that did not take the pictures
      found each pair the same). **Its reviewer found the mend right** (with every other
      modifier, nested, and the eleven conversions exact), and that two nodes of the fidelity
      set should not have been converted: the runestone's grooves and the skeleton's upper arm
      are put back as written, where the mend now shows what the recipe meant. So is the
      skeleton's pelvis, after both ways were built and held against Richer's plate at the
      owner's word ("I'd like you to experiment on both"): as written its wings rise up and
      outward as the hip bones do (pictures in `gallery-full/part4/pelvis/`, checked by a
      model that did not take them).
    - **The form views: built** (2026-11-18; [docs/design/FORM.md](docs/design/FORM.md)):
      `sight_frame` and `sight_orbit` take `form`: `clay`, `normals`, `curvature`, `stripes`.
      Shape alone, with none of the world's light; exact on both adapters. **Look at any form
      in clay and curvature before saying what it is like**: the first look at the figure in
      them found in one pass what sixteen looks under the world's light had not (the plate is
      on the backs too; the arm lies on the shoulder as a separate capsule; the trunk is a box
      with hard vertical edges; an elbow band; a seam where the legs meet the trunk).
      **The studio's sheets take them as passes** (2026-11-19: `"passes": ["colour", "clay",
      "curvature"]`; not reviewed). Not yet: `studio_lineup` and `sight_compare`. **Its reviewer said not to
      merge**: clay's light did not come with the camera when looking down; `gi` views with a
      form drew things that are not there; an inside-out shape looked right, and the
      description promised hard edges in curvature, which cannot show them; and the world's
      frame was 2 to 5% slower on the iGPU for a branch. All mended (the views are behind a
      specialisation bit now, so the world's pipelines hold none of them; clay stains an
      inside-out face red; hard edges are looked for in `normals` and in the form facts).
      **A second reviewer said not to merge the mends**: the first form capture compiled its
      pipelines on the engine thread (440 ms), one scene of the world's was still 2.2% slower
      on the iGPU, and clay stained outlines under an orthographic camera. Mended: the views
      are in shaders of their own (the world's four compiled shaders are byte for byte what
      they were before the feature, and the world's frame times the same: 1.5211 ms against
      1.5208 ms on `cubes_10k`, twelve alternating rounds), their pipelines are built on a
      worker with the capture waiting, and an orthographic camera is told. **A third reviewer
      said not to merge those**: a form pipeline that failed in a later build was counted as
      nothing lost (the next form frame compiled it), a sheet's form tile was framed without
      the reference figure's room, and a waiting form capture held the others. Mended, each a
      test shown to fail on its fault; a frame now only looks a form pipeline up; the sheets
      have a live check in Rust (`form_sheet_check`). **A fourth reviewer said merge**, with
      four small findings, mended (a sheet's silhouette tile framed as its colour tile; the
      live check says what it holds; a failed form set is asked for again).
    - **The form checks: built, but for faceting** (2026-11-18; FORM.md): loose pieces, razor
      edges, shading seams, smeared creases and relief by size, measured from the mesh alone and in
      every asset's facts; `style.toml`'s `[form]` gives limits (only loose pieces are checked
      unless it does). **The first run found a fault in an approved prop**: one of
      `pediment_fallen`'s rubble blocks hangs about 5 cm over the ground. It is one of the 33
      props the owner approved at the window in M4, so the recipe is left as approved and the
      fault is written into Sepulcra's `style.toml` (`[form.allow]`); **mending it is the
      owner's to say.** **Its reviewer said not to merge**: the shape's size was taken from
      every vertex, used or not (one stray vertex hid a loose piece, and one at infinity made
      the check pass on nothing), and a dome on a flat slab read as perfectly smooth; five
      minor findings beside. All mended, each case a test. **A second reviewer said not to
      merge the mends**: faceting left out every edge that turns more than 30 degrees (a
      ten-sided smooth cylinder read 0), a far triangle still voided the loose check, one
      floating piece was never loose, a mesh too large to measure passed. Mended; relief is
      said plainly to be a coarse measure (a turned shape moves a share by up to a quarter).
      **A third reviewer said not to merge those**: faceting, measured over edges shaded as
      one surface, still read 0 on the engine's own coarse cylinders (the modeller hard-shades
      them) and 90 degrees on a smooth-shaded box. **Faceting is taken out** after three
      tries: a mesh alone cannot say what was meant to be round (a box and a four-sided
      cylinder are one mesh). It belongs at build time, from what a recipe says is round
      (segments, subdivisions, a field's resolution against its radii): **not built, and owed**
      (the owner asked for every form check). **A fourth reviewer said not to merge those**:
      one stray vertex with a normal of no length, used by nothing, switched the shading
      measures off, and the checks that limit them passed on nothing; a triangle written
      from another corner moved the small relief by a tenth; vertices no triangle uses were
      all welded before the first look at whether to stop; unreadable triangles passed.
      Mended (shading judged on the vertices in use, and unmeasured shading fails its checks;
      `unread_triangles`). **A fifth reviewer said merge**, with small findings, mended (a
      normal of no finite length is none; a mesh whose faces are all written twice fails
      the crease checks as not measured; triangles with no area fail the loose check).
    - **Shape language: built** (2026-11-19; FORM.md, "Shape language"): how round, square
      or pointed an asset's outline is, in every asset's taste measures; a style may ask for
      one (`[taste] shape_language`), and the `storybook` preset asks for round. With it the
      five items of "form within an agent's reach" are all built. **Its first reviewer said
      not to merge it** (corners counted on a polygon with hard lines between the classes:
      it flipped under small turns), and it was written again as the net turn of a smoothed
      outline within a window. **Its second reviewer said not to merge the rewrite**, with
      three findings that were true: a box rounded by a sixteenth of its size read a fifth
      round at one angle and nine tenths a degree on (**the shape's size was its bounding
      box's longer side, which grows as the shape is turned**: it is the greatest width
      now); a spike read under half pointed (a point counts double now); a disc with one
      slot cut in it failed "round" (only outward turning counts now). It is measured on
      512 pixels, and under 110 nothing is read. A test turns 28 known shapes through a
      quarter turn (steady to a tenth) and sweeps a corner's rounding every three degrees.
      **What it cannot say steadily is written down**: a corner rounded by a twentieth to
      a fourteenth of the size, and a part about a fifteenth of the size thick. Sepulcra's
      recipes lead pointed (30 of 56), square (20) and round (6): no check is asked of it.
      **These mends are not reviewed yet.**
    - **Where its verification stands** (2026-11-19, evening): the full gate passed on
      `466e262b` (the mends of the fifth round of reviews and the light report: 3,296
      tests, the repeats, the slow-worker run, the speed budgets, clippy for both targets,
      both builds), and seven live checks on it (window, sight, terrain, GI, studio, procgen,
      `form_sheet_check`), the window's pictures looked at on both adapters; the same had
      passed on `24bfe7de` before those mends. **Owed:** a review, by models that did not
      write them, of the fifth round's mends (shape language, the rounded field's guard and
      thin weight, the form checks) and of the light report. Nothing of part 4 is pushed.
    - **Faceting, from what a recipe names round: built** (2026-11-19, evening; FORM.md,
      "Faceting, from what the recipe names round"). The modeller records, for every shape
      a recipe names round (cylinder, cone, sphere, torus, capsule, lathe, a sculpt), the
      turn between its neighbouring faces and its radius; an extruded outline of ten sides
      is a prism and says nothing. Every asset's facts have the coarsest turn whose facets
      show at the asset's size, and a style may limit it (`[form] max_facet_deg`: the check
      `faceting`). With it **every form check the owner asked for is built**. Sepulcra's
      props are faceted on purpose (the median 90 degrees a face, rings of three sides):
      its style gives no limit. **Not reviewed yet.**
    - **The light report: built** (2026-11-19; [docs/design/LIGHT.md](docs/design/LIGHT.md)):
      `perf_light` draws a view offscreen as the world has it and with each light feature
      that is on left out (the sun's shadows, ambient occlusion, bloom, anti-aliasing, fog,
      the sky, grading, baked GI, the point lights), in alternating rounds, and answers what
      each costs by the adapter's own timestamps, whether it is told from the noise, whether
      the view holds a budget and what to give up when it does not; and what the engine's
      light has and has not, whatever the machine. It changes nothing and writes nothing.
      **The engine has no quality preset to pick for a machine it has never seen: it
      measures, and whoever builds the game decides.** On this laptop's
      integrated GPU Sepulcra's scene takes 2.6 ms at 720p and 5.6 at 1080p, nine tenths of
      it light and post (bloom, the sky and ambient occlusion the dearest); from a debug
      build the discrete GPU's times wandered and the report said no cost was told from the
      noise, which is the answer wanted there. **Not reviewed yet.**
    - **Then:** the same in 2D; `form`
      on `sight_compare` and the line-up. And on the figure, by what the form views show: the plate front and back, the arm
      over the shoulder, the trunk's hard edges, the seam under the skull, the muscles to the
      arms, the knees' rings, one place for a recipe's body fields.
  - **The research behind parts 4 and 5 is in** ([docs/research/library/](docs/research/library/README.md):
    45 topics, each with a second checker's report; the owner had it researched over two days,
    "ensuring the license allowed this usage"). Claude's review of it the day it came
    ([REVIEW.md](docs/research/library/REVIEW.md)): useful, most of it fit to build from, about
    forty numbers re-fetched from their sources with none found wrong; the body, the face's
    measures, four-legged proportions and gaits, a walk and idles, waves, landforms and
    materials can be built from now; water's colour, leaves and roots, ground as render values,
    the face's movement, hairstyles, running and vehicles are thin and are researched again
    when their part comes. A number is still checked against its source as it is typed in.
- **M5 is done (2026-11-15). M6 started on 2026-11-16 with the scope talk above.** What was written
  for it before the talk:
  - **M6 as it stood before the scope talk** (an index; the details are below and in ROADMAP M6,
    which now has the sizes and the order).
    - *What M6 is:* the proto-1.0, "basic and good enough", released for Windows and macOS under a 0.x
      number, before December if it can be and otherwise before Christmas ("I can wait").
    - *Its two focus items:* what building through the engine costs (the numbers below), and verifying
      in less time.
    - *Talked through on 2026-11-16 (above):* the Rust version policy and how long verifying takes.
      Sized in ROADMAP M6: macOS, and Sepulcra's maps 2 and 3.
    - *ROADMAP M6's own items:* `specula export`; a project gets its SDK from the engine; a module
      builds in a folder of any depth; the repository's tooling in Rust; shipping basics; asset packing;
      Steam as an optional extra; the Steam Deck pass; soak tests; an agent sets the variables on a
      machine it has never seen; the engine pays off by game size; game systems as optional data and
      memory for whoever builds the game (2026-11-16); form within an agent's reach (2026-11-16); cloud
      swarms with per-agent tokens; an adversarial review before any marketing; a private held-out
      eval set; the release order; mods and outside extensions; and Sepulcra shipped as the demo
      with its three maps (2026-11-16).
    - *On other tracks, and needed for M6's release:* macOS natively (ROADMAP's Platform track: a
      Metal renderer, window, input, audio, module loading), behind the shader language that track
      puts first; and a module-built game reaching interface screens, strings and saved settings.
    - *After M6:* Linux; the ladder of light and the rest of the research note; the owner's large
      commercial game; mobile; whatever else matching AAA tools takes.
    - *How a change is verified* (since M6's first work, 2026-11-16): `cargo run -p specula-examen --
      gate`, which sizes itself to what changed (AGENTS.md, "The gate": documents take seconds, the
      engine takes everything), on the Rust the repository pins; what touches the window, the GPU or
      the tools also passes the live checks (`cargo run -p specula-examen -- live`).
  - **The owner's list** of what only a person can judge at the window: [docs/M5_CHECKLIST.md](docs/M5_CHECKLIST.md).
    Nothing in M5 waited on it; what fails there is M6's.
  - **M5's measured baseline and why** ([docs/design/LEVERAGE.md](docs/design/LEVERAGE.md)): the numbers M6 is
    to move, with the same harness.
  - **ROADMAP M6** (the items marked with the owner's words of 2026-11-14), M5's open follow-ups, and "What
    the exit proof and the comparison leave for M6" at the end of ROADMAP's M5.
  - **macOS is to be sized honestly first** (the owner hopes to release for Windows and macOS before
    December; on 2026-11-16: "If not before december, then before Christmas. I can wait"): a Metal
    renderer, its window, input and audio, and module loading do not exist yet.
  - **The owner's additions of 2026-11-16** (game systems as optional data; memory helpers for the
    agent that builds a game): below, and in ROADMAP M6. Noted only. **M6 is the proto-1.0**: "basic
    and good enough"; the version number stays 0.x, and a true 1.0.0 is when the engine is near an AAA
    studio's tools.
  - **Research done before M6, at the owner's request** (2026-11-16):
    [docs/research/FORM_AND_LIGHT.md](docs/research/FORM_AND_LIGHT.md), on putting round and human form
    within an agent's reach and on light that mimics ray tracing without the hardware. It ends with a
    proposal in order and three questions for the owner. Nothing in it is built.
  - **The numbers M6 starts from** (measured; a later figure is compared with these, by the same harness):
    - *What building through the engine costs, against building without it* (2026-11-14,
      [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md); weighted tokens, the mean of two runs a
      cell): a one-screen game 476,696 and 540,002 (two engine builds) against 96,990; a medium game
      582,586 against 187,084; a change to it 181,378 against 109,716. About 5, 3.1 and 1.7 times, for
      the same checks passed within one. Requests to the model: 27 to 39 against 8; 31 and 32 against
      8 and 10; 11 and 13 against 10 and 11.
    - *The same, after M6 part 3* (2026-11-16, on `2a0c5669`; the figure a next attempt is compared
      with): a one-screen game 678,546 against 99,776; a medium game 928,953 against 231,054; a
      change 246,634 against 114,280. About 6.8, 4.0 and 2.2 times, for the same checks passed within
      one. Requests: 36 and 68 against 8 and 7; 61 and 50 against 9 and 13; 33 and 14 against 12
      and 13.
    - *The same, after the second pass at the first hour* (2026-11-17, on `bbe9fa36`): a
      one-screen game 169,613 against 97,184; a medium game 446,356 (three runs: 277,124 to
      672,915) against 171,273; a change 392,016 (three runs) against 99,336. About 1.7, 2.6
      and 3.9 times. Requests: 16 and 16 against 7 and 9; 67, 24 and 39 against 8 and 9; 38,
      45 and 49 against 9 and 11. Checks: alike on the first two; 8, 8 and 7 of 9 against 9
      and 9 on the change.
    - *The same, after the third pass* (2026-11-17, on `6cfff79b`): a one-screen game 145,926
      against 95,483; a medium game 219,007 (198,595 to 242,012) against 217,869; a change
      122,392 against 81,746. About 1.5, 1.0 and 1.5 times. Requests: 13 and 13 against 7 and
      8; 18, 19 and 17 against 15 and 10; 12, 11 and 18 against 8 and 11. Checks: all passed on
      the first two in both arms; 8, 8 and 8 of 9 against 9 and 9 on the change.
    - *The same, after the fourth pass* (2026-11-17, on `d18e1131`): a one-screen game 208,860
      against 94,670; a medium game 207,057 (205,269 to 208,160) against 189,808; a change
      145,347 against 97,842. About 2.2, 1.1 and 1.5 times. Requests: 17 and 25 against 8 and
      7; 15, 17 and 16 against 9 and 13; 15, 24 and 13 against 9 and 12. Checks: 14 and 13 of
      14 against 14 and 14; 19, 19 and 19 of 19 against 17 and 18; 8, 9 and 7 of 9 against 8
      and 9. With nothing changed for it, the plain arm moved by 1%, 13% and 20% between the
      third pass and the fourth: read a difference smaller than that as nothing.
    - *The same, after the fifth pass, in a clean shell* (2026-11-17, on `9450e0f4`; both arms
      told to write files with their file tool: **compare later figures with these, not with
      the ones above**, in which every build wrote its first file twice): a one-screen game
      132,122 against 71,322; a medium game 196,431 (167,638 to 223,545) against 153,572; a
      change 141,703 against 113,478. About 1.85, 1.28 and 1.25 times. Requests: 12 and 11
      against 6 and 10; 14, 16 and 17 against 8 and 12; 20, 13 and 12 against 12 and 14.
      Checks: 14 and 14 of 14 in both arms; 19, 19 and 19 of 19 against 19 and 17; 8, 9 and 9
      of 9 against 9 and 8. Without the shell's fault the third pass comes to about 1.7, 1.1
      and 1.5 and the fourth to 2.7, 1.26 and 1.5 (estimates from their ledgers).
    - *The same, after the sixth pass* (2026-11-17, on `57f515d0`, the same clean shell): a
      one-screen game 111,938 against 69,612; a medium game 180,729 (163,128 to 206,039) against
      148,684; a change 140,412 against 95,452. About 1.61, 1.22 and 1.47 times. Requests: 13
      and 12 against 7 and 7; 15, 16 and 20 against 8 and 9; 23, 13 and 11 against 11 and 9.
      Checks: 14 and 14 of 14 in both arms; 19, 19 and 18 of 19 against 19 and 18; 9, 9 and 9 of
      9 against 9 and 9. Between the fifth and the sixth the plain arm, with nothing changed for
      it, moved by 2%, 3% and 16%.
    - *What verifying costs* (2026-11-15 and 16): 3,049 tests in the gate; the tests themselves about
      2 minutes; the gate about 25 minutes for each Rust version; the audit's 70 live checks about an
      hour; CI from nothing 10 minutes on Linux and 17 to 25 on Windows.
    - *Size* (2026-11-14): about 396k lines of Rust in 35 crates, none from a third party.
    - *Frame time:* the reference is the Intel iGPU; the budgets are data (`perf.example.toml`) and
      the measured baselines are in [perf/README.md](perf/README.md) (`scripts/perf_baseline.py`).
- **M5 as built** (2026-11-06 to 2026-11-15):
  - **Merged:** map 1, audio, timelines, the render follow-ups, the UI sight tools, the editor's core, text,
    the gameplay modules, localisation, the input monitor, accessibility with Sepulcra's shell, tool kits by job,
    GPU work off the engine thread, the editor's Swarm tab with "highlight and ask", the editor's draw tools
    (sculpt, splat, vertex paint, sketched paths), the gate (then `scripts/gate.py`, now `tools/examen`; AGENTS.md "The gate"), and the
    launcher (owner, 2026-11-13: the engine is openable, lists the projects worked on with the last one first,
    resumes it or opens another from the start screen or by asking an agent, and is found and called from a
    terminal: `specula projects`, `open`, `new`, `list`, `tools`, `call`, `stop`; one engine a project;
    Windows first, the macOS and Linux variants with those ports: [docs/design/LAUNCH.md](docs/design/LAUNCH.md),
    then `scripts/launch_check.py`, now `launcher_check` in `tools/examen`), and asset export for other engines (owner, 2026-11-13: assets are data that
    is reused, copied and modified, and easy to carry elsewhere: `model_export` writes any recipe as a `.glb`
    with its baked textures, rig and clips, and says what it does not carry;
    [docs/design/EXPORT.md](docs/design/EXPORT.md), `scripts/export_check.py`).
  - **The performance pass is in** (2026-11-14; what was measured and why each change was made is in
    [perf/README.md](perf/README.md), the paragraphs of that date). What it leaves an agent to work with:
    - **A call sent while the engine starts waits for the start** (`startgate.rs`): no retry around a first
      `agent_hello`. A start that fails refuses its callers at once.
    - **A stall says whether the engine worked or waited:** `engine.stall` carries `cpu_ms` (the engine thread's
      own run time) and `lock_ms` (its wait for the model library's lock). On a quiet machine the live checks
      raise no stall; beside compilers nearly every stall is a wait. Read those two before chasing a stall.
    - **`engine.started`** says what the start spent its time on, stage by stage. With the Vulkan SDK
      installed, the validation layer's shader checks are about 3 s of every start.
    - **Scripts try once.** The retries that made live checks pass by luck are gone (`edswarm_check`,
      `gallery_check`, `m3_exit`, the first `agent_hello` everywhere): a miss is a finding.
    - **A slow window frame says what it did:** its `engine.stall` carries `top`, the frame's zones that
      took the most time themselves.
    - **`models.lock_held`** (a trace event) names who held the model library's lock for 2 ms or more, and
      where it was taken. It found the cause of the frames that waited beside a busy machine: a build
      request read the disk under the lock. A request reads without it now (`Library::request_plan`), and
      `model_build` makes its whole request on a worker.
  - **The measured comparison is in** (2026-11-14; [docs/design/LEVERAGE.md](docs/design/LEVERAGE.md),
    `examen leverage` (then `scripts/leverage.py`), [perf/leverage/2026-11-14](perf/leverage/2026-11-14/report.md)): the same three
    game jobs built through the engine and without it, fourteen builds, each judged against one list by a
    model that built neither. **At these sizes the engine costs more for the same result**: about 5 times
    the tokens on a one-screen game (5.2 over four engine runs on two builds), 3.1 times on a medium one,
    1.7 times on a change to it. It is M5's baseline, to be moved in M6 (the owner's words below), not
    something to tune away in M5.
    - **Why it costs more is measured too** (`examen leverage where RUN`): three to five times the requests,
      each reading the whole context again, nine or more before the first line of code; and about twice to
      four times the words written (a Rust module and its glue against one Python file). Failed builds are
      not the cause. In the change round, where the game's own documents say how it is built, the two arms
      take about as many requests.
    - **Judges disagree, and a first judging of the change games had to be done again** (the judge was not
      told what the game passed before the change; one item was unclear in the brief). Both judgings are in
      the report. Treat a difference of one check between two cells as nothing.
    - Builders and judges run as **processes of their own** (`examen leverage run`): a subagent of a session
      with this repository open is shown AGENTS.md and HANDOFF.md, in both arms.
    - **Tokens are counted by kind** (new in the context, read again from the cache, written out) with one
      weighted figure; the single number Claude Code reports for a subagent is its context's size at the
      end, not what the run used.
    - From a terminal an agent now has: `specula tools` (names by family), `specula tools NAME` (one tool's
      description and arguments), `specula call TOOL @args.json`, and `--as NAME --role lead` for a lead the
      operator granted. A refusal for an unknown argument lists the ones the tool takes. **A lead the
      operator named is never frozen for a refused try**, whatever role its hello said (it was: one call
      without `--role` locked it out, and a run whose only grants are leads had nobody to clear it).
    - A game project outside this repository keeps its SDK in `crates/specula-sdk` and `crates/specula-core`
      (the only place the build service trusts it); the harness copies it there (ROADMAP M6).
  - **The exit proof** is `scripts/m5_exit.py` (ROADMAP M5, "Exit proof"): assets made once, saved, reopened
    and reused, 2D and 3D, in a project outside this repository, on both adapters. **The final audit** is
    the live checks' runner (then `scripts/live_checks.py`, now `cargo run -p specula-examen -- live`): every live check, one after the other, each on its own ports (70 runs, all
    passing on 2026-11-15, after thirteen checks that had gone stale were brought back and one engine
    defect it found was fixed: a capture's forced level, `sight_frame {lod}`, is uploaded for it); run it after
    anything that touches the engine's window, GPU or tools.
  - **Known limits carried out of M5:** the editor's panels still read one pointer for the human and an agent
    (docs/design/EDITOR.md, "Known limits"); a game made of modules and data has no interface screens (only
    a hosted game's executable reaches `specula-ui`, the string tables and saved settings); a new project
    has no style until someone writes one; three engine-thread disk reads are open on the roadmap.
  - **The lead's running notes** are in `.swarmnotes/m5/lead/m5tracker.md` (gitignored, this machine): every
    lane's state, review reports (`reviews/`), and the list of items carried to the performance pass.

- **Owner, 2026-11-16: more for M6, all optional; noted, not built.** "The new feature which I should have
  thought off earlier": a storyline tree ("start of story, on quest init, choices that impact it"), a
  reputation system, an optional RPG set-up, conditions in the game ("like winning conditions for races,
  or scoring system"), captions that agents act the story through ("for devs and studios that cannot
  afford human voice worker or already have voice packs that can be installed") which a player can hide,
  quests and editing them, and music the same way (generated by an agent, or people's own packs). "All of
  these can be optional." And memory: "notes that agents can use to note about the game, both for memory
  and system", with the harness a game is built through given memory helpers too, "as it will be hard to
  remember every detail". The seven agent tools are the model: "Go ahead and copy some functions and put
  them in the engine if it makes sense. Just make sure full zero-dependency remains."
  - **Nothing of it is designed or built** ("Do not implement anything, just add the note for handoff").
    ROADMAP M6 has the item ("Game systems as optional data, and memory for whoever builds the game")
    with what each part can build on: the four gameplay modules, the timeline's subtitle track, the
    accessibility settings, the audio crate, the project store.
  - **Claude's notes for the scope talk.** The memory helpers serve M6's first focus item directly: the
    engine cost least where a game's own documents said how it was built. Captions and a player's
    settings need a module-built game to reach interface screens first. And this is a large addition
    to a milestone that already holds macOS, export and the tooling in Rust.
  - **The owner's answers, the same day.** On its size: "I agree that this is a large addition, but
    this is something that needs to be sorted out now as many devs will be non-tech and not really
    understand this." So it is M6's, not put off past the release; the scope talk sizes and orders it.
    On the release: "If not before december, then before Christmas. I can wait." On speech, asked
    whether the engine should make it: "Engine should only produce the [speech] IF AI agents like
    yourself CAN produce speech. Otherwise, the AI agents without those capabilities cannot tell the
    human devs that they can produce it or use it to produce speech." So what speech a project has is
    read from the project and told to the developer in plain words, never taken from what an agent
    says of itself. Claude cannot make speech (it writes text). What the engine could make with zero
    dependencies is in the ROADMAP item: voiced babble under the captions (small), spoken words from a
    formant synthesiser (a machine's voice, weeks, a language at a time), and natural voices only as
    packs made outside.
  - **M6 is the proto-1.0** (the owner, told plainly that after M6 the engine covers most of what an AAA studio's
    tools cover, at less depth: stylised rendering, an editor with a core and not Unreal's breadth,
    animation that is "good enough", no multiplayer, consoles or mobile, and one shipped game as proof):
    "It's now v1.0. Basic and good enough. But to get it to match AAA, that's post M6. And we'll be
    tuning that and fixing it." Asked whether that means the crates become 1.0.0: "don't worry about
    1.0.0.0. whatever version number you use, it'll be the proto 1.0. I'm actually thinking on this, and
    0.3 is fine. To get to true 1.0.0 is when this tool reaches to a similar capability as AAA tools
    like unreal engine." So M6's bar is complete and dependable at a basic level on Windows and macOS,
    released under a 0.x number; matching AAA tools is the work after it (ROADMAP's Fidelity track and
    long-term list), and 1.0.0 is kept for that. Older lines in this page and in ROADMAP that say "after
    v1.0" (mobile, realism, better animation) were written when v1.0 meant this release: read them as
    "after M6".
  - **Taste and realism, researched before M6** (the owner, the same day): "You can generate graphical
    assets but to get really close to a good one, like round stuff, more human stuff.. I think this
    engine will help you with it. What we need you to do is do your research and see if you can add as
    much tastes as possible for AI agents to handle before we work on M6." And: the engine "needs to
    scale from very basic shapes to actually realistic figures and some level of raytracing (Don't use
    Nvidia's one. I think how it was done is too inefficient. You can come up with efficient ones that
    mimics it". The note is [docs/research/FORM_AND_LIGHT.md](docs/research/FORM_AND_LIGHT.md). In
    short: agents are weak at round form because they describe shapes in numbers and cannot see
    curvature, so taste is carried by handles whose plain result is round (one number for roundness,
    bone chains, a body from a skeleton), by measures (loose parts, razor edges, faceting, proportions)
    and by views where flaws make patterns; this year's studies say a model's own judgement of 3D form
    is not yet to be trusted. Human figures: the engine's own stylised one first, open parametric
    models (Apache 2.0 ones exist since late 2025) as packs on the project's side for realism. Light:
    distance fields, probes and screen-space traces stand in for ray-tracing hardware, as Unreal's
    default path and AMD's open one do; the engine's recipes and its baked probes make that a shorter
    road here than elsewhere. **No prototype was made and no cost on the iGPU is known.**
  - **The owner's answers to that note's three questions** (the same day). The five small items
    (roundness as one number and bone chains; the form checks; the form views; the engine's own figure
    from a skeleton; shape language in the presets) are all M6's: "Add all. This engine SHOULD help you.
    Not leave you lacking, and dealing with people's complaints." ROADMAP M6 has them ("Form within an
    agent's reach"). Human figures: the engine's own body is built up here with no outside data
    ("Only if needed. Zero-dependency remains. But if you can build it up, without relying on external
    downloads, add it to handoff"); a body pack from an open model is only for a project that needs
    more. Claude's plain word on how far that goes: a believable stylised to semi-realistic body, yes;
    a photoreal likeness, no, because the published models get theirs from thousands of scanned people.
    The owner on that: "right now, I'm not concerned about photoreal likeness. Anyone that wants it will
    be waiting after M6 and likely for a long while." They also have "other concerns that we need to
    address at a later time", not yet said, "for much later when the engine is more mature".
    Light: vendor-neutral only, never dependent on ray-tracing hardware ("You read right"), and the
    engine is a guide: "People will have different requirements and AI agents will need to straight up
    tell them on what's possible and what's not." It reports what a machine can run; the agent says so.
    The rest of the note (the ladder of light, creases, sculpt steps, skin) is the road to a true 1.0.0.
  - **The last three open decisions, answered** (the same day):
    - *Remote agents' defaults:* agreed as Claude recommended, all three (a remote lead gets no native
      code, no window input and no module source unless its token names them; owner authority is
      never given remotely unless the operator passes a flag for it; tunnels come before OS TLS):
      "Agreed with all, for later when a team wants to work on the same game through a same engine.
      I'll trust your expertise on this." ([docs/design/CLOUD_SWARMS.md](docs/design/CLOUD_SWARMS.md)).
    - *Which game M6 ships:* Sepulcra, the engine's MVP game, with all three maps of the plan of
      2026-10-22, as the demo: "3 map set in October. As demo. I'll probably ask you to set up 20
      different maps, each very different from the initial 3 maps." Claude had recommended map 1
      alone for M6; the owner chose all three. Map 1 is built; maps 2 (the frozen wasteland) and 3
      (volcanic) are M6's (ROADMAP M6). Twenty more may follow, so a map has to be cheap to make.
    - *Sepulcra's balance:* "Please. Keep it risky." The rusher who calls every wave early keeps its
      risk ([docs/design/BALANCE.md](docs/design/BALANCE.md)).
  - **Four more answers, the same day.** Voices: babble voices and the slot for voice packs are M6's,
    whether to build the machine voice at all is decided after them (the owner agreed to that
    recommendation: "I do. If it'll make it easier"). What to cut if not all of it
    fits: nothing ("All of them. No exceptions. Take your time"); Claude's order is for building: the
    memory helpers and the conditions first, then the storyline tree with captions, then reputation
    and the RPG set-up, then music. Editing: "these are editable through tools and editor. No specific
    editing feature." And the crates and the announcement do not wait for Linux: "Linux check can come
    later. Windows/Mac matter here. Linux will be the next target." They go with the first release,
    which says plainly that Linux is not verified yet; the adversarial review still comes before any
    marketing.
- **Owner, 2026-11-15 (on the Python in `scripts/`): the tooling becomes Rust in M6.** "For now, this is
  acceptable trade off but in M6, I'll want all of this in Rust." The gate, the audit's runner and every
  check are Python today (83 scripts, standard library only); nothing a player or a game's developer runs
  needs it. The plan, its order and how a port is proved are in ROADMAP M6 ("The repository's own tooling
  in Rust"): the gate and the runner first, each check when it is next touched or needed on macOS or
  Linux, all of it before M6 ends. (Changed at the scope talk, 2026-11-16, top of this page: no new
  Python at all, and the only language here is Rust.)
- **Owner, 2026-11-15: who does M6, and what comes after it.**
  - **M6 is likely to be run by a fresh instance** with none of M5's conversation. What it can rely on is
    what is in this repository: this page, AGENTS.md, ROADMAP and the design docs. Claude's memory notes
    and the lead's notes (`.swarmnotes/`) are on the development laptop only; the owner's Mac and Linux
    machines have neither.
  - **The owner is setting up their macOS and Linux machines** for the engine's work there, with another
    assistant. Expect to need on each: git and a fresh clone, Rust by `rustup` at the current stable (the
    gate refuses an older one than CI's), Python 3 until the tooling is Rust, the GitHub CLI signed in; on
    macOS the Xcode tools (whether full Xcode is needed for Metal's shader compiler is to be confirmed
    when the Metal work is sized); on Arch the GPU's Vulkan driver, `vulkan-icd-loader`, the validation
    layers and `shaderc`, and a note of whether it runs Wayland, X11 or both. Rule 12 first: loopback
    only on every machine, a tunnel between them, never an open port.
  - **After M6: a large game, the owner's first commercial one**, in a private repository of its own with
    the engine as a dependency. Sepulcra stays the free, much smaller MVP. The measured baseline says
    the engine does not yet pay at small and medium sizes; whether it pays on a large game is not
    measured. Two M6 items decide much of it and belong early in the scope talk: a project gets its SDK
    from the engine, and a module-built game reaches interface screens, strings and saved settings.
  - **M6's two focus items, in the owner's words** (2026-11-15 evening, shown the baseline again: about
    5 times the tokens on a small game, 3.1 on a medium one, 1.7 on a change): "that's going to have to
    be one of the focus items. On top of making tests and CIs even more efficient to save some time."
    The scope talk starts from these two; it still decides the rest. They meet in one place: a faster
    build-and-check loop for a game project lowers what an agent's first hour costs and what each check
    of it costs. **When M6 starts:** "likely" once the owner's seven agent tools have been updated and
    verified by a separate instance; another assistant (Grok) is also running its own tests on this
    repository first. Whatever any agent reports is checked against the code before it is acted on.
    Both are done (2026-11-16): that review's findings are fixed (below), and the owner says "we're
    ready and the tools are also updated". What is left before M6 is the scope talk. The live test of
    the seven tools (docs/agent-tools) was written before that update: read each tool's own
    instructions before leaning on that page.
  - **Two things the owner wanted talked through before M6 started** (both decided on 2026-11-16 as
    Claude leaned, top of this page; raised while M5's last commit took four pushes to go green on
    CI). As they stood before the talk:
    - **The Rust version.** "Never thought that even a basic update can cause much changes and
      requirements to shift. This is now something I need to discuss with you before we work on M6."
      CI installs the newest stable on every run and nothing pins one; the engine runs Cargo and reads its
      output folders, so a change in Cargo is a change in the engine. Claude's lean, not decided: pin a
      version in the repository and CI, move to a newer one on purpose through the gate, and keep a
      scheduled job on the newest that warns without blocking. Part of the same talk: `Cargo.toml`
      declares Rust 1.85 as the oldest that works and nothing checks it (CI builds with the newest
      only). An outside review built the workspace on 1.85.1 and its test targets did not compile
      (two test lines, since changed); whether 1.85 stays the declared minimum, and what holds it,
      is to be decided.
    - **How long verifying takes.** "Someone will be very concerned when they're pressed on project
      time. I'm already suspecting we got too many test items before we can ship games properly." Measured
      that day: the tests themselves run in about 2 minutes; CI compiles from nothing on every run (10
      minutes on Linux, 17 to 25 on Windows); the gate is about 25 minutes for each Rust version; the
      full audit about an hour; and each fix, however small, took a whole round. Claude's lean, not
      decided: keep CI's build between runs, size the gate to what changed (docs, tests, engine), gate on
      one Rust version, bring a Linux machine into the local loop, and hold to this: changing the engine
      needs all of it, making a game with the engine must not.
  - **An outside review before M6** (Grok, 2026-11-15, of `36c127a`, on a Linux machine with Rust 1.85 and
    then 1.100; its report was checked point by point against the code before anything was changed).
    Its run of the whole suite on Linux found two failures that CI's Linux image cannot show, both since
    fixed at their cause: a model-reload test that failed in one run of two (the model library compared
    file times alone, and two writes inside one tick of the clock carry one time: see CHANGELOG), and
    the Japanese interface test (that machine's first CJK face has no Latin, and an interface slot's
    face is the one face for all its text). Its documentation findings were right and are corrected:
    the release order in ROADMAP, stale status lines, the declared Rust 1.85, the two style files
    (below, "Gotchas"), ignored tests without a reason. What it could not show and nobody has: a
    macOS run; a Linux run with a GPU.
  - **`docs/M5_CHECKLIST.md` is filled in** (2026-11-15, at the owner's request): 8 of its 15 items are
    verified by checks and ticked; 7 have a part left that only the owner can do, each said there. The
    short demo recording of the agent tools is no longer wanted.

- **Owner, 2026-11-14 evening (on remote, on the comparison's first numbers):**
  - **M5 is foundations; M6 is where it is made to pay.** "That's where we'll be looking at scalability in
    M6. Right now, what matters is we're building in foundations and reliability, as well as saving the
    assets, similar to Unity and Unreal Engine and also having 2D/3D assets in M5. In M6, we'll need to work
    things out that they actually make sense for small games, medium games and large games and it should
    make it easier for you to actually make one."
  - **M6** "is finalising and making it well tuned as best as we can for initial phase": "not just entire
    tuning phase but also testing and adversarial check phase. UX experience will matter but not just for
    humans, it's also for agents." Its scope is talked through when it starts, not before.
  - **The numbers are wanted as they are:** "It'll be good to have those numbers in M5 so we both know how
    to tune to improve against them in M6, especially in Windows, MacOS and Linux." And on being told
    plainly that the engine cost more: "I need that clear eyed honesty to really achieve my goal on this
    project."
  - **The first release is Windows and macOS** ("Linux is already at a later plan. But I'll release both
    windows and MacOS. If I have someone willing to help me in Linux space, that will make it much easier"),
    hoped for before December. This replaces the order of 2026-10-22 below (Windows through M6, then macOS
    and Linux). macOS is native work that does not exist yet (a Metal renderer, its window, input and audio,
    module loading): Claude sizes it at the start of M6. The owner expects to run several Claude instances
    at once across their systems.

- **Owner, 2026-11-14 (on remote, during the performance pass):**
  - **The measured comparison goes ahead at Claude's recommendation:** two runs a cell, about 15M tokens in
    all (small game 600k a run, medium 2.0M, change round 500k, four runs each; judging about 1.5M; pilot
    runs about 1M), Sonnet 5.5 for the builders and Opus for the judge. No further go-ahead is needed.
  - **Its sizes are variables:** "gonna have to be a variable as we don't know what other people's
    requirements and their system." Runs a cell, the token cap a job, the time limit and the model are data
    in the brief file, the figures above are only its defaults, and every report states the settings used.
  - **The agent sets the variables for the machine it is on.** "You know this laptop's system and
    requirements by now and have a benchmark. But for other people's system, I'm gonna trust you, when you're
    in their system, building a game, to work out the best variables here. This engine is still agent first
    and native. But unlike you, on its own, it's not gonna know what you're seeing in theirs." So: nothing
    measured on this laptop is baked in as a default for everyone; budgets, caps, quality and frame settings
    are data with safe defaults; and the engine's part is to report what it measured where it runs
    (`engine.started`, `cpu_ms`, `lock_ms`, `perf_frame`), so the agent there can choose. Whether an agent
    arriving cold on another system finds those and chooses well is tested in M6, on the owner's other
    machines (ROADMAP M6).

- **Owner, 2026-11-12: what the engine is for, and how M5 finishes.**
  - **The engine is leverage, not a gate.** Agents can already build games without an engine; Specula exists so they
    spend fewer tokens for a better result ("making efficient use of your tokens, and reducing token waste while
    maximising the results"). A shipped game uses the runtime but never needs the agent tools or the editor (M6
    `specula export`).
  - **Who works through it:** one agent or a swarm. Humans ask (highlight and ask), edit directly in the editor, or
    choose (galleries, the review screen).
  - **Mods and extensions are M6, and live on the other side.** People's own tooling, mods and external engine
    pieces plug in from their system and never enter this repository; Specula stays zero-dependency and validates
    what plugs in. Whether a game is moddable is its human developers' choice.
  - **One builder.** From this date M5 is finished by a single primary agent, lane by lane, with subagents only for
    independent review (the parallel lanes hit the usage limit twice, conflicted in shared files and made
    performance numbers noisy). The review rule stands: code is reviewed by someone who didn't write it.
  - **No fallback lead.** "If people or AI agents start with no --lead, then it shouldn't have lead powers." Lead
    authority comes only from `--lead NAME`. Because owners are often away for hours while agents work, a run
    without a lead must still work and must say so early: saying lead without a grant is refused at the first
    hello with the fix in the message, and nothing ordinary needs a lead
    ([docs/design/AUTHORITY.md](docs/design/AUTHORITY.md)). **Launch every unattended run with `--lead NAME`.**
  - **Assets from people's own work** (an idea for after M5): convert a person's 3D model, 2D model or drawing
    into a code-based asset that they can adjust, or ask agents to adjust, and save. It is in ROADMAP's long-term
    list with what is and isn't feasible.
  - **The engine's leverage is measured in M5** ("find a way to add in measured comparison please, for M5"). The
    same game jobs are built through the engine and without it, by the same model on the same budget, and checked
    against one list by someone who built neither; then an agent with no memory changes each game. Tokens, calls,
    time and checks passed are reported for every run, failures included (ROADMAP M5, "Measured leverage"). The
    owner's aim: "easily used by AI agents no matter the company", so that small teams can compete with large
    studios; the briefs and checks are plain data any vendor's agent can run. **Tool kits by job** come first
    (ROADMAP swarm track), so the measurement runs with them.
  - **Scope and bar.** The engine is for medium to very large games. It stays fully open source and free. The bar
    is "something you'd easily recommend to other users and not hesitate to use".
  - **The display changes under you.** The owner works on a 49" monitor and switches it off to sleep, which moves
    every window to a laptop screen at another scale. Checks that depend on a display pin their own
    (`ui_act {display}`), and scripts speak to windows in physical pixels (`scripts/specula_mcp.py`).

- **How M3 to M5 began (history: the top of this page is where things stand):** M0, M1, M2 and **M3 are done** (M3 on 2026-10-22, after an integration audit; `python
  scripts/m3_exit.py --adapter integrated` re-checks the iGPU budget with a release `sepulcra-app`; M2's check is
  `scripts/m2_exit.py`). The first game, *Sepulcra* (tower defence), is playable and is the engine's dogfood.
  **M4 is done (2026-10-30):** the owner approved the exit proof's 33 props at the window (`scripts/m4_exit.py`), and
  a final integration audit passed before the push (2026-11-05, CI green). **M5 started 2026-11-06.** The owner's
  answers at its start: Sepulcra's props get a **per-prop `blocks_build` flag** as data (big pieces block building,
  small dressing doesn't); Claude's M4 style proposals are **confirmed** (the building, ruin, terrain, set and `prop`
  categories, fire colour on candles and lanterns, the `wood` preset, the `*_t3` budget); and **every M5 lane uses
  the owner's seven agent tools** (AGENTS.md "Agent tools"; the live test is in
  [docs/agent-tools/LIVE_TEST_2026-11-05.md](docs/agent-tools/LIVE_TEST_2026-11-05.md)), reporting what helped and
  what got in the way.
- **Started early:** the owner had planned to restart on 2026-10-21, but upgraded their plan and gave the go-ahead
  the night of 2026-10-17. The pre-M3 work is pushed (`d634afe`, CI green).
- **Owner answers, 2026-10-17** (diary):
  - **Window frame floor under a swarm: make it a setting**, chosen by the controller (lead) AI before it deploys
    a swarm, from how strong the human operator's machine is. Low- and mid-spec laptops can't spare frames (a mid-spec
    one might for 2D, depending on its hardware); high-spec laptops, desktops and cloud hardware can. Today it's the
    constant `BUSY_FRAME` (60 fps) in `crates/specula/src/schedule.rs`; the ROADMAP swarm track has the item.
  - **The stronger cyan selection mark: yes**, keep it (STYLE.md's highlight rule stands).
- **M3 and M4 approved (owner, 2026-10-17 late: "I'd like you to finish M3 and M4 please").** Work runs in waves of
  lanes: wave 1 (Lex, physics P1-P3, animation A3 and agent-authored motion, swarm communication, module host and
  SDK); wave 2 (Locus and navigation/AI, Sepulcra in Lex with save games and the balance lab, P4 and A4 ragdolls,
  playtest swarms and tool ergonomics, streaming and HLOD, then the M3 exit proof and agent evals); wave 3 (M4: stage
  world, contact sheets and taste checks, galleries and preferences, review queue, procedural modelling tiers 2-4,
  baked GI, VFX, terrain, then the M4 exit proof). Check `git log` and ROADMAP ticks for progress.
  **Owner, 2026-10-18: shaders get a language of our own,** compiled to SPIR-V, MSL and later the console APIs, built
  first on the Platform track (ROADMAP), so agents write a shader once for every platform.
  **Owner, 2026-10-22, the release plan (history: replaced on 2026-11-14 by Windows and macOS first):** Windows first, all the way through a Windows-focused M6, then the
  announcement. After that, macOS and Linux, tested on the owner's Windows, macOS and Arch Linux machines with
  Sepulcra and the Steam MVP. Those ports are **native** (Metal, and the Linux window systems directly), never
  translation layers such as MoltenVK or Wine/Proton, because those add overhead and dependencies. "Many people are
  on Windows OS anyway."
  **Decided by the owner on 2026-10-22** (Claude's recommendations, answered by the owner; full notes in the owner's diary):
  - **Steamworks: yes, as an optional extra.** `steam_api64.dll` is loaded at runtime, and games run and build
    without it. Docs must tell human devs and agents this plainly.
  - **Remote and cloud agents: local only until M6.** Lay the foundation notes for cloud swarms at the end of M5.
    In M6, build per-agent auth tokens and test cloud swarms thoroughly (Claude, Grok Build, OpenAI and other
    vendors' swarms). Then run an adversarial review **before any marketing**.
  - **MCP 2026-07-28: wait** until Claude Code supports it, then switch in one small lane. Testing needs restarts:
    handle them with subagents, or ask the owner up front.
  - **crates.io: after M6.** First verify on macOS and Linux with small games; if it holds up, publish to
    crates.io, then market. M6 is effectively proto-v1.0. The Specula engine is **open source**.
    (Changed 2026-11-16: the crates and the announcement go with the Windows and macOS release and do
    not wait for Linux.)
  - **A private held-out eval set: yes, at M6.** Claude reminds the owner at M6, before any benchmark claims.
  - **Sepulcra v1.0:**
    - Three maps of about 10 waves, using **every face of the cube, sides and bottom included** (a "cubic
      globe").
    - Map 1 is the dark graveyard. Map 2 is the opposite, a frozen wasteland with unique mobs and frost versions of
      the towers. Map 3 is volcanic, a mix of light and fire.
    - English first; other languages only if the Steam page shows demand.
    - Staged: map 1 is the prototype on all six faces, which proves the camera and the pathing. Maps 2 and 3 reuse
      the same cube base with different paths.
    - Camera: right-click orbits the cube; it never flips upside-down. Paths stay readable across edges.
    - **Sepulcra stays free and open** (MIT/Apache, like the engine), released as a free demo on Steam to show
      what AI agents can build when humans direct them well. The owner's paid games will live in their own
      private repos, using the engine as a dependency.
  - **Security baseline:** OS-provided TLS only, never our own crypto. Operators secure their own environments
    (AGENTS.md rule 12).
  - **The realism range:** quality presets span "simple, gameplay first" to "really good on a strong PC".
    The low preset always holds budget on the iGPU.
  - **Sepulcra look:**
    - readable moonlight at night;
    - the floating slab over mist, made stylish;
    - cyan for magic, green for the dead;
    - moss may exceed the saturation rule on map 1, and real grass blades should replace flat green;
    - an iron fence;
    - fire colour is for fire only (dim or desaturated shows "can't afford");
    - open-licence (SIL OFL) fonts.
  - **Sepulcra rules:**
    - walls on top-face lanes only;
    - a boss leak is survivable but costly;
    - keep the 3 m edge reach and 8 m per wall;
    - rune pillars don't slow wraiths;
    - the early-call bonus is real but small.
  - **Controls:** gamepad pause, title screen, saved settings and rebinding, in M5/M6. Each game chooses keyboard
    and mouse, controller, or both; agents recommend both.
  - **The five design docs are reviewed,** delegated to Claude. The owner's notes on them:
    - **ARM:** test determinism on their M2 MacBook.
    - **Physics:** aim as high as possible later.
    - **Animation:** "good enough" at M6, better after v1.0.
    - **Procedural modelling:** lay the foundations now for realistic, organic content (Unreal-class quality, but
      more efficient) after v1.0, starting stylised.
    - **UI:** stays zero-dependency, screen reader included.
  **Owner, 2026-10-22: M4 approved** ("Finish M3 and M4 now fully, and with full effort please"): finish M3, then run
  wave 3 (M4) straight after, with the same review rounds.
  **Merged so far (2026-10-18):** all of wave 1 (swarm communication, the module host, physics P1-P3, Lex,
  animation A3), and from wave 2 the playtests and tool ergonomics (the 20+ replay exit proof passes), the visual
  history gallery, save games with the balance lab, streaming with HLOD, Sepulcra fully in Lex with live Lex, Locus with navigation and AI, and (2026-10-22) determinism hardening
  (module builds journaled, the FP-environment guard, determinism lints), the streaming upload and pump fixes, and
  agent evals ([docs/design/EVALS.md](docs/design/EVALS.md), `scripts/evals.py`), project data through the
  protocol (`project_*`), and P4 joints with A4 ragdolls. All three waves are merged; M3 and M4 are done.
- **M3 plan:** Its plan (owner direction 2026-10-17: games
  built through data and tools, not hand-written code; taste engineered in; agent-native) is in ROADMAP's M3/M4,
  from [docs/research/AGENT_GAMEMAKING.md](docs/research/AGENT_GAMEMAKING.md): Lex (gameplay as data) first, then
  Locus (placement by relations), animation A3-A4 with agent-authored motion and motion checks, physics P1-P4,
  balance lab, agent evals, save games, module host, desync detector. The owner said to commit and **push before
  starting M3**.
- **Pre-M3 cleanup done (2026-10-17):** an outside review (Grok) listed gaps and the owner approved fixing them
  before M3. All merged, each reviewed:
  - current docs; a CI guard against session links in commit messages; a pinned MCP revision test; a Markdown link
    check and one owner-decisions list (`crates/specula/tests/docs_links.rs`); Linux clippy in CI.
  - with `include_ui`, opaque HUD panels (alpha >= 0.9) hide what's under them in sight packets
    (`id_buffer.ui_occluders`; textured interface quads never hide anything).
  - the sky eases into the horizon (no crisp line).
  - the near-white slab under a selected brazier was **the selection mark itself** (pale `RUNE_PALE` times a glow,
    washed out by ACES, bloom and the grade), not the fire light: highlights now use saturated `RUNE_GLOW`
    (`looks::highlight`), and the range-ring dots are cyan too.
  - agent calls are served **between presents** (`crates/specula/src/schedule.rs`): one agent with the window
    open went from about 110-230 to 500-1,270 calls/s; under a 64-agent swarm the window keeps 60 fps (frames used
    to stall for 2-3 s) at about 20% less total throughput. `perf_frame.serve` shows the schedule. Numbers in
    `perf/drills/`.
  - Deferred: CI rendering checks (need Vulkan loading on Linux first; Platform track), per-agent auth tokens
    (needed before any non-localhost bind). Outlier-aware auto-framing was done at the end of M4.
- **Owner answers, 2026-11-13:**
  - **A screen an agent puts on the window starts with its focus on Close: yes.** "Many human devs will likely
    be present but many will also be away, thus relying on you to be more autonomous." The review screen and
    the gallery start on Close when an agent opens them (or pages the review screen); the human's own F9 / F8
    still open ready to decide ([docs/design/REVIEW.md](docs/design/REVIEW.md), "Where its focus starts").
    When a lead shows you work, approving takes a click or a move to Approve.
  - **Reviewers may open screens on the window: yes**, "on my window, or on human dev's windows". A granted
    reviewer's `review_screen` and `gallery_open` stay as built: allowed, and seen by the input monitor as an
    alert, like a lead's ([docs/design/SECURITY.md](docs/design/SECURITY.md)).
  - **The raw screen recordings of 2026-11-12** (they show the owner's own windows): not deleted yet; the
    owner decides in M6. Don't share, move or delete them.
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
- **Tools** (each one's schema is in `tools/list`):
  - World: `world_spawn`, `world_set`, `world_delete`, `world_list`, `world_query` (paged filters), `world_batch`
    (atomic), `world_env`, `world_save` / `world_load` (text world files), `world_hash` (per-tick simulation hash).
  - Swarm: `agent_hello` (identity and role), `claim_take` / `claim_renew` / `claim_share` / `claim_release` /
    `claim_list` (leases on subtrees and regions), `world_changes` (change feed), `world_history` and `world_undo`
    (the agent-attributed journal), per-entity versions with `expect_version`.
  - Sight: `sight_frame` (PNG plus a packet: exact screen boxes, `visible_px`, `occluded` from the ID buffer,
    overlays), `sight_orbit` (turntable), `sight_compare` (before/after), `sight_query`, named persistent cameras
    (`camera_set` / `camera_list` / `camera_delete`), `window_view` (point the human's window).
  - Assets: `model_build` / `model_reload` (procedural recipes and glTF/GLB), `model_paint` (vertex colours as
    strokes in the recipe), `model_path` (a sweep part's path), `model_export` (a recipe written as a `.glb` for
    other engines: [docs/design/EXPORT.md](docs/design/EXPORT.md)), 2D tilemaps (`tilemap_get` / `tilemap_set`).
  - Coordination: `task_*` (the task board), `intent_*`, `note_*`, `world_rollup`, `lease_*` (ports, GPU time,
    scratch folders), `swarm_settings` (the frame floor; lead only), `desync_trace` / `desync_compare`.
  - Gameplay as data (Lex): `lex_schema`, `lex_list`, `lex_get`, `lex_edit`, `lex_validate`, `lex_simulate`,
    `lex_import`, `lex_export`, `lex_session` ([docs/design/LEX.md](docs/design/LEX.md)); `lex_live` runs a set live in the
    engine. Sepulcra plays from its Lex rules with `-- --rules lex`.
  - Placement and navigation: `world_place` (relations), `nav_build`, `nav_path`, `nav_point`, `nav_agent`,
    `nav_info`, `ai_check`, `ai_set`, `ai_status`, `model_collider`.
  - Animation: `anim_get`, `anim_set` (clips and graphs as data), `anim_state`, `anim_check` (motion checks),
    `anim_filmstrip` (contact sheets with checks); graph parameters via `world_set {anim: {graph, params}}`.
  - Joints and ragdolls: `physics_joint`, `physics_joints`, `ragdoll_set`, `ragdoll_info`; the `props.ragdoll`
    component; `--physics-threads N` (an opt-in deterministic parallel solver).
  - Physics: `physics_raycast`, `physics_cast`, `physics_overlap`, `physics_contacts`, `physics_events`,
    `physics_info`, `physics_explain`, `physics_check`, `physics_impulse`; `collider` and `body` components;
    `world_env {physics}` (gravity, substeps, sleep, 2D mode).
  - Streaming: `stream_state`, `stream_settings` (lead only), `stream_fly`; distant cells draw as merged HLOD proxies.
  - Saves and balance: `save_write`, `save_read`, `save_list`, `save_delete` (`--saves DIR`); `balance_targets`,
    `balance_sweep`, `balance_tune`.
  - Playtests: `playtest_run` (a spec as data: steps, expectations, replays) and `playtest_report`.
  - Project files: `project_list`, `project_read`, `project_write`, `project_patch`, `project_delete`,
    `project_mount` (recipes, UI, sounds, styles and module sources with claims, versions, journal and undo; outside
    edits come in as `disk`; [docs/design/PROJECT_DATA.md](docs/design/PROJECT_DATA.md)).
  - Asset studio: `studio_stage`, `studio_sheet`, `studio_lineup`, `studio_compare`, `studio_check`, `style_check`
    ([docs/design/ASSET_STUDIO.md](docs/design/ASSET_STUDIO.md)).
  - Procedural modelling: `procgen_texture`, `procgen_terrain`, `procgen_scatter`
    ([docs/design/PROCEDURAL_MODELLING.md](docs/design/PROCEDURAL_MODELLING.md)).
  - Review and direction: `review_submit`, `review_list`, `review_decide`, `review_screen`, `critique_add`,
    `critique_list`, `critique_update`, `merge_suggest`, `task_plan`, `task_progress`
    ([docs/design/REVIEW.md](docs/design/REVIEW.md)); the window's review screen opens with F9.
  - Effects: `vfx_get`, `vfx_set`, `vfx_preview`, `vfx_check`, `vfx_info`; the `vfx` and `decal` components; `--vfx`
    ([docs/design/VFX.md](docs/design/VFX.md)).
  - Galleries and taste: `gallery_create`, `gallery_get`, `gallery_list`, `gallery_open`, `gallery_close`,
    `gallery_wait`, `gallery_decide` (owner), `gallery_round`, `gallery_apply`, `gallery_delete`, `taste_query`,
    `taste_suggest`, `board_measure`, `board_check`, `style_onboard`; the window's gallery screen opens with F8
    ([docs/design/GALLERIES.md](docs/design/GALLERIES.md)).
  - GI, terrain and water: `gi_bake`, `gi_info`, `gi_check`, `gi_settings`; `terrain_info`, `terrain_edit`; the
    `water` component ([docs/design/GI.md](docs/design/GI.md), [docs/design/TERRAIN.md](docs/design/TERRAIN.md)).
  - Disk and binary files: `disk_status`, `disk_prune`, `project_upload`
    ([docs/design/DISK.md](docs/design/DISK.md)).
  - Your tool list: `tools_catalog`, `tools_activate`, `tools_deactivate`, `tools_invoke`, `tools_request`,
    `tools_scope` (lead) ([docs/design/TOOLS.md](docs/design/TOOLS.md)).
  - Tracing and the debugger: `trace_query`, `trace_follow`, `trace_call`, `trace_settings`, `trace_crash`,
    `trace_incidents`, `trace_incident` ([docs/design/TRACE.md](docs/design/TRACE.md),
    [docs/design/DEBUGGER.md](docs/design/DEBUGGER.md)).
  - Game logic: `logic_build` (diagnostics as data), `logic_reload`, `logic_unload`, `logic_status`, `logic_input`,
    `logic_hook` (hot-reloaded Rust modules; see [docs/design/MODULES.md](docs/design/MODULES.md)).
  - Sound: `audio_preview` (a sound or a piece rendered and measured without playing it), `audio_played`,
    `audio_levels`, `audio_settings` ([docs/design/AUDIO.md](docs/design/AUDIO.md)).
  - A game's interface: `ui_tree`, `ui_frame`, `ui_act`, `ui_check` (a hosted game's screens: read, seen,
    driven and checked at any display); `text_edit` (in-world text).
  - Languages: `l10n_get`, `l10n_set`, `l10n_check`, `l10n_tasks`
    ([docs/design/LOCALISATION.md](docs/design/LOCALISATION.md)).
  - Timelines: `timeline_get`, `timeline_set`, `timeline_validate`, `timeline_play` / `pause` / `scrub` / `stop`,
    `timeline_preview`, `timeline_check` ([docs/design/TIMELINE.md](docs/design/TIMELINE.md)).
  - The editor and the human's input: `editor_state`, `editor_input`; `security_incidents`, `security_clear`,
    `security_allow_lead` (the monitor over every way an agent could reach the human's keyboard, window or
    native code: [docs/design/SECURITY.md](docs/design/SECURITY.md)).
  - Performance and time: `perf_frame` (GPU ms per pass, render graph, draws, memory, pacing, budget errors from
    `perf.toml`), `perf_light` (what each light feature of a view costs on this machine, against a budget:
    [docs/design/LIGHT.md](docs/design/LIGHT.md)), `perf_capture`, `perf_zones`, `perf_memory`, `time_control`
    (pause, step, scale), `engine_info`.
  - A project's own game (a `game.json` and modules, no executable of its own: [docs/design/FIRST_HOUR.md](docs/design/FIRST_HOUR.md)):
    `game_state` (the phase, the outcome, the score, the screens, the settings, what is wrong), `game_act`
    (a button's word pressed by an agent: start, pause, a setting, the game's own actions); `memory_note`,
    `memory_get`, `memory_list`, `memory_why` (what the project remembers; `agent_hello` answers with the brief).
  - A hosted game adds `play_*` tools (Sepulcra: `play_new`, `play_command`, `play_control`, `play_state`,
    `play_map`, `play_ui`, `play_record`, `play_replay`).
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
  `crates/specula/tests/private_paths.rs` holds part of this; read a commit before it is pushed. No
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
`crates/specula/tests/docs_links.rs` checks it isn't duplicated). Remove an item once it's decided and record the
answer where it applies. What was decided is recorded by date above (2026-10-22, and 2026-11-12 to 16).
Not decisions of this kind, and also with the owner: the seven items of
[docs/M5_CHECKLIST.md](docs/M5_CHECKLIST.md) that keep a part for them (none has failed), the raw
recordings of 2026-11-12 (kept, for the release discussion), an Apple Developer account (theirs to talk
through with the instance on the Mac), and a Steamworks app ID (later).

None at present. (The reference pictures' place was decided on 2026-11-17: a public repository of
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
