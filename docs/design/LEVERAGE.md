# Design: measured leverage

Status: built 2026-11-14 (M5); run again 2026-11-16 after M6 part 3 (the second Results section: the
engine arm did not get cheaper). Data: `evals/leverage/` (the settings and the three jobs). Harness:
`examen leverage` (`tools/examen/src/leverage.rs`; it was `scripts/leverage.py` through M5, and the port was held against it on M5's runs: the same report, the same analysis of every builder's steps, the same export). Results: `perf/leverage/`. Roadmap item: "Measured leverage" in
[ROADMAP.md](../ROADMAP.md). Siblings: [EVALS.md](EVALS.md) (tasks the engine verifies from its own data),
[LAUNCH.md](LAUNCH.md) (the command line the engine arm uses).

## Why

The engine is leverage, not a gate (owner, 2026-11-12): agents can already build games without an engine,
and Specula exists so they spend fewer tokens for a better result. That is a claim about a difference, so
it is measured as one: **the same game jobs are built through the engine and without it, by the same model
on the same budget, and checked against one list by someone who built neither.** Then an agent with no
memory of the build changes each game, because most of a game's cost is changing it.

Every run is reported, failed ones included. A result that says the engine did not help on a job is a
result, and it says where to work next.

## What is compared

- **Two arms.** `engine`: the builder has a headless Specula engine serving its work folder as a project,
  the engine's documents, and the SDK for a game module. `plain`: the builder has what the machine already
  has and needs no download (the Rust toolchain with its standard library, Python 3 with its standard
  library) and may not use or read Specula.
- **Three jobs** (`evals/leverage/jobs/`), each one brief for both arms:
  - `small`, "Catch": one screen, a paddle, falling pieces, a score, three misses, a restart.
  - `medium`, "Hold the gate": a title menu, settings kept between runs, two languages from tables, five
    waves, win and lose screens, a pause.
  - `change`: a fresh builder is given a finished `medium` game and nothing else, and adds a sixth wave
    with a new kind of walker, a third language and a difficulty setting. Each `medium` game is changed
    by a builder of the arm that built it.
  - `large` (written 2026-11-17, not measured yet): "Hold the gate: the long night", about three times
    the medium game: three kinds of enemy, eight waves from a table of data, a shop between waves, a run
    saved and continued, a best score kept, three languages, a difficulty.
- **One acceptance list a job** (in its file). Every item is something the game does, phrased so that it
  is true or false of a run: "holding left for 200 ticks leaves the paddle's left edge at 0".
- **Checkable without hands.** Both arms must hand in a headless run that prints the game's state as JSON
  and a frame written as a PNG: nobody judges these games by playing them. Through the engine those
  exist as tools; without it the builder writes them. That cost is part of the difference, so the list
  marks those items `checkable` and the report shows them apart.

## What is measured, and by whom

| Measure | From |
|---|---|
| Tokens by kind, requests | the builder's transcript, where its harness keeps one (`record --usage`), else the harness's own figures (`record --by-kind`) |
| Tool uses, time | the builder's own harness, as it reports them when the builder ends (`record`) |
| Engine calls, and the bytes sent and answered | every call through `examen leverage call` (`costs/<run>.calls.jsonl`) |
| What was left: files, lines by kind | the work folder at `finish` (build output, the engine's caches and the SDK copy left out) |
| Checks passed, defects beyond the list | the judge's `verdict.json` (`judge`) |

- **Tokens are counted by kind.** An agent's run is a series of requests, and each re-reads the whole
  context so far. So a run's tokens are: what was **new in the context** (sent, and written to the cache
  once), what was **read again** from the cache (the context so far, at every later request: the largest
  number by far, and the cheapest a token), and what was **written out**. One total hides which of them a
  tool saves. The report gives the three, and one **weighted** figure whose weights are data
  (`token_weights` in the settings: relative prices; the defaults are input 1, cache write 1.25, cache read
  0.1, output 5).
- **A harness's single figure may not be what a run used.** The pilot found that the token count Claude
  Code reports when a subagent ends is the size of its context at its last request (83,563 reported for
  82,740 in the transcript), not the sum over its requests (629,000 tokens of all kinds for that run). The
  harness's figure is kept in each run's record beside the transcript's.
- **Every agent starts with its harness's own context** (system prompt and tool list: about 8,000 tokens
  for a process of its own, about 65,000 for a subagent of a session with the repository open, which is
  also shown the repository's instruction files). Both arms carry it alike; it is in every figure and it is
  not the game's.
- **The judge built neither and sees no costs.** It gets the run's folder: the brief, the game, and
  `JUDGE.md` (the list and the form of its answer). What a run cost is kept in `costs/`, beside the run
  folders and never inside one. The judge runs the game's own two checks with inputs it chooses, looks at
  the frames, and answers every item pass or fail with what it saw. An item it cannot decide fails.
- **The judge knows the arm**, because a game that runs on the engine shows it. It is not told which
  result anyone hopes for, and the list is the same.

## Sizes are data

