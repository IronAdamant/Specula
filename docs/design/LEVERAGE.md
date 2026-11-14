# Design: measured leverage

Status: built 2026-11-14 (M5). Data: `evals/leverage/` (the settings and the three jobs). Harness:
`scripts/leverage.py` (its unit tests: `scripts/leverage_test.py`, run by `cargo test -p specula --test leverage_data`). Results: `perf/leverage/`. Roadmap item: "Measured leverage" in
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
| Engine calls, and the bytes sent and answered | every call through `leverage.py call` (`costs/<run>.calls.jsonl`) |
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
- **The time limit is enforced**: `leverage.py run` stops the builder's process, and what it started, at
  `wall_s`; whoever starts agents another way stops them and records `--ended wall`. A run stopped there
  has no totals from its agent: its tokens are marked incomplete and it is left out of its cell's mean.
  The run's engine is stopped after every builder and every judge, however they ended.

## Running it (any vendor's agent)

`leverage.py run` starts a builder or a judge as **a process of its own, in the run's folder**, from the
command line in the settings (`runner.command`: Claude Code's by default, with six tools and no MCP
servers; any agent's command line that takes a prompt and runs to its end fits). A process started there
sees its brief and nothing of the engine's repository unless it goes and reads it. Agents started another
way (another vendor's harness) are handed the brief by hand and their figures given to `record`.

```
python scripts/leverage.py --check                              # the data is well formed
python scripts/leverage.py prepare small engine 1 --out ROOT --exe target/release/specula.exe
python scripts/leverage.py prepare small plain 1 --out ROOT
python scripts/leverage.py run ROOT/small-engine-1              # the builder; stopped at the time limit; then 'finish'
python scripts/leverage.py run ROOT/small-engine-1 --judge      # the judge; its verdict.json is recorded
python scripts/leverage.py prepare change engine 1 --out ROOT --exe ... --from ROOT/medium-engine-1
python scripts/leverage.py report ROOT                          # report.md and report.json
python scripts/leverage.py export ROOT perf/leverage/DATE       # what the repository keeps: no games, no paths

# by hand, for an agent with no such command line:
python scripts/leverage.py finish RUN; python scripts/leverage.py record RUN --tokens N --tool-uses N --duration-s N --by-kind I W R O
python scripts/leverage.py judge RUN; python scripts/leverage.py judge RUN --verdict RUN/verdict.json
```

The engine arm's builder never runs the engine's executable. It starts and stops its engine with
`leverage.py engine start|stop RUN` (headless, on the run's own port, with the run's own settings folder,
so it is never listed among a person's projects) and calls it with `leverage.py call RUN TOOL '{json}'`
or `TOOL @args.json`, which is the engine's own `specula call` with the run's port and identity, logged.

## What is and is not controlled

- **Controlled:** the model, the brief, the budget told, the acceptance list, the judge's model and
  instructions, the machine.
- **The engine arm reads documents; the plain arm has none to read.** Reading the engine's documents
  costs tokens, and those tokens are in the engine arm's count. That is the honest price of a tool one
  has to learn, and what tool descriptions and catalogs exist to make small.
- **The SDK is put in the project.** A module's crates must lie under the engine's working directory
  ([MODULES.md](MODULES.md)), so `prepare` copies `specula-core` and `specula-sdk` into the engine arm's
  work folder (under `crates/`, where the build service trusts them) as a Cargo workspace. A game project getting its SDK from the engine is M6's export work;
  here the harness does it by hand, and it counts as installation, as the plain arm's toolchain does.
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
Claude Sonnet 5.5 and judges Claude Opus 5.5, each a process of its own (`leverage.py run`), on one Windows
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

**Where the engine arm's tokens went** (`leverage.py where RUN`; owner, 2026-11-14: "Good to have that data
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
