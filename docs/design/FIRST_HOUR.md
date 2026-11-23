# Design: the first hour (M6, part 3)

Status: **built 2026-11-16** (branch `m6-first-hour`): eight slices, four review rounds, the gate, and the
comparison run again. **What it was for did not happen: the engine arm did not get cheaper** (slice 8).
Roadmap: M6, "The engine pays off by game size" and the items beside it ([ROADMAP.md](../ROADMAP.md)).
Measured reason: [LEVERAGE.md](LEVERAGE.md). Siblings: [MODULES.md](MODULES.md) (the module host),
[UI.md](UI.md), [LOCALISATION.md](LOCALISATION.md), [ACCESSIBILITY.md](ACCESSIBILITY.md) (saved settings),
[PLAYTESTS.md](PLAYTESTS.md), [PROJECT_DATA.md](PROJECT_DATA.md), [LAUNCH.md](LAUNCH.md).

## What M5's comparison said

The same small and medium games cost three to five times the tokens through the engine. Not because
builds failed, but because of this:

| Found | What it cost |
|---|---|
| Nine or more steps before the first line of code: finding tools, reading the SDK and an example, setting up a workspace | every step reads the whole context again |
| A project outside the engine's repository has no SDK; the harness copied one by hand | an agent on its own could not start |
| A module-built game cannot reach interface screens, strings by language or saved settings: only a hosted game's own executable can | every builder wrote menus, tables and a settings file itself, in the module or a script |
| The engine ticks at 30 Hz with no game hosted; the briefs asked for 1/60 s | every builder worked round it |
| A game's own check started an engine and built its module on every run | judging cost more than building |
| In the change round, where the game's own documents said how it was built, the gap was narrowest (1.7 times) | what a project remembers is worth more than what an agent is told |

So part 3 is: **a project that starts whole, an engine that already has the menus, and a project that
remembers.**

## The slices

| # | Slice | State |
|---|---|---|
| 1 | The SDK comes from the engine, and is trusted by what it says | built, reviewed (R1 to R4), gated |
| 2 | A module builds in a folder of any depth | built, reviewed (R1 to R4), gated |
| 3 | The game file (`game.json`): tick rate, modules, screens, settings, outcome | built, reviewed (R1 to R4), gated |
| 4 | The project shell: screens, strings, saved settings and pause for a module-built game | built, reviewed (R1 to R4), gated |
| 5 | Outcome and score as data; `game_state`; playtests and the balance lab read them | built, reviewed (R1 to R4), gated |
| 6 | What a project remembers: notes kept with the project, and the brief an agent is given at its first call | built, reviewed (R1 to R4), gated |
| 7 | `specula new` makes a project that builds and plays; the guide it writes | built, reviewed (R1 to R4), gated |
| 8 | The comparison, run again with the same harness | run on `2a0c5669`: the engine arm did not get cheaper (below, and LEVERAGE.md) |
| 9 | A headless run: `specula run` (the second pass) | built, reviewed, gated; measured on `bbe9fa36` (LEVERAGE.md, "Results (pass 2)") |
| 10 | A run that says what went wrong, and a player that reacts: `--follow` (the third pass) | built, reviewed, gated; measured on `6cfff79b` (LEVERAGE.md, "Results (the third pass)": the medium game costs the same as without the engine) |
| 11 | The game's own checks, every run in one command: `specula check` (the fourth pass) | built, reviewed, gated; measured on `d18e1131` (LEVERAGE.md, "Results (the fourth pass)": no build cheaper, the small game dearer; the checks came on top of single runs) |
| 12 | What a first build is not asked for: the guide and the brief, cut (the fifth pass) | built, reviewed, gated; measured on `9450e0f4` in a clean shell (LEVERAGE.md, "Results (the fifth pass)": 1.85, 1.28 and 1.25 times; the gap is before the first line of code) |
| 13 | The first meeting: one read that is enough, and a random stream that begins again (the sixth pass) | built, reviewed, gated; measured on `57f515d0` (LEVERAGE.md, "Results (2026-11-17: the sixth pass, the first meeting)"): 1.61, 1.22 and 1.47 times the plain build, and a quarter less spent before the first file |
| 14 | What a second reading of the six-job comparison asked for: the pages say what physics will do, a slab that carries, a run that speaks a brief and is read on its way (2026-11-21) | built by hand, not reviewed, not measured |
| 15 | What the engine looks for itself: a number of the rules' given by a run (`Cx::param`, `--set`), a number tuned to a condition (`specula tune`), a script of inputs found (`specula find`) (2026-11-21) | built by hand, not reviewed; `search_check` holds it end to end |

## 1. The SDK comes from the engine

- The engine carries the SDK's sources (`specula_logic::sdk_source`, `include_str!`: no build script) and
  writes them into a project's `crates/specula-sdk` and `crates/specula-core`: by `specula new`, and by
  `specula sdk [--project DIR]` to put them back or bring them up to date.
- **The SDK's edition of core.** A project's `specula-core` holds the three modules the SDK is built on
  (deterministic maths, vectors, JSON), each file the engine's own, under a `lib.rs` that names only
  those: 130 KB of text where the whole crate is 490 KB. A module needs no image codecs, and an agent that
  opens the SDK to learn it reads a quarter of the text.