Runs a cell, the token cap and time limit of each job, the two models and the ports are in
`evals/leverage/settings.json` (owner, 2026-11-14: other people's budgets and machines differ). The
figures in the file are what Specula's own runs used, not a recommendation for anyone else. Every run
keeps the settings it was prepared with, and the report states them.

- **The token cap is told, not enforced.** A harness cannot stop an agent at a token count. The brief
  states the budget; the report gives the tokens each run used and marks a run whose weighted figure went
  past its cap.
- **The time limit is enforced**: `examen leverage run` stops the builder's process, and what it started, at
  `wall_s`; whoever starts agents another way stops them and records `--ended wall`. A run stopped there
  has no totals from its agent: its tokens are marked incomplete and it is left out of its cell's mean.
  The run's engine is stopped after every builder and every judge, however they ended.

## Running it (any vendor's agent)

`examen leverage run` starts a builder or a judge as **a process of its own, in the run's folder**, from the
command line in the settings (`runner.command`: Claude Code's by default, with six tools and no MCP
servers; any agent's command line that takes a prompt and runs to its end fits). A process started there
sees its brief and nothing of the engine's repository unless it goes and reads it. Agents started another
way (another vendor's harness) are handed the brief by hand and their figures given to `record`.

(`examen` is `cargo run -p specula-examen --`, or the built `target/debug/examen`: a builder's brief names the built one.)

```
examen leverage --check                              # the data is well formed
examen leverage prepare small engine 1 --out ROOT --exe target/release/specula.exe
examen leverage prepare small plain 1 --out ROOT
examen leverage run ROOT/small-engine-1              # the builder; stopped at the time limit; then 'finish'
examen leverage run ROOT/small-engine-1 --judge      # the judge; its verdict.json is recorded
examen leverage prepare change engine 1 --out ROOT --exe ... --from ROOT/medium-engine-1
examen leverage report ROOT                          # report.md and report.json
examen leverage where RUN [RUN ...]                  # a builder's steps by kind
examen leverage trace RUN [RUN ...] [--judge]        # every request: what it read, wrote and cost
examen leverage trace RUN --said 14-20 [--cut N]     # what was done at those requests: each step and its answer
examen leverage export ROOT perf/leverage/DATE       # what the repository keeps: no games, no paths

# by hand, for an agent with no such command line:
examen leverage finish RUN; examen leverage record RUN --tokens N --tool-uses N --duration-s N --by-kind I,W,R,O
examen leverage judge RUN; examen leverage judge RUN --verdict RUN/verdict.json
```

The engine arm's builder runs its game with `examen leverage play RUN --ticks N [--inputs ...]`: the
engine's own `specula run` on the run's work folder, logged as one call (since the second pass of M6
part 3; the runs of 2026-11-14 and 2026-11-16 had no such command and wrote scripts of their own).
It never runs the engine's executable itself. For the engine's tools it starts and stops an engine with
`examen leverage engine start|stop RUN` (headless, on the run's own port, with the run's own settings folder,
so it is never listed among a person's projects) and calls it with `examen leverage call RUN TOOL '{json}'`
or `TOOL @args.json`, which is the engine's own `specula call` with the run's port and identity, logged.

## Another data folder

Every command but `call`, `play` and `check RUN` takes `--data FOLDER` (default `evals/leverage`,
which works as it always did: its briefs are the same to the byte, held by a test). A data folder is a
`settings.json` and its `jobs/`. A run keeps the folder it was prepared from, and every later command on
it (`run`, `judge`, `record`, `report`) reads that one; a `--data` that is not it is refused.

A settings file gives its arms as words (`["engine", "plain"]`: the two arms whose text is in the
harness's code, measured against `plain`) or as objects, with the arm the others are measured against:

```json
"arms": [
  {"name": "none", "kind": "engine", "exe": "base", "text": ["What you have", "- ..."]},
  {"name": "told", "kind": "engine", "exe": "library", "text": ["What you have", "- ..."]}
],
"baseline": "none",
"brief_names_no": ["Specula", "formae", "library"],
"change_round": false
```

- **`kind`** is what the harness does for the arm: `engine` (a project made by `specula new`, an
  engine the builder may start, its tools through the harness, every call logged) or `plain` (an empty
  work folder).
- **`text`** is the "What you have" lines of the brief. `{tool}` (the harness's command), `{run}` (the
  run's folder), `{work}` (its work folder) and `{docs}` (the documents of the checkout the arm's engine
  was built in: the first folder above the executable with `docs/design/` and a `Cargo.toml`) are
  filled in; `{{` and `}}` are a brace. A job's brief differs between arms only by this text.
- **`exe`** names the engine an `engine` arm runs by a key, which the command line gives a path:
  `--exe base=PATH --exe library=PATH`, so that one comparison runs arms on two builds. An arm with
  no key runs the `--exe PATH` given with none.
- **`brief_names_no`**: the words no job's brief may name (default: `Specula`, `engine`, `Python`,
  `Rust`, `cargo`, for a comparison with a plain arm); **`change_round`**: `false` when no job changes
  another's game.

The report puts every arm of a job in its table and adds a section **against the baseline**: each
cell's mean weighted tokens and requests over the baseline's, and the checks each passed. Its baseline
is the one the runs were prepared with (`report ROOT --data FOLDER` names another).

`prompt` prints what each builder would be told, and runs nothing: the process's prompt, then each
job's brief for each arm as `prepare` would write it (with `--exe`, `{docs}` is found; without, it is
said as a placeholder).

```
examen leverage check --data evals/leverage-props          # the data is well formed (or --check --data ...)
examen leverage prompt --data evals/leverage-props --job kit --out ROOT --exe base=BASE --exe library=LIB
examen leverage prepare kit none 1 --data evals/leverage-props --out ROOT --exe base=BASE --build base
examen leverage prepare kit found 1 --data evals/leverage-props --out ROOT --exe library=LIB --build library
examen leverage run ROOT/kit-none-1; examen leverage run ROOT/kit-none-1 --judge
examen leverage report ROOT
```

`evals/leverage-props/` is the shape library's comparison (`specula-formae`, stage 3b): props built by
a cold agent on an engine without the library (`none`), on the engine with it and its pointers and the
same brief (`found`), and with a brief that names it (`told`), each against `none`. `--build` names
each arm's engine in the report; without it a run records the commit of the checkout `examen` runs in,
which is not the engine's when the executable was built elsewhere. The judge's page for an `engine`
arm is the one the game jobs have (it names `play` and `check` beside `engine start` and `call`).

## What is and is not controlled

- **Controlled:** the model, the brief, the budget told, the acceptance list, the judge's model and
  instructions, the machine.
- **The engine arm reads documents; the plain arm has none to read.** Reading the engine's documents
  costs tokens, and those tokens are in the engine arm's count. That is the honest price of a tool one
  has to learn, and what tool descriptions and catalogs exist to make small.
- **The engine arm starts from what the engine gives a new project.** In M5's runs (2026-11-14) `prepare`
  copied `specula-core` and `specula-sdk` into the work folder by hand, as a Cargo workspace, and the builder
  began with nothing else. Since M6 part 3 `prepare` runs `specula new` there: the project has its SDK from
  the engine, a sample game that builds and plays, its screens and words, a check, and notes that say how it
  is built and checked. That start is the engine's, as the plain arm's toolchain is the machine's; a run's
  record says how much its work folder held before the builder began (`started_with`: files, bytes, lines).
- **Not controlled:** what the builder's harness puts in front of it besides the brief (its own system
  prompt and tool list: about 8,000 tokens for the default runner), how it counts tokens, and how many runs
  share the machine. Two builders run at
  once at most; time is the noisiest of the measures and tokens the least.
- **Small numbers.** Two runs a cell show a large difference and hide a small one. The report gives every
  run, not only means.
- **Judges disagree.** One model, given one list, judged the same behaviour differently in different runs
  (Results, below). An item's evidence is kept so that a reader can tell; a change's judge is told, item by
  item, what its game passed before the change; and a run can be judged again with the first judging kept
  (`run RUN --judge --again WHY`).
- **The engine arm's brief points at one route.** It prepares a module's workspace and names an example
  module; the engine's other ways to make a game get half a line. What a builder then uses says something
  about the brief as well as about the engine.
- **Builders and judges are not confined.** They are agents with a shell and the rights of whoever
  started them. "Write nowhere else" is asked of them, not enforced (one builder wrote scratch files to
  the system's temporary folder). Run this on a machine, and under an account, where that is acceptable
  (AGENTS.md rule 12: operators secure their own environments).
- **These jobs are small.** The engine is meant for medium to very large games, where what is reused
  (rendering, input, UI, saves, physics, checks) is most of the work. A one-screen game needs little of
  it. The jobs were sized to a budget, and the report says so beside its figures.

## Results (2026-11-14: M5's baseline)

Fourteen builds, every one judged and every one reported: [perf/leverage/2026-11-14](../../perf/leverage/2026-11-14/report.md)
(the report, and a file a run with its settings, costs, verdict and where its tokens went). Builders were
Claude Sonnet 5.5 and judges Claude Opus 5.5, each a process of its own (the harness's `run`), on one Windows
laptop, two at a time. At list prices the builds cost 8.55 USD, the first judging of all fourteen 13.20 USD,
and judging the four change games a second time 5.96 USD (27.71 USD in all).

| Job | Arm | Engine build | Runs | Weighted tokens (mean) | Lowest to highest | Build time | Checks passed |
|---|---|---|---|---|---|---|---|
| small (14 checks) | engine | first (`b0794cc2`) | 2 | 476,696 | 428,114 to 525,279 | 320 s, 330 s | 14, 14 |
| small | engine | second (`706837ff`) | 2 | 540,002 | 412,794 to 667,209 | 406 s, 275 s | 14, 14 |
| small | plain | | 2 | 96,990 | 95,994 to 97,986 | 87 s, 87 s | 14, 14 |
| medium (19 checks) | engine | second | 2 | 582,586 | 548,514 to 616,657 | 1,075 s, 375 s | 17, 19 |
| medium | plain | | 2 | 187,084 | 177,732 to 196,435 | 151 s, 164 s | 18, 19 |
| change (9 checks) | engine | second | 2 | 181,378 | 158,685 to 204,071 | 604 s, 157 s | 9, 9 |
| change | plain | | 2 | 109,716 | 108,795 to 110,637 | 92 s, 83 s | 9, 8 |

The second engine build has two changes made after the first pair of small runs (below); the medium and
change jobs ran on it.

**What it says.** At these sizes the engine is not leverage: it costs more for the same result.

- **Small: about 5 times the tokens** (5.2 over the four engine runs; 4.9 on the first build and 5.6 on
  the second, the one the other jobs ran on) and 3 to 5 times the time, for the same 14 of 14.
- **Medium: about 3.1 times the tokens**, for results that do not differ (17 and 19 against 18 and 19).
- **Change: about 1.7 times**, the narrowest gap, with 9 and 9 against 9 and 8. Two runs a cell cannot say
  whether one check is a difference, and the first judging of these four games gave another picture (below).
- **The builders used the engine as a module host and little else.** Every engine game is one Rust module
  holding all its rules, with a script that calls a hook of it. The small games drew their frame with
  `sight_frame`; the medium games called no drawing, interface, save, localisation or rules-as-data tool at
  all: their settings files, string tables and frames are the module's or the script's own. Two things
  put that out of reach. The engine arm's brief names the module route first and prepares it (a
  workspace, the SDK, an example), and gives "rules as data; screens are data" half a line. And on that
  route the engine's interface screens, strings by language and saved settings cannot be used: they reach
  a player only through a hosted game's own executable (as Sepulcra's does), and the SDK gives a module
  the world, its state, input, events and sounds (found while writing M5's exit proof: `ui_tree` on an
  engine with no hosted game answers `no_ui`). The brief's "screens are data" promised more than a
  module-built game can use. The harness's brief says so now; the runs above had the earlier line.
- **Judging costs more than building** (about 1 USD a medium game against 0.40 to build it without the
  engine), and through the engine it is slower: every check starts the engine and builds the module again,
  and one game's checks stepped the engine a tick a call (4,600 calls for its build, about 12,000 for one
  judging of its change, which took 28 minutes).

**Where the engine arm's tokens went** (the harness's `where RUN`; owner, 2026-11-14: "Good to have that data
to compare so we know why before we fix that in M6"). Weighted tokens, lowest to highest run of a cell.
The three columns of tokens are exact: each run's own counts by kind, times the weights. What a step was
(the counts of steps, and the sizes quoted under the table) is a reading of each command's text, with an
answer's or a command's size taken as its characters divided by four: good for shares, not for sums.

| Job, arm | Requests | Steps before the first code | Read again | Written out | New in the context |
|---|---|---|---|---|---|
| small, engine (4 runs) | 27 to 39 | 9 to 27 | 150,801 to 275,360 | 157,465 to 255,265 | 101,181 to 136,584 |
| small, plain | 8 | 1 | 11,456 to 11,642 | 62,710 to 64,295 | 21,828 to 22,048 |
| medium, engine | 31, 32 | 11, 13 | 193,288 to 206,000 | 233,245 to 277,565 | 121,981 to 133,093 |
| medium, plain | 8, 10 | 1 | 16,801 to 23,378 | 122,785 to 131,770 | 38,146 to 41,288 |
| change, engine | 11, 13 | 5, 6 | 31,133 to 41,882 | 69,070 to 97,620 | 58,482 to 64,568 |
| change, plain | 10, 11 | 3 | 19,764 to 21,406 | 52,805 to 56,615 | 32,616 to 36,226 |

- **Steps, and every step reads the whole context again.** An engine build is three to five times as many
  requests. What they re-read costs more than a plain build's whole run on the small job (150,801 to
  275,360 against 95,994 to 97,986) and about as much on the medium one (193,288 and 206,000 against
  177,732 and 196,435). Nine or more steps come before the first line of code: finding tools and their
  arguments (about 3,000 to 11,000 tokens of answers), reading the engine's documents, SDK and example
  (1,600 to 22,000), then building (4,800 to 11,000 of build answers).
- **More is written.** 31,000 to 51,000 tokens against 12,500 on the small job (2.5 to 4 times), 47,000 to
  56,000 against 25,000 to 26,000 on the medium one (1.8 to 2.3 times): a Rust module (390 to 480 lines for the small game) with a script and
  manifests where the plain arm writes one Python file. The engine did not take the game's rules off the
  builder; it added the glue around them. Written tokens weigh the most.
- **It is not failed builds.** No small engine run had a build of its builder's fail (6 to 13 builds a
  run); each medium run had two (of 7 and 13), the module checker refusing something (`native_code`,
  `source_path`). Each run's file has the count (`where.builds`).
- **In the change round the two arms take about as many requests** (11 and 13 against 10 and 11; 13 and
  16 steps against 12 and 12): the game's own documents say how it is built and checked, and the engine is
  not learnt again. That is where the gap is narrowest.

**The change games were judged twice.** A review of this lane found that the first judging could not be
compared across games: c08 ("nothing that passed before the change fails now") was judged without telling
the judge what had passed before, so one game was failed for a defect its parent already had and another
was passed with its parent's defect in it; and c02 was read three ways, because the brief said a run is won
"when the last runner is destroyed with the gate standing" and did not say what a run is when the last
runner goes through a gate that still stands (all four games end it as won; three judges failed that, one
passed it and listed it as a defect). The judge is now given the earlier verdict item by item, and c02
and the briefs say a run is won when the last runner is gone, destroyed or through the gate. That wording
was made after the runs: every builder had the earlier brief, the medium games were judged under it and
were not judged again, and the four change games were judged again under the new c02 and c08. First judging: engine 8 and 8, plain 7 and 8. Second: engine 9 and 9, plain 9 and 8 (a
real miss: its runners arrive every 1.0 s, not 1.5). Both are in the report. **Judges disagree**: the same
model given the same list judged the same behaviour differently, which is why every item's evidence is
kept and why a difference of one check between cells means little.

**What tripped a cold builder, and what was done.** Found by the harness's own smoke test, the pilot and the
runs, in a day; none of it had shown in months of use by an agent that already knew the engine.