- **Trust by content.** The build service compiles the SDK and core without its source check, because the
  ABI needs `unsafe`. That exception was a path (`crates/specula-sdk` under the project): whatever was
  written there was compiled unchecked. It becomes: a copy that matches the engine's sources byte for
  byte (`sdk_source::is_the_engines`), or the crates this engine was built from, or a copy identical to
  those (the engine's own tests copy the whole crates). Anything else under that path is checked like any
  other crate, and its `unsafe` refuses the build.
- **The version is checked at `logic_build`:** a project whose SDK differs gets `sdk_differs`, with the
  files and the one command that puts them right. A loaded module's ABI table already names the SDK's
  version.

## 2. A module builds in a folder of any depth

On Windows the toolchain's linker opens a path of at most 259 characters, and rustc's file of linker
arguments lies about 108 characters below the engine's cache. Today a deeper project is refused by name
(`build_path_too_long`). An agent's scratch folder is often that deep.

- Before a build, the engine works out the deepest path the build will make. When it would pass the
  limit, the build's target goes to a short folder of the engine's own, per user (the system's local
  application data, `Specula\b\<hash of the project's cache folder>`), and the answer says where
  (`build_dir`, and why).
- That folder is the engine's cache like any other: the disk budget counts it and `disk_prune` clears it.
- `build_path_too_long` stays for the case that even the short folder is too deep (a very long user name).

## 3. The game file

`game.json` at the project's root, `specula.game.v1`. A project that has one gets the project shell
(below); a project without one is as before. Everything in it has a default, so the smallest file is
`{"format": "specula.game.v1"}`.

```json
{
  "format": "specula.game.v1",
  "title": "Hold the gate",
  "tick_hz": 60,
  "modules": ["modules/gate"],
  "start": "title",
  "screens": { "title": "title", "pause": "pause", "won": "won", "lost": "lost", "hud": "hud" },
  "locale": "en",
  "settings": [
    { "name": "volume", "kind": "int", "min": 0, "max": 10, "default": 7, "is": "volume" },
    { "name": "language", "kind": "locale" },
    { "name": "difficulty", "kind": "choice", "options": ["easy", "normal", "hard"], "default": "normal", "sim": true }
  ],
  "outcome": {
    "lost": { "key": "gate.hp", "lte": 0 },
    "won": { "all": [{ "key": "gate.wave", "gte": 5 }, { "key": "gate.runners", "eq": 0 }] },
    "score": "gate.score"
  }
}
```

- **`tick_hz`** (1 to 240; 30 when absent): the clock's rate for the run, as a hosted game's
  `Game::tick_hz` sets it. Fixed for the run, and a save names it.
- **`modules`**: the module crates the game is made of. **The engine does not run a project's native code
  because the folder was opened.** They are built and loaded by an owner or a granted lead
  (`logic_build`), as now, or at start when the operator says the project's code is theirs to run
  (`--run`; `specula play NAME`). Opening a folder someone sent you shows its screens and runs none of
  its code.
- Validated like every data file: structured errors that say what to change; unknown keys are refused by
  name; it is a project file, so claims, versions, the journal and undo apply, and a change is picked up
  while the engine runs.

## 4. The project shell

A `Game` built into the engine, used when the opened project has a `game.json` and no hosted game's
executable. It is to a module-built game what Sepulcra's own shell is to Sepulcra, written once.

**Screens are the project's data**: `assets/ui/*.json`, `specula.ui.screen.v1`, the format Sepulcra's
screens use, drawn and driven by `specula-ui`. The `ui_tree`, `ui_frame`, `ui_act` and `ui_check` tools
work on them as they do on a hosted game's.

**What a screen shows is what the game keeps.** A screen binds to:

| Key | What |
|---|---|
| `<module>.<key>` | the module's state store (`Cx::set_state`): numbers, text, true or false, lists of rows |
| `setting.<name>`, `setting.<name>.text` | the player's settings |
| `game.outcome`, `game.score`, `game.paused`, `game.screen` | the shell's own state |

- **No new call to learn.** A module already keeps what matters in its state store; the HUD reads it
  there. A value that is a map with `msg` (and `args`) is a line from the string tables, so numbers and
  plurals are formatted by the player's language and the module never holds a translated word.
- Reading is one way. Nothing of the interface reaches the simulation except the two events below.

**What a press does.** A button's `action` is a word. The shell knows these, so menus need no code:

| Action | Does |
|---|---|
| `open:NAME`, `close`, `back`, `goto:NAME` | the screen stack |
| `start`, `pause`, `resume`, `title` | the run: the title holds the clock, a pause stops it, `title` ends the run |
| `set:NAME:VALUE`, `step:NAME:+1`, `cycle:NAME` | a setting |
| `save:SLOT`, `load:SLOT` | save games, through the engine's own |
| `quit` | the player's alone: refused from an agent |

Any other word goes to the game: the engine hands the modules an event `ui` (`{action, widget, screen}`)
at the next tick boundary, in order, saved with the run like any event. A module reads it with
`Cx::events()`, as it reads another module's.

**Pause is the clock's.** A game that is paused does not tick: the shell pauses the engine's clock, so a
cooldown counted in ticks does not run out behind a menu. `time_control` says the game paused it.

**Strings by language**: `assets/strings/<locale>.json`, `specula.strings.v1`, as Sepulcra's. The chosen
language is a setting and is kept (Sepulcra's is not: found while this was designed). The `l10n_*` tools
work.

**Settings are kept**: the game file declares them; the shell keeps them in the per-user settings file
the engine already manages (`--settings DIR|off`, and the rule that an agent's changes never reach a
player's own file). Built in: `language`, `volume`, `ui_scale`, `text_size`. A setting marked `"sim":
true` is one the rules read (a difficulty): it reaches the modules as an event `setting` (`{name,
value}`) at the start of a run and when it changes, at a tick boundary, so a replay sees the same.

**Said plainly, not in this slice:** key rebinding screens, colour-vision themes, captions and subtitles
(Sepulcra has its own; captions are part 6's), and Sepulcra moving onto this shell. Sepulcra keeps its
shell until then; about 3,000 of its lines are generic and are the model for this.

## 5. Outcome and score

"Conditions in the game" (owner, 2026-11-16: "like winning conditions for races, or scoring system"). A
game declares them in `game.json`, over the same keys its screens bind to (and `world:/path.prop` for
something in the world), with the matchers playtests already use (`eq`, `ne`, `lt`, `lte`, `gt`, `gte`,
`in`, and `all` / `any`):

- `won`, `lost`: when one holds after a tick, the run ends: the outcome and its tick are recorded, the
  clock pauses, the modules get an event `outcome`, and the `won` or `lost` screen opens.
- `score`: a key holding a number. `standings`: a key holding a list of rows (a race's order, a table of
  players), shown and reported as it is.
- **`game_state`** (a new tool, open to every agent): the outcome, the score, the standings, the screen
  that is up, the settings and the tick: one bounded answer, which a playtest's `expect` and the balance
  lab's targets read. A game no longer writes its own "print my state" for a check.
- A game whose rules are code can also just keep `won` and `lost` as keys of its own and name them.

## 6. What a project remembers

(Owner, 2026-11-16: "notes that agents can use to note about the game, both for memory and system ...
as it will be hard to remember every detail". The owner's own tools are the model, and their functions
may be written into the engine: Relatum's notes that supersede, wikifier's "why this file", Stele's
search.)

- **Notes are project files**: `notes/<id>.md`, a few lines of front matter (title, kind, what it
  supersedes, the paths it is about) and text. Project files have claims, versions, a journal and undo
  already; a person can read and edit them with any editor; they travel with the folder.
- **Kinds**: `brief` (what the game is: one), `how` (how it is built and checked), `decision` (what was
  decided and why), `thread` (what is open), `fact` (people, places, numbers).
- **Tools** (`memory_*`, open to every agent, bounded): `memory_note` (write one, or supersede one),
  `memory_get`, `memory_list` (by kind, and by words: a plain match on title and text), `memory_why`
  (the notes about a path, with who last changed that file and when, from the project's journal).
- **The brief at the first call.** `agent_hello` on a project with notes answers with the brief, the
  `how` note and the titles of the rest, in at most about 600 tokens: what the change round's builders
  got from the game's own documents, given without being asked for.
- **Superseded notes stay** and are not listed, as in Relatum: memory that only grows is not read.

## 7. `specula new` makes a project that builds and plays

A new project today is a folder, an empty `assets` and a marker. It becomes:

| Path | What |
|---|---|
| `game.json` | the game file, with a title, a pause, a settings and two end screens named |
| `assets/ui/*.json`, `assets/strings/en.json`, `assets/input.json` | those screens, their words, the actions that drive menus |
| `assets/style.json` | a plain style, so a recipe builds on the first try (a new project had none, and `model_build` refused) |
| `modules/<name>/` | a module that builds: one thing spawned, one number counted and shown, the two events read |
| `crates/specula-sdk`, `crates/specula-core`, `Cargo.toml` | the SDK and the workspace (any folder under `modules/` is a member) |
| `checks/runs.json` | the game's own checks, played by `specula check` (section 11) |
| `checks/smoke.json` | a playtest that starts the run, steps it and reads `game_state` |
| `notes/brief.md`, `notes/how.md` | what this is, and how it is built and checked |

- **The guide is `notes/how.md`**, about forty lines: the loop (one engine left running, `logic_build`
  with `reload`, `time_control step`, `playtest_run`), where each kind of thing lives, and the five
  tools to start from. It is the project's own note, so it is the first thing a later agent is told.
- `specula new NAME --bare` makes what it makes today.

## 8. The comparison again

The same three jobs, harness, models and judges as 2026-11-14, with the engine arm starting from
`specula new` and nothing copied by hand. Reported beside M5's numbers, every run, failures included.
A fourth job that needs more to reuse is part 12's.

**Run on 2026-11-16** ([LEVERAGE.md](LEVERAGE.md), "Results (2026-11-16)";
[perf/leverage/2026-11-16](../../perf/leverage/2026-11-16/report.md)). Twelve builds, all judged, checks
passed alike in both arms. Through the engine a small game cost 6.8 times the plain arm's tokens (M5:
5.2), a medium one 4.0 times (3.1), a change 2.2 times (1.7). With two runs a cell only the medium rise
is beyond what two runs alike have shown; no cell got cheaper.

- **What part 3 closed:** M5's findings that a project had no SDK, that a module-built game could not
  reach screens, strings or settings, and that the tick was 30 Hz. Every builder started from `specula
  new`, wrote a game file and drew its HUD as a screen of the shell.
- **What it did not move:** the number of requests, which is the cost. The medium builds took 50 and 61
  where M5's took 31 and 32. More of the engine is within reach and the builders spent requests finding
  it; a headless check of a game with menus is still written by hand, call by call; and no builder
  called a memory tool or wrote a note (four of six never said `agent_hello`, where the brief is given).
- **What follows** is in ROADMAP M6 ("The first hour, measured again"): fewer steps to a running game,
  not more that the engine can do.

## 9. A headless run (the second pass, 2026-11-16)

Slice 8 measured the first seven and the engine arm had not got cheaper. Every request of those twelve
runs was then laid out (`examen leverage trace RUN`; [LEVERAGE.md](LEVERAGE.md), "Where every token
went"). What it showed:

| Found in the ledgers | Share of an engine build |
|---|---|
| The headless check was a script each builder wrote and debugged (180 to 530 lines, then 6 to 10 patches at the largest context of the run) | 33% to 42% |
| Discovery before any code: the sample, tool lists, the SDK's sources, and this design document for the game file's format, each carried by every later request | 17% to 25% |
| Waiting for its own slow check, starting and stopping engines, finding leftover processes | 7% to 23% |

So the second pass is one loop, the plain arm's: **edit, run, read.**

- **`specula run [NAME|FOLDER] --ticks N [--inputs T:WORD,...] [--seed N] [--settings FOLDER]
  [--frame FILE.png --size WxH]`** builds the game file's modules as they are on disk, plays the game
  from its start with no window, prints how it stands as one JSON object, and ends. Nothing is left
  running. A build that fails is the compiler's diagnostics as data and exit code 1. About 4 s for a
  short run of the sample.
  - **An input is a word at a tick**: a press on the interface (`up down left right enter escape`, or
    a source such as `Key.Space`), an action of a module (held until the next input, as a held key
    is), or `none`. A word that is both does both, as one key does. Words joined by `+` are held
    together, and an action that is not a button takes its value (`throttle=0.5`, `move=1/0`). A
    source presses the interface only. A word the game does not take, a value of the wrong kind
    and an input after the last tick are refused before a tick is played, with the words it takes.
  - **The game's clock is the game's.** The script counts every tick; a tick of the game runs only
    while the game has its run's clock running. The title and a pause hold it and an ended run has no
    further tick, as for a player. The engine is started with the clock standing still
    (`Engine::script_clock`; `TimeOp::Play`), so no time of the machine becomes a tick: the same seed
    and inputs print the same state, to the letter, and the same frame, byte for byte.
    Nobody else's word moves that clock: `time_control` answers `info` and refuses the rest while
    the run lasts, and a run that was asked for ticks with the game's clock running and did not
    get them fails (`ticks_lost`) instead of printing a state its script did not play.
  - **`--seed`** seeds the modules' random streams. **`--settings FOLDER`** is where the run's player
    settings are read and kept: a later run with the folder comes up with them. Without it they are
    kept nowhere, and the run's engine is listed nowhere.
  - **One engine has a project at a time**: a run on a project that an engine has open is refused
    with that engine (`project_open`); another run of the same project is waited for, also when the
    two began in the same moment (the engine that was refused the lock says so, and its command
    starts it again).
  - **It is the operator's command**, as `specula play --as-is` is: what is on disk is built and
    loaded with the user's rights, by whoever runs the command ([MODULES.md](MODULES.md)). It records
    no approval: `--run` still loads only what an owner or a granted lead built.
- **The game file says what is printed and how the game is looked at.**
  - `"report"`: each line a name and the key whose value it shows, in order: what a module keeps
    (`gate.walkers`), `setting.NAME`, `game.phase`, `game.score`, `game.outcome`, `game.tick`,
    `run.ticks`, `run.seed`, `ui.screen`, `ui.focus` (the focused button's words), `ui.focus_id`, `ui.focus_index`, `ui.texts` (the top
    screen's texts) and `ui.all_texts` (every screen's). No report: everything.
  - `"view"`: `{"center": [x, y], "height": metres}` (straight on, as a 2D game is seen) or
    `{"eye", "target", "fov_y_deg"}`. A run's frame uses it, and so does the window while nobody has
    pointed it elsewhere: a 2D game is no longer seen by the camera that fits whatever is in the world.
- **Left and Right step a setting** on the button that moves it, and stop at its ends; Enter does the
  button's own action. Only while the press moved the focus nowhere.
- **`notes/how.md` is one page that answers what the builders looked up**: the loop and its command,
  every key of the game file, what a screen's text can show and a button's action can be, the SDK's
  calls a game's rules use. A test holds that it names every key the game file takes. The sample's game
  file has a report. `specula new` says the run command as what to do next, and a first call's
  `project_memory.how` says where the rest of a long note is.
- **What it is made of**: `crates/specula/src/script_run.rs` (a caller's thread, as the operator who
  started the engine, through the tools an agent would use), `launch_cli.rs` / `launch_run.rs`
  (`Cli::Run`), `specula-ludus` (`GameFile::view`, `report`), `shell.rs` (`step_focused_setting`, the
  view given to the window). The live check `new_project_check` has it as its sixteenth stage.
- **Found while building it:** a job owned by a session that is not a connection was dropped by the
  look for closed sessions (a run's frame, one run in four). The operator's own session counts as open
  now (`Engine::open_session_ids`), for a worker's job, a stepped job and an interface's state.

## 10. A run that says what went wrong, and a player that reacts (the third pass, 2026-11-17)

The second pass was measured (LEVERAGE.md, "Results (pass 2)") and every request of its builds laid
out again. Through the engine the small game cost 1.7 times the plain build (6.8 before) and the
medium one 2.6 times (4.0). What the medium builds still spent, by their ledgers:

- **A module whose tick failed, and a run that printed a state all the same.** A call that returns
  an error is undone, the whole of it. Two of three medium builds had a tick that failed every time
  (two things spawned under one name; a parent deleted by the module's own query), and `specula run`
  printed the state of a game whose rules did nothing. One builder spent thirty-four requests and the
  other fifteen putting prints of their own into the module to find one line the engine had all
  along (`logic_status`'s last error). It was the largest single cost of the pass: about 280,000 of
  one build's 673,000 weighted tokens, and about 115,000 of another's 389,000.
- **A player that reacts.** Whether the medium game can be won depends on aiming at walkers whose
  lanes come from the seed. A script of inputs written beforehand cannot aim. One builder wrote a
  small program that read the printed state and chose the next input, and started the run again from
  tick 0 for every decision: 347 engines, half an hour. The others worked a script out by hand from
  logs they added. The judge wrote a planner of its own for the same question.
- **Things looked up that the guide did not say:** how a key is spelt in a binding (three requests,
  one of them a search that printed 64 MB); what a function of the SDK returns (every build listed
  the SDK's `pub fn`s); why a query under a path gave the parent too.

So, in the engine:

- **A run whose module failed answers that, never a state.** `module_failed`: the module, how many
  of the run's ticks failed, the first failure whole (its code, its message, its tick), and the last
  one's tick; a module that panicked and was stopped answers the same, with `stopped` and where it
  panicked. The module host keeps a module's first failure beside its last (`logic_status`:
  `first_error`). Exit code 1. Only that module's call is undone: the clock, and any other module,
  went on.
- **Ticks are the script's.** The game's own clock stands at the title, in a pause and after a
  run's end, and a module's failure and its log lines carry the game's tick. The run keeps where
  the game's ticks fell among the script's and says each as the script counts it (`run_tick`,
  beside the game's `tick`): a failure "at tick 30" is at the input written `30:`.
- **What the rules log is said beside the state**: the last 20 lines of `cx.log` (of all the
  modules, by tick), each with its tick, on standard error, when the run ends.
- **A run's answer is one line, whatever it is.** A refusal used to be printed over several lines,
  and a reader that took the last line of a run read a brace.
- **`specula run --follow`.** After the script (which may be none) the state is printed; then each
  line read from standard input is a step, `TICKS` or `TICKS WORD` (`10 up+shoot`: that input, then
  ten ticks), and the state is printed after it; the run ends when the input does. A program that
  starts the command once, reads a state and writes a step plays a whole game in one run, by the
  same inputs a script gives. A line that is not a step is answered as one line (`bad_step`,
  `unknown_input`) and changes nothing, so the player always reads one line for each it wrote. At
  most 100,000 lines and 1,000,000 ticks a run, 200 characters a line (no more of a line is ever
  kept), and thirty minutes.
  - *How the lines travel.* The engine reads the command's own input. Its states go back on its
    standard error, each one write that begins with a mark byte, and the command passes them on
    as its output. Not on the engine's standard output: a module is code in the engine's process,
    and what it prints would land between the states (the review's finding; a module's own prints
    go nowhere, as in every headless run).
    A mark is a state's start wherever it comes in a line: a module that writes to standard error
    and does not end its line leaves words in front of a state, and they are kept as the engine's.
  - *A reader that goes away* ends the run: the command stops the engine and exits with 1.
- **The guide says what was looked up**: the sources most games bind (a test holds each
  against the input map), what each call of the SDK gives back, how things that come and go are
  drawn (a tag and no name; what a query under a path gives), what a failed tick means, and
  `--follow`.

- **A choice's value is words too.** `setting.NAME.text` showed a choice as the game file spells it
  (`hard`), in every language. A builder of the change round, asked for a difficulty setting in
  three languages, spent ten requests reaching "Difficile" with three labels and a condition each.
  It is now the line `setting.NAME.VALUE` of the player's language where the tables have one (the
  source language's where only that has it), and the value as it is where none does. The sample
  game's own difficulty has its button on the settings screen and its three lines, so the pattern
  is in what every builder reads first; and the guide says what a `"visible"` condition takes.

- **From the judges of the second pass, the engine's own:**
  - *A run that names no seed is the run with seed 1* (four of five engine games were marked for a
    run with no seed that was not the brief's default). A headless run is the same run every time.
  - *The game's name can be said in a language*: `game.title` is the game file's title, or the
    line `title.name` of the strings where they have one, in the player's language. One game failed "every word
    changes with the language" on a title that no table could translate.
  - *A game's menus show which button is chosen from their first frame* (`Ui::show_focus`; the
    toolkit's own default is to wait for a key, which suits a pointer). Three judges saw a title
    whose chosen Start looked like any other button.
- **A module that did not load says why as data**: a binding the input map does not read came back
  as a kilobyte of quoted JSON inside a refusal, and is now `load_failed` with its `why`.

And in the comparison's harness: its `play` takes the work folder as well as the run's, passes a
followed run's input and output through, its `tools` says so when no engine is running, and
`trace RUN --said A-B` prints what was done at those requests (each step whole and the start of its
answer), which until now took a script outside the repository.

**What it is made of**: `script_run.rs` (`Run::stand`, `Run::check_word`, `parse_step`, `say`),
`logic/mod.rs` (`first_error`), `launch_run.rs` (`usage_line`, `log_line`), `scaffold.rs` (the
guide, the sample's difficulty), `shell.rs` (`Shell::fill`: a choice's words). The live check's
sixteenth stage holds the run's part of it.

## 11. The game's own checks: every run in one command (the fourth pass, 2026-11-17)

The second pass's ledgers again, for the builds in which nothing went wrong. After its code was
written a small build ran its game in 5 or 6 requests, and a medium one in 15: start a run, read its
state, think, start the next. Each request reads the whole context again, by then 40,000 to 55,000
tokens, so each run costs 6,000 to 14,000 weighted tokens whatever it prints. That was a quarter of a
small build (about 50,000 of 178,372) and nearly half of the cheapest medium one (requests 8 to 22 of
`medium-engine-2`: 126,000 of 277,124). The builders without the engine checked theirs in 2 or 3
requests: their game is a program of their own, and they wrote its checks as a second one and ran
them together. And what a builder checked through the engine was left nowhere but in its README's
words: the next agent, and the judge, started again.

So a project keeps its checks as data, and one command plays them:

    specula check [NAME|FOLDER] [--only NAME] [--file FILE]

- **`checks/runs.json`** (`specula.checks.v1`) lists runs of the game. Each is what `specula run`
  takes (`ticks`, `inputs` as the command's script or a list of such, `seed`, `frame` and `size`),
  a `name`, and `expect`: keys of the game file's report (or a path into one, `walkers.0.health`)
  with the value each must have, or comparisons that must all hold (`ne`, `lt`, `lte`, `gt`, `gte`,
  `has`, `len`; two of them are a range). An object that mixes a comparison with another key is
  refused when the file is read, with the key (as a value to equal it would hold against nothing),
  and so is a key written twice.
  A value and `ne` are the same within a rounding (a state's numbers are sums of ticks); `lt`,
  `lte`, `gt` and `gte` take the number as it is. An unknown key of a
  check is refused by name, and so is a file with no check in it. **A key the state does not have
  never holds**, whatever is expected of it: a misspelt key that "is not 5" would be a check that
  cannot fail.
- **A line a check**: `{"check", "ok": true}`, or `"ok": false` with `differs` (for each key that
  did not hold, what it is and what was expected), the state, and what the rules logged; a run that
  could not be made (an input the game does not take, a module that failed) is that check's `error`.
  Then one line that counts them. Exit code 1 when one did not hold. A game that does not build is
  said once, and the rest are not played.
- **A check with no `expect` prints its state.** Runs to look at are written the same way as runs
  to hold: several at once, read in one request, and what each must be is written after.
- **Checks that name the same `settings` share one player's settings**, in the file's order, in a
  folder that is the command's own and goes with it. "Kept between runs" is two checks. A check
  that names none has a new player's settings, whatever folder the command itself was given
  (`--settings` says only where the list of projects is: a check never reads or writes a player's
  own settings). `--only NAME` plays that check alone, with nothing an earlier check set.
- **The file is a project's data, and may be someone else's.** It names ticks, inputs and
  expectations, never a command. The one file it has written is a frame: a PNG below the project's
  `checks` folder (no way up, no root, no drive, no second stream, no other kind of file, so no
  file of the game is written over), and where it is written no step of the path may be a link (a
  junction named `checks/out` would lead anywhere) and a frame written before is removed first,
  never written into (it may be a second name of a file elsewhere). The review's findings: the
  first version held only the path's spelling.
- **Bounds**: 200 checks and 1 MiB a file; what a line shows of a value that did not hold is its
  first 400 characters, and of a state its first 4,000; the command starts no new run after an
  hour, and stops at a run whose engine did not answer.
- Each check is a run with an engine of its own, as `specula run` is (about four seconds each once
  the module is built): a check starts from the game's start, with nothing of the check before it
  but the settings it names.
- **`specula new` writes the sample's own** (seven checks: the title, a win, a loss, a pause, a
  setting changed, kept, and not there for a new player), and the guide says how one is written.
  `checks/smoke.json` stays what it was: a playtest for an engine left running.

**What it is made of**: `crates/specula/src/run_checks.rs` (the file, and a state held against what
a check expects), `launch_run.rs` (`check_game`; `run_in`, which a run and a check share),
`launch_cli.rs` (`Cli::Check`), `scaffold.rs` (the sample's checks, the guide). The comparison's
harness gives the engine arm `examen leverage check RUN`. The live check's sixteenth stage plays
the sample's checks, and one that does not hold.

**What its measurement said** (2026-11-17; [LEVERAGE.md](LEVERAGE.md), "Results (2026-11-17: the
fourth pass)"). Every engine build wrote a checks file and ran it, and none in place of single
runs: each played its game a run a request first, as before, and wrote the checks from what it had
seen. So a first build got no cheaper, and a one-screen game got dearer by the checks it did not
need. The change builds, which begin from a game that has checks, ran them after their edits and
added their own. What the command is for is therefore the second visit to a game, not the first
hour of it. One defect was found by a builder and fixed (`e8097c2a`, `648cb757`): two comparisons of one
key (a range) were read as a value to equal and never held.

## 12. What a first build is not asked for (the fifth pass, 2026-11-17)

The fourth pass's ledgers put the engine arm's extra cost in one place, at all three sizes: more
requests, each reading a larger context again ([LEVERAGE.md](LEVERAGE.md), "Results (2026-11-17:
the fourth pass)"). Not more code: on the medium game the engine arm writes a quarter less than
the plain one. And not more thinking there: about 12,000 tokens in both arms. So this pass adds
nothing to the engine. It takes away what made requests.

- **The guide no longer asks for work the game was not asked for.** Checks kept with a game are
  described after the rules, as what they are for (a game someone comes back to, or hands on),
  and "write the checks as you write the game" is gone: every engine build of the fourth pass
  wrote and corrected a checks file on top of its single runs (26,464 and 27,126 weighted tokens
  in the two small builds). `--follow` is said as what it does, without proposing a player to
  write. **The sample's checks go with the sample**: the guide's first lines and the brief say
  to delete them or make them the game's (a game that never touched `checks/` would hand on
  seven checks of a tap game, and the next builder would run them: the review's finding).
- **First contact is one read.** The guide's first lines name what is read with it (`game.json`
  and the module's `src/lib.rs`), and `specula new` answers the same three in `read_first`. Every
  engine build had read the guide in one request and the sample in the next.
- **What was still looked up is where it is looked for.** A setting the rules read, as its four
  places (all three change builds of the fourth pass opened this document for it, though the
  guide said it in half a sentence); where a player's settings are kept (looked up for a README);
  that the rules never see a seed.

**In the comparison's harness** (`tools/examen/src/leverage.rs`, `evals/leverage/`):

- The engine arm's brief gives the one read as a command, and says `check`, an engine left
  running and its tools in one line, as there and not needed (they were three paragraphs).
- **A change job's brief no longer calls the game it hands over "a small sample game" whose
  rules are to be replaced.** It had since the change job was first run: the engine arm's
  paragraph was one text for every job.
- **Both arms are told to write files with their file tool and never with a shell
  here-document.** Every build of every pass, in both arms, had its first here-document refused
  by its shell and wrote its first file twice. That falls harder on the plain arm, whose first
  file is the whole game: by the ledgers about 47,000 to 59,000 weighted tokens of a plain medium
  build against 31,000 to 47,000 of an engine one. So the measured ratios flattered the engine:
  with that waste taken out the third pass's medium game is about 1.1 times the plain build (1.0
  measured) and its small game about 1.7 (1.5); the fourth pass's, about 1.26 and 2.7. These are
  estimates from the ledgers; the fifth measurement is the first without the fault.
- A `large` job is written as data (`evals/leverage/jobs/large.json`: about three times the
  medium game; three kinds of enemy, a shop, a saved run, a best score, three languages; 31
  checks). **Not measured yet**, and it asks for something a game made of modules cannot do
  today: keep anything for the player between runs but its settings (a best score, a saved run).
  A save is a button's word and goes to the project's `saves` folder; the rules cannot ask for
  one, and nothing else is kept. That is the engine's to build before the job is run.

**What it is made of**: `scaffold.rs` (the guide, the sample checks' note), `launch_run.rs`
(`read_first`), the harness's brief and its data. Nothing here changes what the engine does.

**What its measurement said** (2026-11-17; [LEVERAGE.md](LEVERAGE.md), "Results (2026-11-17: the
fifth pass)"). In a clean shell: 1.85 times the plain build on the small game, 1.28 on the medium
one, 1.25 on the change. The one read was taken as written by every build, and a second followed
all the same (the sample's screens: the guide names the widgets and shows no screen); every build
still wrote a checks file and ran it once or twice. An engine build has spent 49,000 to 74,000
weighted tokens when it writes its first file and a plain build about 6,000; after it the engine
arm is the cheaper one on the medium game. Still looked up, a third measurement running: whether
a run that starts again gets the same random numbers, and what the built-in volume is.

## 13. The first meeting: one read, and a run that starts again (the sixth pass, 2026-11-17)

The fifth measurement, the first in a clean shell, put the whole of what the engine arm costs more
in one place ([LEVERAGE.md](LEVERAGE.md), "Results (2026-11-17: the fifth pass)"): an engine build
has spent 49,000 to 74,000 weighted tokens when it writes its first file, a plain build about
6,000, and after the first file the engine arm is the cheaper one on the medium game. Before the
first file there are four requests: the brief; the guide with the game file and the rules; the rest
of the sample (its six screens, the look, the words, the inputs, the checks); and one of looking
something up. The owner, shown it: "That gap needs to be looked at and fixed up."

- **One read that is enough.** The guide's first lines name six files to read with it (the game
  file, the rules, the inputs, one screen, the screen that moves settings, the words) and say that
  is the reading done: the other screens have the same form, the look needs no reading, and every
  call of the SDK a game needs is on the page. `specula new` answers the same list, and the
  comparison's brief gives it as one command. The fifth pass had named three files, and every
  build then read the rest of the sample in a request of its own, its 1,450 tokens of look
  included.
- **A random stream begins again when asked**: `cx.random_restart("stream")` (the request
  `rng_restart`; [MODULES.md](MODULES.md)). The one-screen job asks that a restart begin "a fresh
  run with the same seed", the SDK had no way to say it, and seven of the eight
  one-screen builds of the last four measurements wrote a generator of their own in their rules
  (and looked for the call in the engine's documents first); the eighth kept a list of the
  columns it had drawn and played it back. It is the engine's own gap, found by reading what the builders looked up three
  measurements running; the guide says what a stream does when a run starts again.
- **What else was looked up three times is said**: the built-in volume is an int from 0 to 10, 8
  at first, and how a game gives it another range.
- **Checks are not in the first read.** Their description is a note of its own
  (`notes/checks.md`); the guide keeps four lines under "Also", and its first lines say to delete
  the sample's `checks` folder (they said "or make them your game's", and every build of the
  fifth pass made them its own and ran them).
- **The sample's rules file says what it is in four lines** (its header repeated the guide).

**What it is made of**: `logic/handler.rs` (`rng_restart`, undone with a call that fails),
`specula-sdk` (`Cx::random_restart`), `scaffold.rs` (the guide, `notes/checks.md`, the sample's
header), `launch_run.rs` (`read_first`), the harness's brief.

## 14. What physics will do, and a run that is asked many things at once (2026-11-21)

The six-job comparison of 2026-11-21 (LEVERAGE.md) was read a second time, by Grok, from the same
ledgers and the code. The two readings agree on where the tokens go: the engine's builders make two
to three times the requests, most of them after the first code and short, each paid for by the
context read again. They go to finding out what the pages did not say or said wrong, to a wrapper
between the brief's words and the engine's, and to reading one number a run. What was built for it:

- **The pages say what physics will do, and the guide says to write the game whole.** `notes/world.md`
  was wrong on two points (bounce takes the larger of the two things that meet, friction the
  geometric mean; it said "on both") and silent on what the builders measured: the default friction,
  that a rolling ball takes 5/7 of a push, when a thing sleeps, that the rules run before physics in
  a tick (what they keep is a step behind), where `fell` fires, where the volume applies. The guide
  opened with "The loop: edit, run, read"; it opens with "Write the game whole, then run it".
- **A slab that carries** (`Spawn::carries`, PHYSICS.md). A ball at rest on a sliding bridge is left
  behind by honest contact, and a brief that says "it carries the ball" could be met only by a rule
  of the builder's over the engine's friction, which is what a judge then saw as twice the speed.
- **A run in the brief's words.** The game file's `"inputs"` gives a script words of its own
  (`"menu_down": "Key.Down"`). A report's line reads the world itself, after the tick's physics:
  `world.PATH.position`, `.x`, `.velocity`, `.exists`, `world.#TAG.positions`, `.count`; and
  `run.sounds` is every sound the rules asked for, in order. No rule copies places into its state
  for the report, and nothing is a step behind.
- **A run read on its way.** `--at 90,180,270` prints the state at each of those ticks in one run,
  and `--keys` only the lines asked for. The state at the end is the same with or without it
  (tried by hand: bit for bit).

Not built in this pass: results handed to a builder without a request (hooks). The searches the
engine makes itself are section 15.

**What it is made of**: `specula-physics` (`space.rs`, `carry`; the snapshot's version 4),
`physics.rs` (`Collider::carries`), `specula-sdk` (`Spawn::carries`), `scaffold.rs` (the two pages),
`specula-ludus` (`GameFile::inputs`), `script_run.rs` (`in_game_words`, `of_world`, `--at`, `--keys`),
`module_sounds.rs` (the order of what was asked for), `launch_cli.rs`.

## 15. What the engine looks for itself (2026-11-21)

The owner, on the two kinds of search: "Then we'll do both. And each use must be determined by you,
or your builders, depending on a task." Three commands, none of which a game needs to be built, and
no program of a builder's between them and the game. In the comparison both arms wrote such
programs: the engine's builders a player over `--follow` (nine requests and about 135,000 weighted
tokens in one run of the 3D game) and a tracer of their own; the plain builders a `bot.py` and a
`search.py` over their own simulation.

- **A number of the rules', given by a run.** `cx.param("grip", 0.6)?` reads a number a headless
  run may set (`specula run --set grip=0.3`) and answers the rules' own default when it sets none: a
  player's game has none set and plays as written. The numbers are the run's, as its script and
  its seed are; a module cannot change them, reading one changes nothing, and a save does not keep
  them (a search goes back to saves within one run, where they are the same). `--set grip=0.2,0.4,0.8`
  plays the run once for each value and prints a line each (`{"set": .., "state": ..}`).
- **`specula tune`**: `--vary grip=0.1..1 --until "ball.x>3"` finds where a condition over the
  report's lines begins to hold at the run's end, by halving the range. Every try is a whole run
  of its own from the game's start (about four seconds here), so a value found is one a plain run
  plays again. It needs the condition to hold at one end and not at the other, and says so when it
  holds at both or at neither; a condition that comes and goes inside the range is not one a
  halving finds.
- **`specula find`**: `--until "won==true" --words right,left,up,down,none --rise score` finds a
  script of inputs that reaches a condition. It keeps the situations it has reached, each as a save
  of the whole game with the script that led there, told apart by a cell (the report's lines that
  say where the game is, rounded: `--cells`); it goes back to one (three times in four to one that
  stands at the most of what `--rise` names; among those, to one tried less), plays on a few
  stretches with an input held through each, and keeps what is new or a better way to a cell it
  knows. `--never` names what must not happen on the way (a life lost, a fall): such a situation
  is not kept. This is the first phase of Go-Explore (Ecoffet and others, "Go-Explore: a New
  Approach for Hard-Exploration Problems", 2019). The search's choices come from the run's seed:
  the same command makes the same tries in the same order (a search ended by its seconds
  got as far as the machine went: its `why` says so, and only then does the answer depend
  on the machine).

**What it rests on, and how that is held.** A game put back from a save must play on as the game
that was saved: the world, the modules' state and random streams, physics' hidden state, the
project shell. So a script found is played again from the search's first situation, straight
through, before it is answered, and one that no longer reaches the condition is answered as the
fault it is (`not_reproduced`). And `search_check` (a live check, in Rust) takes the script a
search answered, plays it in a run of its own, and holds the state against the search's: equal to
the last bit, on a game with rigid bodies. The same check holds the sweep, the tuning, a range the
condition holds nowhere in, and that the same search finds the same script.

**What it cost to find, by hand** (a ball, a board 8 m square, three coins up to 5.4 m apart): the
script was found after 3,295 stretches of 15 ticks, in 29 seconds, at about 10 ms a stretch. Two
things had to be mended on the way. With no word for what is worthless, a ball that had left the
board fell for ever through cells that were all new, and filled what the search may keep: hence
`--never`, and the worst kept situation gives way when the search is full. And picking among all
situations alike never played on from the best: hence the three picks in four.

**What it does not do.** It searches by what the report prints, so a game whose report does not
say where it stands gives it nothing to tell situations apart by. A sound asked for is no part of
a save: the line `run.sounds` is left out of the state a search answers (a run of the script says
it). It gives one input a stretch; a game that needs two things held at once takes a word that
joins them (`up+shoot`) among `--words`. A press of the interface in `--words` is given again each
stretch.

**Found while trying it cold.** A module that built with a warning and did not load was answered
as "did not build", with no error to read (`specula run` read the warnings as the failure); it is
answered as the load's failure now, with the reason. The guide gave an action's fields and not the
file's own form (`{"format": "specula.input.v1", "actions": [..]}`), and an actions file written
from the page alone was an array; the page says the form now.

**What it is made of**: `script_run/find.rs` (the search), `script_run.rs` (`Until`, the spec's
`set` and `find`), `launch_run.rs` (`tune_game`, a run for each value), `logic` (`set_params`, the
request `param`), `specula-sdk` (`Cx::param`), `launch_cli.rs`, `scaffold.rs` (the guide's "When
the engine should look for you"), `tools/examen/src/native/search.rs` (`search_check`).

## 16. What a second reader's bug list found (2026-11-22)

A second reader went through the code and the 24 ledgers of the second comparison and listed
thirteen faults ([LEVERAGE.md](LEVERAGE.md) has the comparison; the list was a report to the
owner, not a file of this repository). What was mended from it, by hand, the same day:

- **A damaged save could hang the engine.** A physics snapshot's settings were read with no
  range check, and the substep count is a loop's length: one byte wrong, and the next step
  ran for hours. They are checked as a game file's are (`bad_snapshot`), a step holds the
  count to `MAX_SUBSTEPS` (64), and a test changes a few bytes of a save 4,000 ways: each is
  refused, or restored and stepped.
- **A number no rule reads was set in silence.** `--set bord=3` played the game as written. A
  run now says so among its problems ("the run set 'bord' and no rule read it in this run"),
  a check says it on its own line, and `specula tune` refuses to vary one
  (`unread_parameter`): every try would be the same run.
- **A check could not give its run a number.** `"set": {"board": 3}` is a key of a check now,
  as `--set` is a flag of a run.
- **A loop over the tick's events could not change the world.** `cx.events()` lends the list;
  `cx.take_events()` hands it over, and the sample and the guide use it.
- **"Silent at volume 0" could be shown by no run**, a headless run playing nothing.
  `run.sounds_heard` is each sound asked for with how loud: its own gain times the player's
  volume as the game stood.
- **The sounds a run reports were no part of a save** (found by `search_check` once its game
  made a sound, not by the reader): a script a search found was answered with the sounds of
  every stretch the search had tried. The list is a section of the save (`sounds`, there
  only when a sound was asked for).
- **What two pages did not say**: every public word of the SDK is on the guide or the world
  page (twelve were on neither, and four of six builders of the 3D jobs searched the SDK's
  source), and the world page lists a sound recipe's fields. A test holds both against the
  code. The world page is twelve half-kilobytes for it.
- **Said, not mended:** a search that ends by its seconds answers by how far this machine got
  (the page said "the same command finds the same script"; it says when that holds). The
  guard against tests that pass by the machine's speed reads upper bounds only: what it
  cannot see is written at its head.
- **Left as they are:** the sample's screens keep their names (`hud`, `pause`; the report's
  `phase` line says `paused`); three lookups that panic on a name not in the engine's own
  tables; the engine on Linux ([LINUX.md](LINUX.md): the second reader builds that side).

`search_check` plays the three that need a game: the sound's loudness at the tick a coin is
taken, a tuning refused for a number no rule reads, and three checks that give their runs a
number, one of them mistyped.

## 17. What the large job ran out of (2026-11-22, after the cold comparison)

The cold comparison's one larger job cost 5.9 times a plain build
([LEVERAGE.md](LEVERAGE.md), the last results). Its three builders spent 99 requests reading
the engine's documents and source, and a second reader's three passes over the ledgers put
about four fifths of that on two things the engine could not do or did not say, tied: what a
game keeps between two runs of the program, and screens of the game's own. The same passes
found two smaller traps that cost checks. Built for it, by hand:

- **What a game keeps for the player** (`"keep": {"best": "gate.best"}` in the game file: a
  name, and a key of a module's state). The engine writes the value when the rules change
  it, in the player's settings file under `kept.NAME` (so only where settings are kept), and
  tells it back to the rules as the event `kept` before the first tick and after a save is
  loaded. Any value the state holds, 4 KiB as JSON, sixteen names. Nothing of the sim reads
  a file: what was kept comes in as every other thing from outside does, as an event at a
  tick's boundary.
- **Screens the rules put up** (`"show": {"shop": "gate.shopping"}`): while a run is under
  way or paused and the key is true, the screen is up over the game's own and under any
  menu, with the focus. Its buttons' words reach the rules as `ui` events; the clock goes
  on; the rules take it down by the key. Escape still pauses over it.
- **A key is read by itself.** `--keys`, a check's `expect`, and a condition of `find` or
  `tune` take any key the report could name (`game.phase`, `run.sounds_heard`,
  `world./gate/door.position`, `gate.wave`) with no line of the report written for it first.
  A key with no value is left out, so a name mistyped is still refused by name. (Four of
  five games failed "silent at volume 0" because the line that shows it was in no report;
  twelve searches were refused over `game.phase`.)
- **`ui.texts` is the top screen's texts** (it was every screen's, and five games printed
  the title's buttons under a pause); `ui.all_texts` is every screen's. **`ui.focus` is the
  focused button's words**, `ui.focus_id` its id.
- **The guide says what the engine does not do**, five lines: no file from the rules, no
  setting changed by them, no screen opened by a call, Escape not theirs to hear, no tick
  at the title.

`search_check` plays the kept score (set in one run, found in the next in the same place,
not found elsewhere) and the keys read by themselves. A shown screen is held by a unit test
of the shell and was played by hand; no live check plays one yet.

**Not done here, and next:** a smaller project to start from and an index of the SDK's
signatures (builders still opened its source for types); one command that plays every check
of a brief. Both are about the number of requests, which these mends do not touch.

### What a second reader found in it (2026-11-23), and what was mended

The second reader applied this section's change to a clone, built and tested it on Linux, and
read the five runs' ledgers again by a rule of its own. Its findings, with what was done:

- **A file that was there and could not be read was written over at the first change**
  (too large, not UTF-8, damaged JSON, another format): defaults replaced it. Before this
  section that cost a player their settings; with `keep` it costs their saved run.
  **Mended:** nothing is saved over a file that was not read (`SettingsStore::saving`), a
  text that is no settings file is told apart and said, and the engine's own writer holds
  to the same.
- **Sixteen kept values at their most made a file its own reader refuses.** **Mended:** what
  a game keeps is 48 KiB at most in all; a value that would pass it is not kept, and said.
- **A kept name of 60 to 128 characters was written and gone at the next start** (the
  file's reader takes names of 64, and `kept.` stands before it). **Mended:** a kept name is
  48 characters at most, refused in the game file by that number.
- **Two engines on one settings folder shared one temporary file.** **Mended:** each writes
  a temporary file of its own. **Not mended:** each still writes its whole view of the file,
  so of two that change different things the later write wins. Runs side by side on one
  folder are for reading, not for keeping.
- **A kept value from the file was told whatever its size.** **Mended:** one larger than a
  game may keep is not told, and said. Its kind is still not checked: a file changed by
  hand tells the rules what it holds.
- **After a load a telling the save held was told in place of the player's file's.**
  **Mended:** the file's value takes its place; what was last seen of the modules is
  forgotten at a load.
- **`--keys` dropped a key with a dot in it that had no value, so one mistyped vanished.**
  **Mended:** a key `--keys` names is always printed, `null` when it has no value (every
  state of `--at` has the same lines). A key only a condition reads is still left out when
  it has none, and the condition says it could not be read. And a key of a module named as
  a line of the report (`--keys gate.wave` beside a line `gate`) is read.
- **Escape over a screen the rules put up paused the game** (four of five games paused in
  their shop, against their brief). **Mended:** over such a screen Escape is told to the
  rules (`ui`, action `back`, with `screen`) and does nothing else.
- **The guide said "Escape is not the rules' to hear"**, which a working game of the five
  contradicted. **Mended**, with what `keep` left unsaid (the rules only set the state; the
  telling comes at the first tick, before `run`), the kinds a state's value is read as, and
  the case every builder went to the source for: a title or a menu of the game's own is
  `"start": "run"`, a shown screen, and rules that answer its buttons.

**Left, and said:** `ui.focus` takes the first text inside the button's rectangle and falls
back to the id; `ui.texts` changed its meaning, which a check written against the old one
will show; a shown screen is drawn by its own `layer`, whatever order the shell has it in
(the guide now says which layer to give it).

### After the large job was measured again (2026-11-23): the engine's words on the rules' own screens

The measurement of `ff4b1298` ([LEVERAGE.md](LEVERAGE.md), the results of 2026-11-23) and the
second reader's two reviews of it found two more things, both small, mended before the engine
and its guide were frozen for a second large job:

- **A button that says `resume` on a pause screen of the game's own did nothing** (five of
  fifteen games failed the pause item so; a game whose pause was the engine's own screen
  passed with the same word). `resume` was answered by the engine, which had paused nothing,
  and the rules never heard it. On a screen the rules put up, the words that move the run
  and the screens (`start`, `pause`, `resume`, `title`, `back`, `close`) now reach the rules
  as the event `ui`, as any word of the game's own does, and the engine does nothing. What
  serves any menu is still the engine's there (`open:SCREEN`, the settings' words, `save:`,
  `load:`, `quit`), and on its own screens every word is the engine's as before.
- **What was kept reached a screen a tick late**: the rules hear `kept` at the first tick,
  so a title at the very start showed no Continue and "Best: 0" (four of fifteen games; no
  item of the list sees it). A screen now reads what the game keeps as `kept.NAME`, from the
  player's file, from the first frame.

The guide says both, and which words are whose. **From this commit the engine and the guide
are frozen** until the second large job's runs are judged: the job is written by the second
reader, who did not write the guide, and the lead does not see what in it the guide does not
answer. Typos excepted, and recorded before the runs.

### A word given again is pressed again (2026-11-23, after the second large job)

The second large job ([LEVERAGE.md](LEVERAGE.md), the results of 2026-11-23, Night Market)
asks that every input word act once at its tick, "even when the same word was given at the
tick before". A script let go of what was held and held the new word between two ticks, and
a module read an action as pressed only when it had not been held the tick before: so the
same word twice running was one press. Three of five games failed the items that turn on it;
two got past it with a hidden button and a chord for every word. The second reader named
three faults of input. Looked at in the runs, they are one:

- **Mended:** an action let go and held again between two ticks is pressed again
  (`logic/input.rs`: a hold placed where none stood is a press, whatever the tick before
  held). A hold made longer while it stands is no press, as before. `5:tap,6:tap` is two
  presses.
- **A side of it the live check found:** asking whether a word is one the game takes let go
  of what the word named and held it again, which was then a press. A word that is only
  looked at, or refused, must change nothing: what is held at that value is not asked again
  (`script_run::check_word`). `new_project_check` holds both: a refused step adds no press,
  and the same word given again adds one.
- **"A key reaches the rules only through a button" was this fault**, by the two builders'
  own READMEs: the hidden buttons were there to get a second press and for nothing else.
- **"The rules cannot change a setting": no fault found.** The builder who spent thirteen
  requests on a settings screen of its own got it working with the engine's stepping
  buttons; the cost was finding that out.

### A screenful of text in one kept value (2026-11-23, after Day Market)

Five of ten cold builders' Day Market games put a whole end-of-day summary (about sixteen
lines, 300 to 600 characters, joined by newlines) into one kept text bound by one label. The
shell gave a screen no text longer than 256 characters and dropped a longer one without a
word; the label showed `{work.disp}` as written, and the run's warning said that nothing kept
`work.disp`, which sent the builder to look for a `cx.set_state` that was there. No page gave
the limit, and nothing recorded a reason for it.

- **The bound is a label's own now**: a kept text of up to 4,096 bytes reaches a screen whole
  (`specula_ui::state::MAX_TEXT`, raised from 512 bytes, which a summary of that size would
  have been cut at). What it bounds is a label's cost a frame (rendered, compared with the
  last layout, one glyph quad a character); what the modules keep is bounded by their store.
- **A value that is still not shown is said, with why**: `game_state` lists it under
  `not_shown` (its key, and a text's length against the bound, or a kind a screen cannot
  show, with what to do), and a run's problem for a screen that binds it says so. "Nothing
  keeps" is said only of a key no module keeps.
- **A run's `ui.texts` holds a text whole** (`ui_tree`'s `text_chars`, up to a label's
  bound; a page's long texts are at most 64 KiB together). `ui_tree` still cuts at 160
  characters unless asked.

## As built

**Built, with its own tests, the structural guards and clippy passing; the full gate passed on its last commit** (2026-11-16, branch
`m6-first-hour`):

| Piece | Where |
|---|---|
| The SDK's sources carried by the engine; `write_into`, `differs`, `is_the_engines` | `crates/specula-logic/src/sdk_source.rs` |
| Trust by content; `sdk_differs` refused before Cargo runs | `specula_logic::build::{trusted_manifests, sdk_differs}`, `vet` |
| `specula sdk [NAME|FOLDER]` | `launch_cli.rs` (`Cli::Sdk`), `launch_run.rs` (`write_sdk`) |
| The link's path length known before a build; the short folder | `build::link_arguments_chars`; `logic/tools.rs` (`pick_target`, `short_targets_root`, `build_dir` in the answer) |
| The game file, conditions, settings, actions, the run's state machine | `crates/specula-ludus` (`game`, `cond`, `settings`, `action`, `run`) |
| Notes, superseding, finding, the brief | `crates/specula-memoria` |
| An event from the engine to the modules; a view of what modules keep | `LogicHost::{push_engine_event, state_of, states}` |
| The hosted game's new hooks: `after_modules`, `see_modules`, `take_module_events`, `take_clock`, `clock_seen`, `game_state`, `game_file_changed` | `crates/specula/src/game.rs`; `sim_writes::GameCall::{AfterModules, GameFile}` |
| The project shell, hosted for a project with a `game.json` and no executable of its own | `crates/specula/src/shell.rs`; `run_with` in `lib.rs` |
| Its built-in files, which are also what `specula new` will write | `crates/specula/src/shell/new/assets/` (a theme, six screens, the English words) |
| `game_state` (family `game`) | `tools.rs`, `Engine::game_state` |
| The project-root mount: `game.json` (kind `game`) and `notes/<id>.md` (kind `note`) as project files | `project.rs` (`MountKind::Root`, `ROOT_MOUNT`), `project_engine.rs` (`project_mount_root`) |
| The shell follows a changed game file while it runs | `Shell::reload_game`; `project_reload` hands it the store's text |
| `memory_note`, `memory_get`, `memory_list`, `memory_why` (family `memory`); the brief in `agent_hello` (`project_memory`) | `crates/specula/src/memory_tools.rs`; `tools.rs` (after the hello) |
| `game_act` (family `game`): a button's word pressed by an agent, answered with `game_state` | `tools.rs`, `Engine::game_act`, `Game::act` |
| `--run` and `specula play NAME`: the game file's modules built and loaded at start | `crates/specula/src/run_modules.rs`; `Swarm::admit_run_operator`; `launch_cli.rs` |
| `specula new` writes a project that builds and plays; `--bare` what it wrote before | `crates/specula/src/scaffold.rs`; `launch_run.rs`; the start screen's new project |

As built, where the text above said less or something else:

- **`setting.<name>.text`** is the value as words: a language by its own name, a toggle as `on` or
  `off`, anything else as it is. **`game.*`** holds `title`, `phase`, `outcome`, `screen`, `paused`,
  `running`, `runs`, `has_score` and `score`.
- **`ui_scale` and `text_size` are meanings, not built-in settings**: a game that wants them declares
  an `int` setting with `"is": "ui_scale"` or `"is": "text_size"`, in per cent (100 is as designed;
  50 to 300 is taken). `language` and `volume` are always there.
- **The event `ui`** carries `action`, and `widget` and `screen` when the press came from a screen.
- **A changed game file** takes effect at once for the title, the screens' parts, the outcome and the
  settings (the player's choices are kept where a setting still exists); a new `tick_hz` is used from
  the next start, and `game_state` says so under `problems`. A file that is not a game file changes
  nothing and is named there too.
- **A project has one brief**: a second is refused with the first one's id. A note is replaced by one
  note at most, and never in a ring. `memory_why` puts what was decided about a path first, and the
  brief and the how-to last, by title.
- **`game_act`** was not in the design: an agent starts, pauses and plays a run by word, in one call,
  where it would otherwise drive the interface step by step. `quit` stays the player's alone.
- **A playtest owns the clock.** While one runs, the game's own wish for the clock (a run begins, a run
  ends) changes nothing: the playtest's steps tick the run, and its ticks go on past the run's end.
- **Events wait for a module.** What the game has for its modules stays with the game until a module is
  loaded (at most 256): a run started while `--run` is still building is told to the module when it loads.
- **A screen opened over another must have a higher `layer`**; one that has not is drawn underneath, and
  `game_state` says which under `problems`.
- **A new project has no `assets/input.json`**: the menus' actions are the interface's own, and a game's
  input is its module's `actions.json`. Its module is named from the project (`modules/<name>`).
- **A shell command that cannot keep its agent's identity says so** (`identity_not_kept`): `specula call
  --as NAME` keeps a resume token beside the engine's instance file, which is only there when the
  command reads the settings folder the engine was started with.

- **`--run` runs what an owner or a granted lead last built, and nothing else** (reviews R1 and R2;
  [MODULES.md](MODULES.md), `crates/specula/src/approved.rs`). A build that passed the gate and loaded
  leaves, in the operator's settings folder, the digest of the module's folder and of the project
  root's build configuration (`Cargo.toml`, a toolchain file, `.cargo/`); `--run` loads a module only
  while both hold, and loads nothing of a module that compiles a crate from outside its own folder.
  `--run-as-is` (`specula play NAME --as-is`) is the operator saying that what is on disk now is
  theirs to run; it approves nothing for later starts. What `--run` loads is built in a folder of the
  operator's own, never in a build folder that could have come with the project (review R3).
- **An agent's `game_act` never saves or loads** (`use_save_tools`): the save tools do, with their
  rules. **An agent's change to a setting lasts for the session.** The player's settings file holds
  the choices the player made and nothing else: not a default, and not a value a changed game file
  put in a choice's place (a choice the game no longer takes stays the player's, unused until it fits
  again). **A refused `quit` is told to the input monitor**, and `game_act` is refused while one of
  the engine's own screens (the review screen, a gallery) is open.
- **A game file that is not one is never stored**, `allow_errors` or not: the project would not open.
- **No note is hidden, whatever the files say** (`specula_memoria::Notes::read`): a replacement that
  would close a ring is not followed, and `memory_list` says so under `problems` (with a second
  brief, of which the first by id is given). The check at the write (`project_engine::note_guard`:
  one replacement a note, no rings, one brief, for `memory_note` and `project_write` alike) refuses
  early; an undo and an edit on disk pass it by design, and the reader holds. A note's front matter
  is at most 16 lines. Only notes count toward the 1,024 that are read: files in the folder that are
  not notes take no note's place, however many (review R3), and `memory_note {id, supersedes: ""}`
  takes a line out.
- **`project_memory` is the project's text, and says so**: `source` (whose words these are), `brief`
  and `how` each with `id`, `by`, `version` and `text`, and `also` (ids, kinds and titles). Nothing of
  a note is joined to the engine's own words, and no agent takes a name that reads as the engine's
  or its operator's (`operator`, `engine`, `specula`, `system`, `admin`).
- **The clock after a playtest is as the run stands**: the game's last wish for the clock made while a
  playtest had it is done when the playtest gives the clock back.
- **`time_control {step}` says what became of every tick**: `ran` in this call, `queued` (one call
  runs a bounded number, the frames after it the rest), and `not_run` with why when the game stopped
  the clock on one of them.
- **Only running modules' state is read** by a screen or a condition: a module that was unloaded
  leaves nothing behind for them. What is unchanged is compared in place, not copied, each tick.
- **The shell reads nothing from the disk after its start**: a changed screen, theme or string table
  reaches it as the project store's text (`Game::asset_texts`), taken by the rule the disk is read
  by (each folder's files by name, and it is said when there are more than are taken).
- **The sample module handles its events in the order they happened**: a tap that comes with the
  start counts, and a run started and given up in one tick is given up.

**Found by the first live run of a new project** (2026-11-16; each fixed, with a test that holds it):
the game's start un-paused a clock a playtest was stepping; the settings screen opened under the title
(its layer was lower); a playtest spec took no `notes` key; a second command as a granted lead was
refused with nothing to say why.

**What closed the build:**

1. **A live check in Rust**, `new_project_check` (`tools/examen/src/native.rs`), the first check
   written in Rust: a new project is made, the brief read at `agent_hello`, its module built, its own
   playtest run, the game file changed and followed on a clock stepped by hand, the settings found over
   the title with the focus, a note written, a second language chosen, and the engine started again with
   `--run`: the module loads, the language and the difficulty were kept, the next agent is told the note,
   and a builder's own build is still only a proposal. Since review R1 it also holds: a save of a run
   loads again, `game_act` refuses the save words, a game file that is not one is refused, a builder's
   change to the module's source keeps `--run` from loading it (the game says why), and `--run-as-is`
   loads it. Since review R2: a step of 1,000 ticks is said as run and queued; a run begun in a
   playtest goes on after it; a save read at the title brings its run back; two notes written on
   disk to replace each other hide neither; one `--run-as-is` start approves nothing; a lead's build
   is what `--run` then loads; a changed `.cargo/` at the root, and a crate from outside the
   module's folder, keep `--run` from loading. About 55 s (`examen live --only new_project_check`).
2. **The comparison's harness is Rust** (`examen leverage`, `tools/examen/src/leverage.rs`) and
   starts its engine arm from `specula new`. The comparison was run again with it (slice 8).
3. **Not built, and said:** conditions over the world (`world:/path.prop`); a condition reads what the
   modules keep, the settings and the run's own state.

## What this does not do

- It does not make a module's first build instant: Cargo still compiles the SDK once a project.
- It does not move Sepulcra onto the shell.
- It does not sandbox a module: who may load native code is unchanged ([MODULES.md](MODULES.md)).