| Found | State |
|---|---|
| A tool's arguments could not be read from a shell: in the first pair of small runs the builders asked `tools_catalog` for one tool under names it does not take (twelve times between them, by their transcripts), one passed a made-up argument to three tools to read the refusal, and one activated whole families for their schemas | `specula tools NAME` shows one tool; a refusal lists the arguments a tool takes. The second pair, on that build, met 2 and 0 refused calls where the first met 9 and 10. Their tokens did not fall (540,002 against 476,696, with 412,794 to 667,209 between two runs alike): what it saves is smaller than what two runs can show |
| `specula tools` with no word answered 46,000 bytes | names by family, about a tenth |
| A lead the operator granted could not act as lead from a terminal (the command's hello said no role), and a refused try froze it as an intruder, so that saying the role afterwards was refused too | `specula call --as NAME --role lead`; the refusal says what to say; a refused try by an identity that holds the operator's lead grant is an alert and freezes nothing |
| A tool's arguments had to be quoted for a shell | `specula call TOOL @FILE` |
| A project outside the engine's repository has no SDK to build a module against, and one copied anywhere but `crates/` is refused | the harness copies it; a roadmap item for M6 ("A project gets its SDK from the engine") |
| The engine ticks at 30 Hz and the briefs ask for steps of 1/60 s: every builder had to notice and step its rules twice a tick, or outside the tick | open (M6) |
| A game's own check starts the engine and builds its module on every run | open (M6) |

**The pilot** (one small game an arm, before the runs above) used subagents of a session that had the
repository open. It was the pilot that showed those are given the repository's instruction files, in both
arms, though both builders answered that they had been shown none; the runs above were started as
processes of their own for that reason (owner, 2026-11-14). Its figures (engine 786,296 weighted tokens,
plain 221,366, both 14 of 14) carry about 65,000 tokens of that session's context a request and are not
comparable with the table.

**What is kept where.** The repository keeps the report and a file a run. The games and each builder's and
judge's whole transcript hold the paths of the machine they ran on, and stay with whoever ran them.

**What these numbers are for** (owner, 2026-11-14): "That's where we'll be looking at scalability in M6.
Right now, what matters is we're building in foundations and reliability... In M6, we'll need to work
things out that they actually make sense for small games, medium games and large games and it should make
it easier for you to actually make one." "It'll be good to have those numbers in M5 so we both know how to
tune to improve against them in M6." They are the baseline. M6 runs the same jobs with the same harness, on
each system the engine ships for, and a larger job where there is more to reuse.

## Results (2026-11-16: after M6 part 3)

The same three jobs, the same harness (now `examen leverage`), the same models, two runs a cell, on the
engine as built by M6 part 3 (`2a0c5669`; [FIRST_HOUR.md](FIRST_HOUR.md)). Twelve builds, every one judged
and every one reported: [perf/leverage/2026-11-16](../../perf/leverage/2026-11-16/report.md). At list
prices the builds cost 8.32 USD and eleven of the twelve judgings 10.14 USD (the twelfth has no price:
below).

**The engine arm did not get cheaper. It got dearer.**

| Job | Arm | Runs | Weighted tokens (mean) | Lowest to highest | Build time | Checks passed | Engine against plain | In M5 |
|---|---|---|---|---|---|---|---|---|
| small (14 checks) | engine | 2 | 678,546 | 451,899 to 905,193 | 377 s, 1,251 s | 14, 13 | 6.8 times | 5.2 |
| small | plain | 2 | 99,776 | 96,839 to 102,714 | 90 s, 88 s | 14, 14 | | |
| medium (19 checks) | engine | 2 | 928,953 | 802,492 to 1,055,414 | 958 s, 855 s | 18, 19 | 4.0 times | 3.1 |
| medium | plain | 2 | 231,054 | 204,994 to 257,115 | 177 s, 220 s | 19, 19 | | |
| change (9 checks) | engine | 2 | 246,634 | 172,787 to 320,482 | 970 s, 1,196 s | 9, 9 | 2.2 times | 1.7 |
| change | plain | 2 | 114,280 | 109,265 to 119,295 | 90 s, 99 s | 9, 9 | | |

- **How sure this is.** Two runs a cell. The medium rise is outside what two runs alike have shown: both
  runs (802,492 and 1,055,414) are above both of M5's (548,514 and 616,657). The small cell is not: one run
  is inside M5's range (428,114 to 667,209) and one is far above it, past its token cap. The change cell
  is the same: one run inside M5's range, one above. The plain arm, which nothing here changed, cost 23%
  more on the medium job than in M5 (231,054 against 187,084) and the same on the other two, so compare
  the ratios and not the engine arm's figures alone. What can be said: **no cell got cheaper, and the
  medium cell got dearer.**
- **Checks passed are alike**, as in M5: one item failed in each of two engine games (a stale piece drawn
  after a restart; a resume that let the clock run on), none in a plain game. A difference of one check
  is nothing (M5's results, "Judges disagree").
- **One judging ended at its time limit.** The judge of `change-engine-2` wrote its whole verdict (9 of 9)
  three seconds before the harness stopped it at 30 minutes. The verdict is kept and counted; its tokens
  are marked incomplete and its price is not in the total. Judging a game built on the engine is still
  slow: 20 and 30 minutes for the change games, against 4 for the plain ones.

**Where the engine arm's tokens went** (`where RUN`, read as in M5's table).

| Job, arm | Requests | Steps before the first code | Read again | Written out | New in the context |
|---|---|---|---|---|---|
| small, engine | 36, 68 | 15, 10 | 183,510 to 500,778 | 170,070 to 265,000 | 98,318 to 139,415 |
| small, plain | 8, 7 | 1 | 9,560 to 11,860 | 65,005 to 67,585 | 22,274 to 23,268 |
| medium, engine | 61, 50 | 16, 12 | 404,063 to 543,635 | 257,305 to 340,945 | 141,124 to 170,834 |
| medium, plain | 9, 13 | 1 | 21,846 to 39,635 | 139,715 to 163,605 | 43,433 to 53,875 |
| change, engine | 33, 14 | 8, 5 | 41,722 to 139,471 | 78,190 to 102,360 | 52,874 to 78,651 |
| change, plain | 12, 13 | 3 | 25,301 to 29,087 | 49,445 to 53,750 | 34,519 to 36,458 |

- **More requests, not fewer.** The medium builds took 50 and 61 requests where M5's took 31 and 32, and
  what they read again doubled (404,063 to 543,635 against 193,288 to 206,000). That is the whole rise.
  Steps before the first line of code did not fall (10 to 16; M5: 9 to 27).
- **Part 3 put more of the engine within reach, and the builders spent requests reaching it.** In M5 a
  module-built game could not use the engine's screens, strings or settings, and no builder tried. Now
  all four small and medium builders wrote a game file and drew their HUD as a screen of the shell; three
  also used its strings, its settings and its outcome, and one medium builder drove its game through
  `game_act`, `game_state`, `ui_act` and `ui_tree`. Finding
  those tools and their arguments took 7 and 10 steps in the medium builds (about 8,000 and 11,000 tokens
  of answers), and reading the engine's documents and source 2 and 13. The other medium builder wrote its
  menus, string tables and settings inside its module, as M5's builders did, and was the cheaper of the
  two.
- **The rules got smaller; what is written did not.** A small game's module is 226 and 230 lines of Rust
  (M5: 390 to 480). The medium builders still wrote 51,000 and 68,000 tokens against the plain arm's
  28,000 and 33,000 (M5: 47,000 to 56,000 against 25,000 to 26,000): the driver script each game needs
  for its headless check is 255 and 534 lines by itself.
- **What a project remembers was read as files and never used as a tool.** Every engine builder read
  `notes/how.md` and `notes/brief.md` (2 to 6 times). No builder called a `memory_*` tool, in any of the
  six runs, and neither medium builder wrote a note: the two change builders were handed a project whose
  notes were the ones `specula new` wrote. Four of the six never said `agent_hello`, which is where the
  brief is given: an agent that works from a shell goes straight to the tool it wants.
- **The small builders ran their headless check as one playtest** (`playtest_run`, 14 and 40 calls). **The
  medium builders did not**: each stepped the clock and held inputs call by call from its own script
  (`time_control`, `logic_input`, `game_act`: 3,290 and 3,464 calls a build, and 6,264 and 9,074 in the
  change builds), each call a process started from the shell at about 105 ms. That is where the build
  minutes and the judges' minutes went. Why they chose it is not in their transcripts.
- **Failed builds are still not the cause:** one or two a run (of 1 to 9 build steps).
- **One run went wrong in the old way.** `small-engine-2` spent 28 of its 70 steps reading its own files
  again, chasing pieces of an earlier run that its restart did not remove, and ended past its cap with
  that defect still in the game.

**What this says about part 3.** It made things possible that were not: a project builds against the
engine's own SDK with nothing copied by hand, a module-built game has screens, words, settings, an
outcome and a score, and a game's tick is the game's. Those were M5's findings and they are closed. It did
not make the first hour cheaper, and the measure says why: the cost is requests, and a larger surface that
has to be found from a shell costs more requests, not fewer. What would lower it is fewer steps to a
running game, not more that the engine can do.

**What tripped a cold builder this time.** From the builders' own closing reports and the judges'
findings. "Shown" means the runs' records or the engine's code show it; "reported" means a builder said
it and it was not reproduced.

| Found | State |
|---|---|
| A headless check of a game with menus is written by hand, call by call (the medium and change builds, above), and a judge then pays for every call again | open. Shown (the call logs) |
| A press of Start or Resume by an agent lets the clock run in real time, so a script that steps the clock by hand gets ticks it did not ask for (the one failed medium item; two builders added a hidden action to freeze their game so its state could be read) | open. Shown: it is the shell's design that a press moves the clock unless a playtest has it |
| A setting an agent changes lasts for the session and is never saved, by design, so a builder cannot show "kept between runs" through the engine: both medium builders kept a settings file of their own in their script | open. Shown (FIRST_HOUR.md, "Settings are kept") |
| The memory tools and the brief at `agent_hello` are not on the path of an agent that works from a shell (above) | open. Shown (the call logs) |
| A game's own check still starts the engine and builds its module on every run (about 5 s for the shortest) | open, from M5 |
| Left and Right do nothing on a settings button of the shell in a window; Enter steps it | reported |
| The texts an agent is given list the top screen only, so a menu over the field leaves out the HUD under it | reported |
| With the engine's own "lost" outcome, a restart after game over came out one or two ticks off; the builder ended its runs in the module | reported |
| Quit cannot be pressed by an agent | by design (the input monitor); it cannot be checked headless |
| `"start": "hud"` in the game file stopped the engine at its start | not a defect: reproduced, and the engine's message names the two values `start` takes. The builder took the line out and so kept the engine's title screen in front of its own |
| The engine ticked at 30 Hz and the briefs ask for 1/60 s (M5) | closed: `tick_hz` in the game file; all four small and medium builders set it to 60 |
| A project had no SDK to build against (M5) | closed: `specula new` |

## Where every token went (the ledgers of the 2026-11-16 runs)

Owner, 2026-11-16, on the result above: "I need you to investigate the root cause of larger tokens than
expected... Note down every token spend. Assess. Pay attention to files and how agents are using the
engine." So every request of every run is laid out: `examen leverage trace RUN` (`--judge` for its
judge), and each run's file under [perf/leverage/2026-11-16](../../perf/leverage/2026-11-16/) has both
ledgers (`trace`, `judge_trace`): for each request the context it read, what was new in it, what it
wrote, its own cost, what it left for every later request to carry, and its steps with what each wrote
and was answered.

**What is exact.** A request's tokens read, and how far the context grew between two requests, are the
run's own records, and the ledger's total is the run's weighted figure to the token. A request's tokens
written are not in a stream (only the run's total is): each request's share is its growth less its
answers' size, scaled to the exact total. What was written and is not visible text (thinking) is the
difference.

**The arithmetic of a run.** Weighted cost is, for every request, the context it reads (new tokens at
1.25, tokens read again at 0.1) plus five times what it writes. A request reads the whole context so
far. Late in an engine build the context is 70,000 to 140,000 tokens, so a request costs 10,000 to
20,000 before it does anything. The plain medium build made 9 requests in a context that ended at
40,000; the engine builds made 50 and 61 in contexts that ended at 118,000 and 142,000. **Cost goes
with requests times context, and what a step reads early is paid for at every request after it.**

**Where the requests went** (the ledger's cost at each phase's last request, and its share):

| Phase | medium-engine-1 | medium-engine-2 | small-engine-1 |
|---|---|---|---|
| Discovery before any code: notes, the sample, tool lists, the SDK's sources, the engine's design document | requests 1 to 12: 174,811 (17%) | 1 to 9: 160,303 (20%) | 1 to 11: 113,723 (25%) |
| Writing the game, building, trying it by hand | 13 to 31: 319,538 (30%) | 10 to 20: 192,910 (24%) | 12 to 19: 115,180 (25%) |
| The headless check's own script, and debugging it | 32 to 53: 414,038 (39%) | 21 to 38: 266,379 (33%) | 20 to 33: 190,304 (42%) |
| README, waiting for its own check, stopping engines | 54 to 61: 147,028 (14%) | 39 to 50: 182,900 (23%) | 34 to 36: 32,692 (7%) |

**Root causes, largest first.**

1. **The headless check is a script the builder writes and debugs.** `headless.py` (3,165 tokens),
   `gate.py` (2,068), `catch.py` (1,809), then 6 to 10 patches of it at 12,000 to 25,000 a request, at
   the largest context of the run: a third to two fifths of every engine build. The plain arm's check
   is its game's own `main`: 2 to 4 requests. The brief allowed for it ("may be small scripts of yours
   that call the engine"), and the engine had no command that did it.
2. **Discovery, carried.** In medium-engine-1, request 4 (the sample's files: 5,361 tokens) cost 68,262
   to carry, request 9 (this repository's FIRST_HOUR.md: 5,683 tokens) 53,183, request 6 (eight tools'
   descriptions: 5,309) 52,224. Both medium builders and one small one opened the engine's design
   document for the game file's format, because `notes/how.md` did not say it.
3. **What is written and not seen.** 61% to 69% of an engine build's written tokens are not visible
   text (43,161 of 68,189; 34,580 of 51,461; 20,762 of 34,014), and 54% of a plain medium build's
   (15,027 of 27,943). At five a token. It goes with the number of requests, and with not knowing.
4. **Waiting and process care.** Sleeping and polling for its own check of 3 to 6 minutes (3 to 5
   requests at 14,000 to 17,000), stopping engines and looking for ones left behind (medium-engine-2,
   requests 46 to 49).
5. **Things looked for that were not there**: Left and Right on a settings button (medium-engine-1,
   requests 11 and 24 to 26), a camera for a 2D game's frame (28 to 31), a start screen (small-engine-1,
   9 to 11 and 28), the module's kept values (16, 20), settings kept from a headless run (both medium
   builders kept a file of their own).
6. **Build cycles**: 5 to 9 builds a run, 10,000 to 30,000 each.
7. **Judges pay the same way**: reading the game's files is 40% to 58% of a judging, and an engine
   game's judging cost 1.4 to 2 times a plain one's.

**The floor, by the same arithmetic.** About 12 to 15 requests and 25,000 written tokens for the medium
game is about 190,000 weighted: the plain arm's figure. The engine arm goes under that only where a
game through the engine is less to write than a plain one, which at this size it is not: a medium
game's rules alone (3,300 to 5,100 tokens of Rust) are as much as the plain arm's whole game (5,055 of
Python). What the engine takes off a builder is the menus, the settings, the pause, the picture and the
check; the rules stay.

**What was done about it** (M6 part 3, the second pass; [FIRST_HOUR.md](FIRST_HOUR.md), section 9): one
command that builds, plays a script of inputs and prints the game's state and a frame (`specula run`),
so that the loop is edit, run, read; the game file's `report` and `view`; Left and Right on a setting;
and a `notes/how.md` that answers what all six builders looked up. The harness gives the engine arm
that command (`examen leverage play RUN ...`, logged as one call) and its brief says so; the briefs'
game sections and both acceptance lists are unchanged.

## Results (2026-11-17: the second pass, `specula run`)

The same three jobs, harness and models, on the engine with the second pass
(`bbe9fa36`; [FIRST_HOUR.md](FIRST_HOUR.md), section 9). The engine arm's brief names the one command
that builds, plays and prints (`examen leverage play RUN ...`, the engine's own `specula run`); the
briefs' game sections and both acceptance lists are as they were. Fourteen builds, every one judged and
every one reported: [perf/leverage/2026-11-17-pass2](../../perf/leverage/2026-11-17-pass2/report.md),
each run's file with its ledgers. The engine's medium and change cells have three runs (the roadmap's
word after the last measurement: two runs alike differ by as much as the effect looked for).

**The small and the medium game got much cheaper through the engine. The change got dearer.**

| Job | Arm | Runs | Weighted tokens (mean) | Lowest to highest | Build time | Checks passed | Engine against plain | After part 3 | In M5 |
|---|---|---|---|---|---|---|---|---|---|
| small (14 checks) | engine | 2 | 169,613 | 160,854 to 178,372 | 169 s, 156 s | 14, 14 | 1.7 times | 6.8 | 5.2 |
| small | plain | 2 | 97,184 | 90,971 to 103,396 | 79 s, 88 s | 14, 14 | | | |
| medium (19 checks) | engine | 3 | 446,356 | 277,124 to 672,915 | 1,818 s, 274 s, 409 s | 18, 17, 19 | 2.6 times | 4.0 | 3.1 |
| medium | plain | 2 | 171,273 | 163,197 to 179,349 | 149 s, 146 s | 18, 19 | | | |
| change (9 checks) | engine | 3 | 392,016 | 305,298 to 445,466 | 1,161 s, 2,463 s, 602 s | 8, 8, 7 | 3.9 times | 2.2 | 1.7 |
| change | plain | 2 | 99,336 | 97,497 to 101,174 | 82 s, 73 s | 9, 9 | | | |

- **How sure this is.** The small cell: both runs (160,854 and 178,372) are less than half the cheapest
  engine run this job has had in three measurements (428,114). The medium cell: all three runs are below
  both of part 3's (802,492 and 1,055,414), and two of the three are below every earlier engine run
  (548,514 the lowest). The change cell: all three runs are above the cheaper of part 3's two (172,787)
  and two are above the dearer (320,482). The plain arm, which nothing here changed, cost 26% less on
  the medium job than a day earlier (171,273 against 231,054): compare the ratios.
- **What did it is requests.** The small builds took 16 requests each where part 3's took 36 and 68; two
  of the medium builds took 24 and 39 where part 3's took 50 and 61. No engine builder wrote a
  headless-check script (a third to two fifths of every build before), none opened the engine's design
  document for the game file's format, and the engine was called 10 times in each small build and 19 and 43 times in
  two medium ones (part 3's medium builds: 3,290 and 3,464 calls).
- **Checks passed are alike** in the small and medium cells (a walker that comes a tick late, a bolt
  that passes a walker in its lane: the same kind of slip in both arms, and one plain game has the
  second too). **In the change cell they are not:** all three engine games failed the first item, and
  one a second (8, 8 and 7 of 9 against 9 and 9). Each sent the new wave's runners faster than the
  walkers' one every 1.5 seconds (0.75 s, 1 s, 1 s; "the brief gave no rate", one wrote), where both
  plain builders kept the game's own rate; and one left its README naming the folder the game was
  copied from.
- **A judging through the engine still costs more**: 182,222 and 137,734 against 131,294 and 141,791
  on the small job; 352,114 to 458,535 against 219,879 and 291,125 on the medium one; and on the
  change, where the judge has to see a six-wave game won, 312,315 to 527,414 against 214,650 and 240,359 (twenty to twenty-seven minutes a game against four).

**Where the tokens went** (`where RUN`).

| Job, arm | Requests | Steps before the first code | Read again | Written out | New in the context |
|---|---|---|---|---|---|
| small, engine | 16, 16 | 8, 7 | 39,948 to 44,772 | 79,340 to 84,435 | 41,566 to 49,164 |
| small, plain | 7, 9 | 1 | 9,243 to 13,908 | 61,135 to 67,315 | 20,593 to 22,173 |
| medium, engine | 67, 24, 39 | 6, 5, 5 | 90,492 to 364,418 | 124,375 to 208,810 | 62,257 to 99,688 |
| medium, plain | 8, 9 | 1 | 16,367 to 17,447 | 109,880 to 124,825 | 35,870 to 38,157 |
| change, engine | 38, 45, 49 | 8, 4, 7 | 140,560 to 219,474 | 101,395 to 142,995 | 63,342 to 82,997 |
| change, plain | 9, 11 | 3 | 16,076 to 20,580 | 46,295 to 53,535 | 30,622 to 31,563 |

**What the engine builds still spent, by their ledgers** (`trace RUN`, and `trace RUN --said A-B` for what
was done at a request; the third pass is built from this list, [FIRST_HOUR.md](FIRST_HOUR.md), section 10).

1. **A module whose tick failed, and a run that printed a state all the same.** A call that returns an
   error is undone. In `medium-engine-1` the module's own query deleted the parent it spawned under, and
   every tick after failed; in `medium-engine-3` two walkers were spawned under one name. `specula run`
   printed the state of a game whose rules did nothing, with no word of it. The first builder spent
   requests 14 to 47 on it (290,027 of the build's 672,915 weighted tokens, by the ledger), the second
   requests 12 to 27 (123,869 of 389,029), each putting prints of its own
   into the module to find one line the engine had (`logic_status`'s last error). Without it the first
   build would have cost about what the third did.
2. **Showing that the game can be won needs a player that reacts**, and a script of inputs written
   beforehand does not. `medium-engine-1`'s builder wrote a small program that read the printed state
   and chose the next input, starting the run again from tick 0 for every decision: 347 runs, thirty
   minutes, requests 51 to 66 (177,191). The other two worked a script out by hand from logs they
   added (85,597 and 39,213). The judges wrote planners of their own for the same question. All
   three change builders then ran the first builder's way again (602 to 2,463 seconds a build).
3. **In the change round, a setting's value as words.** The change asks for a difficulty setting in
   three languages. `setting.NAME.text` shows a choice as the game file spells it (`hard`), and each
   builder went looking for how to show "Difficile": 10 to 25 requests in the engine's design documents
   and the interface crate's source (in `change-engine-3`, requests 5 to 30 hold it, with the changes' own code: 211,071 of 445,466),
   ending at three labels with a condition each. This, with the second point, is why the change cell
   went the other way; neither was in the games part 3's change builders were handed, whose own scripts
   did both.
4. **Looked up, because the guide did not say:** how a key is spelt in a binding (`medium-engine-1`,
   three requests, one of them a search that printed 64 MB); what the SDK's functions return (every
   build listed its `pub fn`s, two requests each); what a `"visible"` condition takes.
5. **One check a request.** After its code was written a small build ran the game in 5 or 6 requests
   and `medium-engine-2` in 15, each 6,000 to 14,000 weighted at the context it had by then. The plain builds
   checked theirs in 2 or 3.
6. **The harness's own:** two builders named their work folder where the command wanted the run's and
   spent a request on the refusal; its `tools` said nothing when no engine was running.

**Beyond the list, from the judges** (the engine's, not a game's): a run with no `--seed` is not the run
with seed 1, which the briefs call the default (four of five engine games); the title screen's focused
button has no focus mark until a key is pressed (three); the sample's title shows the game file's title,
which no string table can translate (one game failed "every word changes with the language" on it, one
builder moved it into the tables).

**One builder wrote outside its folder**: `medium-engine-3` copied its work folder beside the runs' as
a backup, said so, and could not remove it (its shell refused the removal). Nothing read it.

## Results (2026-11-17: the third pass, a run that says what went wrong and can be played by reaction)

The same three jobs, harness and models, on the engine with the third pass (`6cfff79b`;
[FIRST_HOUR.md](FIRST_HOUR.md), section 10). The engine arm's brief gained one flag in its command's
line (`--follow`); nothing else of the briefs or the acceptance lists changed. Fourteen builds, every
one judged and every one reported:
[perf/leverage/2026-11-17-pass3](../../perf/leverage/2026-11-17-pass3/report.md).

**Through the engine the medium game now costs what it costs without it, with every check passed.
The small game and the change cost half as much again, and each change build still fails one check
that the plain builds pass.**

| Job | Arm | Runs | Weighted tokens (mean) | Lowest to highest | Build time | Checks passed | Engine against plain | Second pass | After part 3 | In M5 |
|---|---|---|---|---|---|---|---|---|---|---|
| small (14 checks) | engine | 2 | 145,926 | 142,520 to 149,332 | 138 s, 206 s | 14, 14 | 1.5 times | 1.7 | 6.8 | 5.2 |
| small | plain | 2 | 95,483 | 84,688 to 106,277 | 75 s, 93 s | 14, 14 | | | | |
| medium (19 checks) | engine | 3 | 219,007 | 198,595 to 242,012 | 240 s, 272 s, 206 s | 19, 19, 19 | 1.0 times | 2.6 | 4.0 | 3.1 |
| medium | plain | 2 | 217,869 | 207,481 to 228,257 | 182 s, 179 s | 19, 19 | | | | |
| change (9 checks) | engine | 3 | 122,392 | 108,723 to 148,065 | 154 s, 204 s, 393 s | 8, 8, 8 | 1.5 times | 3.9 | 2.2 | 1.7 |
| change | plain | 2 | 81,746 | 78,710 to 84,782 | 62 s, 62 s | 9, 9 | | | | |

- **How sure this is.** Two or three runs a cell, as before, and the cells do not overlap what they
  were: the dearest medium build of this pass (242,012) is below the cheapest of the pass before
  (277,124), and the dearest change build (148,065) is under half the cheapest of the pass before
  (305,298). The three medium builds lie within 22% of each other, where the pass before spread from
  277,124 to 672,915: the two that had cost most then were the two whose module failed in silence.
  The medium cell's ratio is one run's noise from 1.0 either way (the plain arm's two runs differ by
  10%); what can be said is that the two arms now cost the same on this job, not that the engine is
  cheaper.
- **Checks passed.** Every small and medium game of both arms passed every check (14 of 14; 19 of
  19: the first measurement in which no medium game missed one). **Each engine change build failed
  the same one item**, as all three did the pass before: the new wave's runners come faster than
  the walkers' one every 1.5 seconds (0.75 s, 0.75 s, 1 s). Six engine builds of six in two passes,
  and none of four plain ones. The change brief names no rate for runners; why the builders that
  work through the engine choose another and the others keep the game's own is not in their
  transcripts (what they wrote and did not show is two thirds of what they wrote).
- **Requests.** Small: 13 and 13 (16 and 16 the pass before; the plain arm 7 and 8). Medium: 18, 19
  and 17 (67, 24, 39; plain 15 and 10). Change: 12, 11 and 18 (38, 45, 49; plain 8 and 11).
- **Through the engine a medium game is less to write.** The three engine builds wrote 19,122 to
  23,146 tokens and the two plain ones 28,045 and 28,152: the menus, the settings, the pause, the
  languages' plumbing and the picture are the engine's. What the engine arm pays for instead is
  reading (the guide and the sample: 53,025 to 66,364 read again, against 23,526 to 43,773).
- **What the third pass was built for, used.** No build met a module that failed (the guide now says
  how things that come and go are drawn, and none drew them the way that failed). Four of the five
  small and medium builds, and all three change builds, played their game by reaction through
  `--follow` (a player of 280 to 360 tokens, written in one request and run in one); the change
  builds took 2.5 to 6.5 minutes where they had taken 19 to 41. Each change build showed the new
  setting's values in three languages with the strings' own lines, in no extra request.
- **A judging through the engine still costs more, and takes longer**: 282,764 to 353,891 against
  208,765 and 235,753 on the medium job (10 to 12 minutes against 3), 147,950 and 160,583 against
  136,561 and 149,955 on the small one. On the change the two arms' judgings now cost the same
  (265,354 to 339,916 against 264,551 and 293,503), though the engine's still take 7 to 12 minutes
  against 4.

**Where the tokens went** (`where RUN`).

| Job, arm | Requests | Steps before the first code | Read again | Written out | New in the context |
|---|---|---|---|---|---|
| small, engine | 13, 13 | 7, 5 | 28,927 to 30,530 | 74,290 to 79,620 | 39,182 to 39,304 |
| small, plain | 7, 8 | 1 | 9,001 to 12,095 | 56,435 to 71,030 | 19,252 to 23,152 |
| medium, engine | 18, 19, 17 | 3, 5, 3 | 53,025 to 66,364 | 95,610 to 115,730 | 49,960 to 59,918 |
| medium, plain | 15, 10 | 1, 2 | 23,526 to 43,773 | 140,225 to 140,760 | 43,195 to 44,259 |
| change, engine | 12, 11, 18 | | 24,492 to 47,154 | 39,170 to 59,410 | 37,531 to 41,998 |
| change, plain | 8, 11 | 3 | 12,581 to 19,783 | 37,075 to 38,325 | 27,804 to 27,923 |

**What the engine builds still spent, by their ledgers** (the fourth pass is built from the first:
[FIRST_HOUR.md](FIRST_HOUR.md), section 11).

1. **One run a request.** After its code was written a medium build ran its game in 8 to 11 requests,
   each one run or two, each reading the whole context again: requests 7 to 17 of `medium-engine-1`
   (99,951 of 216,414), 8 to 18 of `medium-engine-2` (108,670 of 242,012), 7 to 14 of
   `medium-engine-3` (65,146 of 198,595). A third to nearly half of a build in which nothing went
   wrong; in the change builds, 5 to 9 requests. The plain builds checked theirs in 2 to 4: their
   checks are a second program, written once and run together.
2. **The guide and the sample, read before a line is written**: two requests and 7,500 to 8,000
   tokens that every later request carries (in `medium-engine-2`, 11,225 and 26,710 weighted to
   carry). This is the engine arm's floor on a small game: what it has to learn, the plain arm
   already knows.
3. **Still looked up:** how the rules get the run's seed (`small-engine-1`, two requests: they do
   not need it, and the guide says so only where the command is described); what turns into a
   `Value`, and what the built-in volume is (`medium-engine-2`, two requests); and one build failed
   once on a helper that read the state through `&Cx` where the SDK takes `&mut Cx`.

**A cost that is the builders' shell's, in both arms.** Every build of this pass and the one before,
through the engine and without it, wrote its first file twice: its first here-document was refused
by its shell ("unexpected EOF while looking for matching `'`": the tool's shell on Windows and an
apostrophe in the text), and the file was written again with the file tool. That is 2,200 to 2,700
tokens written and thrown away, carried by every later request, and a request to see what happened:
roughly 20,000 to 50,000 weighted a build (requests 4 and 5 of `medium-engine-1` are 51,224). It
falls on both arms alike and is in every figure above; it is said here because it is a tenth to a
quarter of what a build costs and nothing of either arm's making.

## Results (2026-11-17: the fourth pass, a game's own checks in one command)

The same three jobs, harness and models, on the engine with the fourth pass (`d18e1131`;
[FIRST_HOUR.md](FIRST_HOUR.md), section 11). The engine arm's brief gained the `check` command in
its commands' lines, and the judge's brief says a game may bring checks of its own; nothing else of
the briefs or the acceptance lists changed. Fourteen builds, every one judged and every one
reported: [perf/leverage/2026-11-17-pass4](../../perf/leverage/2026-11-17-pass4/report.md).

**The fourth pass made no build cheaper. The medium game still costs about what it costs without
the engine, the change still half as much again, and the small game costs more than it did the
pass before: 2.2 times the plain build, from 1.5.**

| Job | Arm | Runs | Weighted tokens (mean) | Lowest to highest | Build time | Checks passed | Engine against plain | Third pass | Second pass | After part 3 | In M5 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| small (14 checks) | engine | 2 | 208,860 | 196,269 to 221,450 | 286 s, 301 s | 14, 13 | 2.2 times | 1.5 | 1.7 | 6.8 | 5.2 |
| small | plain | 2 | 94,670 | 89,050 to 100,291 | 86 s, 79 s | 14, 14 | | | | | |
| medium (19 checks) | engine | 3 | 207,057 | 205,269 to 208,160 | 259 s, 265 s, 235 s | 19, 19, 19 | 1.1 times | 1.0 | 2.6 | 4.0 | 3.1 |
| medium | plain | 2 | 189,808 | 184,304 to 195,313 | 150 s, 156 s | 17, 18 | | | | | |
| change (9 checks) | engine | 3 | 145,347 | 127,741 to 176,737 | 241 s, 507 s, 388 s | 8, 9, 7 | 1.5 times | 1.5 | 3.9 | 2.2 | 1.7 |
| change | plain | 2 | 97,842 | 82,693 to 112,991 | 62 s, 87 s | 8, 9 | | | | | |

- **How sure this is.** Nothing changed for the plain arm between the third pass and this one, and
  its three cells moved by 1%, 13% and 20% (small 95,483 to 94,670; medium 217,869 to 189,808;
  change 81,746 to 97,842): that is what two runs a cell are worth. The engine arm's medium cell
  (219,007 to 207,057, 5% down) and its change cell (122,392 to 145,347, 19% up) moved by no more
  than that, and say nothing either way. **Its small cell is a rise**: 145,926 to 208,860, 43% up,
  and each of its two builds cost more than every small engine build of the two passes before
  (142,520 to 178,372).
- **The medium ratio went from 1.0 to 1.1 by the plain arm's doing**: its two builds came out
  cheaper this time, with fewer checks passed (17 and 18 of 19 against the engine's 19, 19 and
  19). By the token a passed check the two arms are level: 10,898 through the engine, 10,846
  without. The three engine builds lie within 1.5% of each other (205,269 to 208,160); the pass
  before they spread by 22%, and the one before that from 277,124 to 672,915.
- **Checks passed.** Medium: every engine game passed all nineteen; the plain games missed two and
  one (walkers every 91 ticks where the list says 1.5 seconds, and a wave a tick late; a bolt that
  passes through a walker in 7 of 120 timings). Small: one engine game missed one (its README's
  example command is refused by the engine: an input after the run's last tick); the rest passed
  all fourteen. Change: 8, 9 and 7 through the engine against 8 and 9 without. Two of the three
  engine change builds again sent runners every 0.75 seconds where the list says 1.5; the third
  kept 1.5. That is eight engine builds of nine over three passes; the brief names no rate, and
  why these builders choose one is still not in what they show of their thinking. One plain change
  build failed the same item this time, for an interval of 91 ticks (1.517 seconds) that came with
  the game it was given: judges differ by an item (M5's results say so), and a difference of one
  check between two cells is nothing.
- **Requests.** Small: 17 and 25 (13 and 13 the pass before; the plain arm 8 and 7). Medium: 15,
  17 and 16 (18, 19, 17; plain 9 and 13). Change: 15, 24 and 13 (12, 11, 18; plain 9 and 12).
- **A judging through the engine still costs more**: 282,802 to 302,126 against 256,023 and
  295,090 on the medium job (6 to 9 minutes against 4 to 5), 177,056 and 223,010 against 119,995 and
  132,325 on the small one, 308,174 to 408,920 against 248,795 and 280,561 on the change (8 to 10
  minutes against 4 to 5). Three of the eight engine judgings ran the game's own checks.

**What the builders did with `specula check`.** Every one of the eight engine builds left a
`checks/runs.json` (8 to 11 checks in the small and medium games, 12 to 20 in the changed ones)
and ran it. **None used it in place of single runs.** Each still played its game a run or two a
request first (5 and 9 runs in the small builds, 5 to 7 in the medium ones), read the states, and
then wrote its checks from what it had seen, as one more thing to hand in. The guide proposes the
other order (write the runs you want to see as checks that expect nothing, read them all in one
request, then write what each must be); no builder took it. So the check runs came on top of the
runs they were built to replace:

| Job | Requests after the code was written | Their cost (weighted) | Share of the build | The pass before |
|---|---|---|---|---|
| medium, engine | 7, 7, 8 | 75,684; 60,810; 72,656 | 37%, 29%, 35% | 11, 11, 8 requests; 46%, 45%, 33% |
| small, engine | 8, 16 | 65,268; 105,178 | 33%, 47% | 3 requests, 18% (`small-engine-1`) |

- **On the small game the checks are the rise.** Writing the checks file and running it until it
  held took requests 13 to 15 of `small-engine-1` (26,464 weighted) and 20 to 23 of
  `small-engine-2` (27,126); both builds also wrote a player that reacts (one of two did the pass
  before), and `small-engine-2` spent four requests (14,642) repairing the escapes in that player's file,
  which it had written through its shell's here-document. A one-screen game with nothing to keep
  between runs does not need what a checks file gives.
- **Where the checks were worth something.** A change build begins from a game that has them: the
  three change builds found the medium builds' 8 to 11 checks in the folder, added their own (to
  18, 12 and 20) and ran all of them after their edits; `change-engine-1` showed its three changes
  and the unbroken game in one request, 18 of 18. In `change-engine-2` the first run of them
  answered with the module's compile error as data. And the checks stay in the folder for whoever
  comes next, which no figure here counts.
- **A defect of the fourth pass, found by these runs.** `small-engine-1` wrote a range,
  `"paddle_x": {"gt": 8.49, "lt": 8.51}`, and the check said it did not hold for 8.500000000000009:
  two comparisons in one object were read as a value to equal. It cost the build two requests, and
  its builder ended by pinning the drifted number itself, which its judge noted. Fixed after the
  measurement (`e8097c2a`, and `648cb757` after a review by a model that did not write it: several
  comparisons of one key must all hold; a comparison beside a key that is none is refused when the
  file is read, where it would have held against nothing; the guide gives the range and says what
  is equal within a rounding; the live check holds a range). The figures above are of the build
  with the defect.
- **Not the engine's:** `small-engine-2`'s builder appended `rm -rf target/..` to a command, which
  is its whole working folder. Its own harness refused the command, and the builder said so and
  went on. Nothing was removed.

**Where the tokens went** (`where RUN`).

| Job, arm | Requests | Steps before the first code | Read again | Written out | New in the context |
|---|---|---|---|---|---|
| small, engine | 17, 25 | 7, 6 | 54,435 to 77,680 | 89,265 to 96,185 | 47,585 to 52,569 |
| small, plain | 8, 7 | 1 | 9,228 to 11,677 | 59,475 to 66,110 | 20,346 to 22,505 |
| medium, engine | 15, 17, 16 | 5, 5, 5 | 45,427 to 55,368 | 99,515 to 108,075 | 51,768 to 53,513 |
| medium, plain | 9, 13 | 1, 1 | 20,319 to 31,463 | 123,760 to 123,875 | 39,975 to 40,226 |
| change, engine | 15, 24, 13 | 6, 6, 12 | 34,804 to 70,979 | 45,920 to 60,015 | 41,722 to 47,017 |
| change, plain | 9, 12 | 3 | 14,885 to 24,015 | 39,345 to 56,275 | 28,463 to 32,702 |

**What the ledgers of this pass say.**

1. **A command is not taken up in place of a habit because a guide proposes it.** The builders
   explore a run at a time and write their checks afterwards, with the engine as without it (the
   plain builds do the same with their own program). What `specula check` changes is not what a
   first build costs; it is what the build leaves behind.
2. **What is left on the medium job is what a first meeting costs.** An engine build has 21,600
   to 25,300 tokens in its context when it first writes code (the brief, the guide, the sample)
   and a plain one 9,500; every later request carries the difference. The engine arm
   then writes less (99,515 to 108,075 weighted against 123,760 and 123,875: the menus, the
   settings, the languages and the picture are the engine's), and the two come out level. On a
   one-screen game there is nothing for the engine to write in the builder's place, and the reading
   is not paid back.
3. **The builders' shell, in both arms, again.** Every build wrote its first file twice (a first
   here-document refused, then the file tool): requests 2 and 3 of `medium-plain-1` are 76,768 of
   its 184,304, requests 4 and 5 of `medium-engine-1` 48,152 of 205,269. It is in every figure of
   every pass, on both sides, and is neither arm's making.

**The three passes, side by side** (the owner's order of 2026-11-16; weighted tokens of the engine
arm, and the engine against plain).

| Job | M5 (2026-11-14) | After part 3 | Second pass | Third pass | Fourth pass |
|---|---|---|---|---|---|
| small | 508,349; 5.2 times | 678,546; 6.8 | 169,613; 1.7 | 145,926; 1.5 | 208,860; 2.2 |
| medium | 582,586; 3.1 | 928,953; 4.0 | 446,356; 2.6 | 219,007; 1.0 | 207,057; 1.1 |
| change | 181,378; 1.7 | 246,634; 2.2 | 392,016; 3.9 | 122,392; 1.5 | 145,347; 1.5 |

The second pass (one command that builds, plays and prints) took the small game from 6.8 times to
1.7; the third (a run that says what failed, and a player that reacts) took the medium game from
2.6 times to level and the change from 3.9 to 1.5; the fourth moved nothing down and the small
game up. Level is where it stands on a medium game, and half as much again to twice on a small
one and on a change: the engine has stopped costing more at one size, and does not yet pay at
any of the three.

## Results (2026-11-17: the fifth pass, a subtraction, and the first measurement in a clean shell)

The same three jobs, harness and models, on the engine with the fifth pass (`9450e0f4`;
[FIRST_HOUR.md](FIRST_HOUR.md), section 12). Two things changed in the briefs, and they are why
this section is read apart from the ones before it. The engine arm's part was cut (one read as a
command; `check`, an engine left running and its tools in one line; a change builder no longer told
the game is a sample). And **both arms were told to write files with their file tool and never with
a shell here-document**: in every build of every earlier pass the first here-document was refused
by the builder's shell and the first file written twice. Fourteen builds, every one judged and
every one reported: [perf/leverage/2026-11-17-pass5](../../perf/leverage/2026-11-17-pass5/report.md).

**In a clean shell the engine costs a quarter more than the plain build on the medium game and on
the change, and nearly twice on the one-screen game. The fifth pass took back the fourth's rise on
the small game and brought the change near level; it did not move the medium game.**

| Job | Arm | Runs | Weighted tokens (mean) | Lowest to highest | Build time | Checks passed | Engine against plain |
|---|---|---|---|---|---|---|---|
| small (14 checks) | engine | 2 | 132,122 | 120,842 to 143,401 | 161 s, 158 s | 14, 14 | 1.85 times |
| small | plain | 2 | 71,322 | 63,740 to 78,903 | 60 s, 72 s | 14, 14 | |
| medium (19 checks) | engine | 3 | 196,431 | 167,638 to 223,545 | 182 s, 256 s, 288 s | 19, 19, 19 | 1.28 times |
| medium | plain | 2 | 153,572 | 142,408 to 164,735 | 134 s, 274 s | 19, 17 | |
| change (9 checks) | engine | 3 | 141,703 | 115,398 to 187,215 | 586 s, 303 s, 270 s | 8, 9, 9 | 1.25 times |
| change | plain | 2 | 113,478 | 108,170 to 118,787 | 78 s, 84 s | 9, 8 | |

**The ratios of the passes before flattered the engine, and by how much.** The refused
here-document cost the plain arm more, because its first file is the whole game. Estimated from
each build's ledger (the text written into the refused here-document, what carrying it cost, and
the request spent finding out what had happened):

| Job | Third pass, measured | Third pass, without the fault (estimate) | Fourth pass, measured | Fourth pass, without the fault (estimate) | Fifth pass, measured (no fault) |
|---|---|---|---|---|---|
| small | 1.5 | 1.7 (118,110 against 68,768) | 2.2 | 2.7 (181,800 against 68,424) | 1.85 (132,122 against 71,322) |
| medium | 1.0 | 1.1 (178,452 against 161,486) | 1.1 | 1.26 (175,042 against 138,872) | 1.28 (196,431 against 153,572) |
| change | 1.5 | 1.5 to 1.6 | 1.5 | 1.5 to 1.6 | 1.25 (141,703 against 113,478) |

"The medium game costs the same" (the third pass) was 1.1 at best, and the fifth measurement says
1.28: with two and three runs a cell, and a plain arm that moved by 13% and 20% between two passes
in which nothing changed for it, 1.1 and 1.28 are not told apart. What can be said: **on these three
jobs the engine has not been cheaper than the plain build in any measurement, and in a clean shell
it is a quarter dearer on two of them and nearly twice on the third.** The fault fell on the change
job least (a change is patches, not one large first file), so that cell's estimates are not given
to a figure.

- **Checks passed.** Every small game passed all fourteen. Medium: every engine game all nineteen;
  the plain games nineteen and seventeen (walkers every 91 ticks where the list says 1.5 seconds; a
  README command that fails in a fresh folder). Change: 8, 9 and 9 through the engine, 9 and 8
  without. The runners' rate again in one engine build (one a second), and in one plain build for
  the 91 ticks its game came with. It is now nine engine change builds of twelve over four passes.
- **Requests.** Small: 12 and 11 (the plain arm 6 and 10). Medium: 14, 16 and 17 (8 and 12).
  Change: 20, 13 and 12 (12 and 14).
- **The change is nearly level in two builds of three.** `change-engine-2` and `change-engine-3`
  cost 122,496 and 115,398, the plain builds 108,170 and 118,787; `change-engine-1` took 20
  requests and 187,215 (it ran the game's own checks, then tried its changes a run or a check a
  request for a dozen more). The pass before, the three had cost 127,741 to 176,737 against 82,693
  and 112,991.
- **A judging through the engine still costs more**: 204,223 and 206,927 against 171,236 and
  133,322 on the small job; 271,502 to 340,736 against 250,996 and 227,992 on the medium one (7 to
  7.5 minutes against 3.5 and 6); 411,246 to 490,019 against 294,026 and 203,073 on the change (10
  to 11 minutes against 3 and 4).

**Where the gap is: before the first line of code.** Every engine build read for three requests
after its brief, and wrote its first file in the fifth request. The plain builds wrote theirs in
the second.

| Build | Weighted tokens before its first file | After it | The plain builds, after theirs |
|---|---|---|---|
| `small-engine-1`, `-2` | 55,795; 49,032 | 87,606; 71,810 | 58,137; 73,301 |
| `medium-engine-1`, `-2`, `-3` | 63,649; 67,716; 73,945 | 103,989; 130,394; 149,600 | 136,023; 158,419 |

A plain build has spent 5,600 to 6,400 when it writes its first file. **After the first file the
engine arm is the cheaper one on the medium game**, by the means (127,994 against 147,221; two
builds of three are under both plain builds): it writes less (6,100 to 7,800 tokens that can be
seen, against 7,800 and 8,100), the menus, the settings and the picture being the engine's.
Everything it loses, and more, it loses in the first four requests: the brief, the guide with the
game file and the rules (4,400 tokens), the rest of the sample (its six screens, the strings, the
actions, the checks: about 5,000 more), and a fourth request of looking things up. By its first
file an engine build's context is 23,000 to 32,000 tokens and a plain build's 9,100 to 9,600,
which every later request then carries; and in each of the three reading requests the builder
plans as it reads (about 2,000 tokens of thinking in each).

- **What the fifth pass's cuts did there.** The one read was taken as written by every engine
  build (its second request is that command). The second read followed in every build all the same:
  the guide names the widgets and shows no screen, so the sample's screens were read. Every build
  still wrote a checks file ("or make them your game's") and ran it once or twice, and every medium
  and change build a player that reacts.
- **Still looked up**, each for the third measurement in a row: whether a run that starts again
  gets the same random numbers (`small-engine-1`), and what the built-in volume is and how far it
  goes (`medium-engine-1`, `medium-engine-3`).
- **The new rule held**: no build wrote a file twice. Builders still used here-documents for
  small patches of an existing file, which worked.

**What the five measurements come to.** The first meeting costs an engine build 45,000 to 65,000
weighted tokens that a plain build does not pay, at every size: the builder knows Python and has
to read the engine. On a one-screen game there is nothing for the engine to give back, and the
game costs nearly twice. On a medium game the engine gives back about half of it after the first
file. So at these sizes the best the engine has shown is a quarter dearer, and no change to the
guide alone will make it cheaper than the plain build: what would is the engine doing more of the
game's work than a title, a pause, settings and a picture (a larger game with things to keep,
kinds of things, levels, assets), which none of these three jobs asks for and which is not
measured. The `large` job is written for that and waits for the engine to keep things for a player
between runs.

## Results (2026-11-17: the sixth pass, the first meeting)

The same three jobs, harness, models and clean shell as the fifth measurement, on the engine with
the sixth pass (`57f515d0`; [FIRST_HOUR.md](FIRST_HOUR.md), section 13): one read of six files
named as the reading done, `cx.random_restart`, the volume's range said, and the checks'
description out of the first read. Fourteen builds, every one judged and every one reported:
[perf/leverage/2026-11-17-pass6](../../perf/leverage/2026-11-17-pass6/report.md). **Compare with
the fifth pass, not with the ones before it.**

**Through the engine the one-screen game now costs 1.61 times the plain build (it was 1.85) and
the medium game 1.22 times (1.28). The change to an existing game costs 1.47 times (1.25): the
engine builds cost what they cost before, and the plain ones cost a sixth less.**

| Job | Arm | Runs | Weighted tokens (mean) | Lowest to highest | Build time | Checks passed | Engine against plain | The same cell in the fifth pass |
|---|---|---|---|---|---|---|---|---|
| small (14 checks) | engine | 2 | 111,938 | 111,068 to 112,808 | 121 s, 112 s | 14, 14 | 1.61 times | 132,122 |
| small | plain | 2 | 69,612 | 66,839 to 72,384 | 67 s, 62 s | 14, 14 | | 71,322 |
| medium (19 checks) | engine | 3 | 180,729 | 163,128 to 206,039 | 182 s, 176 s, 776 s | 19, 19, 18 | 1.22 times | 196,431 |
| medium | plain | 2 | 148,684 | 147,791 to 149,576 | 137 s, 139 s | 19, 18 | | 153,572 |
| change (9 checks) | engine | 3 | 140,412 | 112,479 to 184,989 | 296 s, 223 s, 176 s | 9, 9, 9 | 1.47 times | 141,703 |
| change | plain | 2 | 95,452 | 89,155 to 101,750 | 68 s, 63 s | 9, 9 | | 113,478 |

**How much of that is the pass, read arm by arm.** The engine arm, which is what changed: 15% down
on the small game (and its two builds within 2% of each other, where the fifth pass's were 17%
apart), 8% down on the medium one, 1% on the change. The plain arm, for which nothing changed: 2%
and 3% down, and 16% down on the change. A plain arm has moved by 13% to 20% between two passes
before, so: **the small game got cheaper through the engine; the medium game's 8% is inside what
two measurements alike differ by; and the change's ratio rose because the plain builds were
cheap this time, not because the engine builds were dearer.**

**The gap the pass was aimed at, before the first file, is a quarter narrower.** Counted the same
way in both passes (the weighted tokens spent when the request that first writes a file begins):

| Build | Before its first file, fifth pass | Sixth pass | After it, sixth pass | The plain builds, after theirs |
|---|---|---|---|---|
| `small-engine-1`, `-2` | 55,795; 49,032 | 40,888; 42,321 | 70,180; 70,487 | 66,835; 61,207 |
| `medium-engine-1`, `-2`, `-3` | 63,649; 67,716; 73,945 | 56,858; 51,189; 40,038 | 116,163; 111,939; 166,001 | 141,401; 143,148 |

A plain build has spent 5,500 to 6,400 at that point, as before. Three engine builds of five now
write their first file in their fourth request (all five wrote it in the fifth before).

- **The one read was taken as written by every engine build**, and a second read still followed
  in all five: the sample's other four screens (title, pause, won, lost), which the guide says
  "are read when they are changed", and every game changes them. It took one more request in
  three builds and two in two, and with what those carry it is the 18,000 to 34,000 that still
  separates the one read from the first file.
- **`cx.random_restart` was used by all five new-game builds, and none wrote a generator of its
  own** (seven of eight one-screen builds had, over the passes before). Nobody looked up random
  numbers or the volume. One build looked something up at all (`medium-engine-2`: how a screen
  is named in the state, in the engine's design note).
- **No build wrote a checks file**, and every one deleted the sample's (in the fifth pass every
  build wrote one and ran it).
- **After the first file the engine arm is again the cheaper one on the medium game by the
  means** (131,368 against 142,275; two builds of three under both plain builds), and level on the
  small one (70,334 against 64,021).
- **Checks passed.** Every small game all fourteen. Medium: 19, 19 and 18 through the engine, 19
  and 18 without; the item failed is the same in both arms (the picture does not show what the
  state says). **Change: all nine in all three engine builds, for the first time**; the runners'
  rate, which nine engine change builds of twelve had failed over four passes, held in all three.
  The one thing that changed for those builds is the brief's first read, which now names the
  game's own README and strings; whether that is why is not shown.
- **Requests.** Small: 13 and 12 (the plain arm 7 and 7). Medium: 15, 16 and 20 (8 and 9). Change:
  23, 13 and 11 (11 and 9).
- **Where the change builds differ from the plain ones is after the reading, not before it.**
  Both arms read for most of a change (the plain builds had spent 62,000 and 73,000 of their
  89,000 to 102,000 before their first edit). The engine builds then try their change a run a
  request: 9 engine calls of 23 requests in `change-engine-1`, which is the dear one (184,989).
- **The dearest medium build was dear in time, not tokens**: `medium-engine-3` took 13 minutes,
  drew its pieces at the same depth as its field's background, and shipped frames in which most
  of them cannot be seen (its judge failed that item). The same fault is in the game the change
  round starts from. A game drawn in the engine's world has a depth to get wrong that a canvas
  does not; the guide does not say so.
- **A judging through the engine still costs more**: 435 to 730 seconds against 207 to 459.

**What the six measurements come to.** In a clean shell the engine has been 1.85, 1.28 and 1.25
times the plain build, and now 1.61, 1.22 and 1.47, with a plain arm that wanders by a sixth.
The sixth pass did what it was aimed at: the first meeting costs a quarter less, and the
one-screen game, which is nearly all first meeting, is measurably cheaper. What is left of the gap
before the first file is 34,000 to 51,000 weighted tokens over a plain build's 6,000: the brief,
the one read (about 22,000 for both), and the second look at the screens. The conclusion of the
fifth measurement stands: at these sizes the first meeting is not paid back, and what would make
the engine the cheaper arm is a game that asks for more of what the engine does. Two things the
runs point at that are not built: the other screens folded into the one read (or a game's words
changed without reading its screens), and a line in the guide on depth for a game drawn flat.

## Results (2026-11-20: the shape library, the small job)

Another question, with the same harness: does the engine's library of shapes by name
(`specula-formae`) make props cheaper for a cold agent, and does a cold agent find it? The job
(`kit`: four pickups for a game, each a model recipe: a gear token with twelve teeth and a hole, a
heart with softly rounded edges, a five-pointed star, a coin with a raised rim; a headless build
that prints each one's triangles, closedness and bounds; a frame of each) was built in three arms
that all build through the engine:

- **`none`**: an engine built without the library (`8b67796d`);
- **`found`**: an engine with the library and its pointers (`4deccc36`), **and a brief that says
  not a word of it**, to the byte the brief of `none`: whether it is found by the engine's own
  pointers (a new project's notes, the tools' descriptions, the documents);
- **`told`**: the same engine, with one more line in the brief that names the library and its two
  calls.

Builders: Sonnet 5.5, a cap of 400,000 tokens and an hour. Judges: Opus 5.5, one list of nine
items, against a build they had not seen. The jobs and the arms are data on the library's branch
(`evals/leverage-props/`; the harness reads a data folder since that branch). Every run is in
[perf/leverage/2026-11-20-props](../../perf/leverage/2026-11-20-props/report.md).

**One run is not a measurement and is kept as it came out.** `kit-none-1` never had an engine:
the harness gave it port 7680, which a Windows service held on this machine; the engine failed to
start about eight times, and the builder wrote its recipes by guess and built nothing (0 of 9).
It is in the folder and in the report's tables, **whose means and ratios for `none` therefore
include it and are not to be quoted**. A third `none` run was made on a free port. The figures
below are the six runs that had an engine.

| Arm | Weighted tokens (each run) | Mean | Requests | Engine calls | Checks passed |
|---|---|---|---|---|---|
| `none` | 447,786; 594,327 | 521,057 | 31; 48 | 50; 158 | 9, 9 of 9 |
| `found` | 509,455; 663,346 | 586,400 | 44; 52 | 139; 232 | 8, 9 of 9 |
| `told` | 507,469; 562,963 | 535,216 | 64; 56 | 130; 173 | 9, 9 of 9 |

**What it says.**

- **A cold agent finds the library without being told.** Both `found` builders took shapes by
  name in two or three of their four recipes, as both `told` builders did. The path works.
- **On a job this small the library made nothing cheaper.** `found` cost 1.13 times `none` and
  `told` 1.03 times, for the same checks passed, **and two runs of one arm differ by a third**
  (447,786 and 594,327 with nothing changed between them): the arms cannot be told apart. No
  arm wrote fewer words (24,000 to 35,000 tokens written in every run).
- **Where a build's tokens go is not the shapes.** By the ledgers (`examen leverage where`), in
  every arm a build is reading (its own project, the engine's documents) and looking up tools
  and their arguments (11,000 to 38,000 tokens of answers a run for the two together), then
  calling the engine to build and to look, again and again (8 to 35 steps), with the whole
  context read again at every request. Writing code is 2,500 to 5,600 of the 24,000 to 35,000
  tokens a run writes, with the library or without. The `told` builders read a third as much
  as the `none` builders and called the engine two to four times as often.
- **Builders wrote their own tooling for what the engine could say in one call.** By the
  judges' notes, three of the four library-arm builders wrote their own check of watertightness
  on an exported file, where `model_build`'s report says whether a model is closed; two
  builders (one in `none`, one in `told`) wrote their build and frame commands in Python; and
  every builder wrote a script to build four recipes and print four lines. That is the larger
  cost, and it is the engine's to take away: a call that builds every recipe of a project and
  prints each one's facts, and a call that frames each. (The same lesson as `specula run` and
  `specula check` for games, in the passes above.)
- **A named shape does not carry its own good use.** One `told` builder drew its heart by hand
  beside a library that has one; one `found` builder asked for a `pillow` with an inset of a
  tenth of a millimetre and called the square-edged result softly rounded (its judge did not).

**What it does not say.** Four pickups are the smallest job there is: a dozen props, where
shapes are shared and a style has to hold across them, is the other job written
(`props`), and it is not run. Two runs a cell cannot show a difference smaller than a third. And
nothing here measures a project that already holds assets to reuse, which is what the next stage
builds (ROADMAP M6: a project's own catalogue; the engine telling an agent where everything is).

## Results (2026-11-20: the same job after the builders' blindness was mended)

The result above (found, and not cheaper) made the owner doubt the whole approach, and ask
whether the builders had been working blind. They had. The runs were read request by request
and the engine's code beside them (by the lead, by hand, with no agents):

- **No answer of the engine's said whether a model is closed**, and the brief told every builder
  that `model_build` did (a sentence nobody had checked). The job asked for it, so each builder
  exported its models and wrote its own check: about 90,000 of 509,000 weighted tokens in one
  run.
- **Nothing said how a recipe is written** but a design document of 62 KB (one builder read it
  whole and carried it through 41 requests: 134,000 weighted tokens) or one refused build a
  field: 30 to 146 builds a run for four props, up to 77 of them refused.
- **The shape the job asked for could not be made**: the library's heart takes no soft edge (its
  point and its notch close an inset at once). One builder spent nine builds finding that out.
- **A new project said nothing about models**: its notes are about a game.

The engine's parts had been verified for correctness, by tests and by reviewers, and never for
use: until that comparison nobody had made one prop a game wants with them. Building games
through the engine had six passes of exactly this treatment (the sections above); building
models had none, and was measured cold.

**What was mended** (four changes, by hand, on the engine that was measured): every build's
answer says `closed` and `open_edges`; `model_schema` says a recipe's whole format in one
answer from the parser's own tables, with an example that builds, and a sculpt's nodes with a
soft heart that builds; a new project has one page on models (`notes/models.md`), held against
the engine by a test; a raise that cannot go as deep as asked says what to do instead.

**Measured again**, the same job, the same builder and judge models, the same harness, two runs
a cell; every run is in
[perf/leverage/2026-11-20-props-fixed](../../perf/leverage/2026-11-20-props-fixed/report.md):

| Arm | Before the mends (the runs above) | After | Requests before, after | Builds, of which refused: before, after | Checks |
|---|---|---|---|---|---|
| `found` (the same brief, to the byte, in both) | 509,455; 663,346 | 219,039; 360,234 | 44, 52; 29, 34 | 53/11, 146/77; 18/2, 22/4 | 8, 9; 9, 9 of 9 |
| `guided` (new: the brief points at the project's page on models, not at a design document) | | 173,926; 158,904 | 22, 20 | 16/0, 23/3 | 9, 9 of 9 |
| `none` (an engine without the library, before the mends) | 447,786; 594,327 | not measured again | 31, 48 | 30/7, 68/6 | 9, 9 of 9 |

- **With the brief unchanged, the same job costs half** (289,637 against 586,400 on the means):
  the engine's own answers and the project's own page did that.
- **With a brief that points at that page, it costs 0.28 times what it did** (166,415), with two
  runs 9% apart where the arms before were a third apart, and every check passed.
- Every builder asked `model_schema` (five or six times a run), took shapes by name in two of
  four recipes and sculpted the heart from the example; none wrote a check of closedness
  (each one's script reads `closed` from the engine's answer). The two `guided` builders never
  opened the design document; the two `found` builders, whose brief still names it, read parts
  of it, which is most of the difference between the two arms.
- **What is still paid for and need not be**: every builder still wrote a script of its own to
  build the four and print their facts (the engine now has a call for that, measured next), and
  the shell's answers were still indented.
- **What the judges saw that the list does not ask**: the sculpted heart has a crease down its
  middle where its two halves join and a ragged seam at its sides, and five to nine thousand
  triangles where the other props have hundreds; the coin's rim does not show in a front view;
  a new project's style check fails a prop for being brightly coloured.

**What this does not show.** `none` was not measured again: an engine with the mends and
without the library does not exist as a build, so the two cannot be told apart here (the
library by name was used in two of four recipes in every run; the mends are what changed
between the two columns of `found`). Two runs a cell. One small job. And whether the engine
pays on a game that needs what it has (3D, physics, sound) is still not measured.

**The lesson, in the owner's words**: "sometimes, you need to pause, if you're seeing same
numbers despite same attempts, which means something else has gone wrong."

### With one call that builds every recipe, and compact answers (2026-11-20)

Two things the runs above still paid for were then mended: `model_build_all` builds every recipe
of a folder and answers a line each (triangles, size, closed, or the refusal with where and
why), and the shell's commands print compact JSON through a pipe (indented for a person at a
terminal). The `guided` arm again, two runs
([perf/leverage/2026-11-20-props-build-all](../../perf/leverage/2026-11-20-props-build-all/report.md)):

| | Weighted tokens | Requests | `model_build` calls, refused | Checks |
|---|---|---|---|---|
| `guided`, before these two | 173,926; 158,904 | 22, 20 | 16/0, 23/3 | 9, 9 of 9 |
| `guided`, with them | 158,740; 151,322 | 18, 19 | 13/0, 13/0 | 9, 9 of 9 |

**A small gain, inside what two runs alike differ by** (155,031 against 166,415 on the means:
7% less, where the two runs of a cell are up to 9% apart). Both builders wrote their four
recipes in one go, called `model_build_all` (two and three times), and had no build refused.
Each still wrote one script: the job asks for commands that work from no engine running, and
that script starts the engine, calls it and stops it. What a run costs now, by its ledger: the
brief and the project's page, `model_schema` and a few look-ups (about 7,000 tokens of
answers), the four recipes, two or three rounds of building and looking, the script and the
README. **The floor for this job through the engine is about 150,000 weighted tokens**; the
same job cost 510,000 to 660,000 in the first comparison, earlier the same day, in the arm with the old brief.

One run's clock time was thirteen minutes where the others took two or three: its builder ran
`python -` with a here-document, which hangs on this machine until its time limit. No tokens.

## Results (2026-11-21: six jobs, three in 2D and three in 3D, the engine against none)

The owner asked for the plain comparison again on the engine as it stands (`7f2167f8`: the
blindness mended, typed `Spawn` words, what physics tells a module, a push and a bounce), and
for three jobs that need what the engine has. The 3D jobs are a second data folder,
`evals/leverage-3d`: **small3d** (a ball on a board with rigid-body physics, four coins, three
lives, three sounds), **medium3d** (three boards, six crates to push, a moving bridge, a menu,
saved settings, two languages) and **change3d** (a fourth board with a ramp, French, a setting
for the ball's mass, by a builder who did not build the game). Builders Sonnet 5.5, judges
Opus 5.5, two runs a cell, 24 runs, all ended `done`, all judged
([perf/leverage/2026-11-21-2d](../../perf/leverage/2026-11-21-2d/report.md),
[perf/leverage/2026-11-21-3d](../../perf/leverage/2026-11-21-3d/report.md)).

| Job | Plain (weighted tokens) | Engine | Times plain | Requests, times plain | Checks, plain | Checks, engine |
|---|---|---|---|---|---|---|
| small | 67,724 | 120,172 | 1.77 | 1.73 | 14.0 / 14 | 14.0 / 14 |
| medium | 150,926 | 183,690 | 1.22 | 1.83 | 19.0 / 19 | 19.0 / 19 |
| change | 103,624 | 121,900 | 1.18 | 1.17 | 9.0 / 9 | 8.5 / 9 |
| small3d | 136,317 | 220,499 | 1.62 | 2.53 | 15.0 / 15 | 15.0 / 15 |
| medium3d | 430,514 | 736,712 | 1.71 | 2.85 | 23.5 / 24 | 22.0 / 24 |
| change3d | 259,524 | 321,007 | 1.24 | 1.58 | 10.5 / 11 | 11.0 / 11 |

**The engine costs more on every job, 3D included.** The 2D figures stand where the sixth pass
left them (1.61, 1.22 and 1.47 on 2026-11-17): medium the same, change lower, small higher,
each inside what two measurements of a cell have differed by. At the runner's list prices the
builds cost 3.15 USD (2D) and 7.92 USD (3D), and the judging 9.58 and 15.83.

**The items that failed.** `change-engine-1` c01: runners sent every 60 ticks where the brief
says 90 (the builder's constant). `medium3d-plain-1` m22: a Spanish heading wider than the
frame. `medium3d-engine-1` m10: board 2's crates appear in the state one tick after the board
changes. `medium3d-engine-2` m16: a ball on the moving bridge is not carried at rest (it moves
at about twice the bridge's speed, and a push against it speeds it up). Both `medium3d-engine`
runs, m23: the judge could not see where the volume setting scales a sound (the engine does
it; the README did not show it). `change3d-plain-1` c02: the hand-written ramp is only its top
plane, and the ball passes through its sides. The engine's ramp was solid in both runs.

**Where the extra goes: requests.** One `medium3d` run of each arm, by its ledger:

| | Plain (`medium3d-plain-2`) | Engine (`medium3d-engine-2`) |
|---|---|---|
| Requests | 17 | 58 |
| Read again (the context, at each request) | 72,206 | 365,762 |
| New in the context | 74,218 | 118,582 |
| Written out | 231,270 | 267,350 |

The engine's builder wrote about what the plain one wrote. Four fifths of the difference is
the context read again at three times as many requests. Its 58 requests, read one by one
(`examen leverage trace`): seven to read the brief, the project's pages, the SDK's functions
and the documents (107,000 weighted tokens); the whole game written at the eighth; first runs,
frames and the four sounds, all working, by the seventeenth (221,000 so far, less than either
plain build's whole cost); then **nine requests probing the bridge, seven writing a Python
wrapper and a driver** so that a headless run takes the brief's inputs and prints the brief's
state, and **eighteen tuning the ball and the crates** (spin, mass, sleep, bounce), each a run
that reads a few numbers and changes one constant, at 10,000 to 15,000 weighted tokens a
request because the context is read again each time.

**What this says.** A plain builder writes its own physics and knows what it will do; the
engine's builder has a better solver and has to measure it. The judges found the plain builds'
physics crude (crates that never rotate, a crate at rest with nothing under its centre, a ramp
with no sides), and little of that is on a list of checks. One judge wrote down as a defect
that a 4 m/s2 push moved the rolling ball at 2.86: that is 4 x 5/7, what a solid ball that
rolls without slipping does. Sight of the tools made the first working game cheap; it does
nothing for not knowing what the world will do.

**Not controlled, and not shown.** Two runs a cell. The jobs are of a size a model writes
whole (600 to 1,800 lines); nothing here measures a game too large for that, several agents
in one world, or a frame rate. The bridge's fault is a judge's finding and has not yet been
reproduced by hand.
