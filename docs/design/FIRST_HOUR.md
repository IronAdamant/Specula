# Design: the first hour (M6, part 3)

Status: **design, written 2026-11-16; built in slices, each ticked here when it is gated and reviewed.**
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
| 1 | The SDK comes from the engine, and is trusted by what it says | in progress |
| 2 | A module builds in a folder of any depth | |
| 3 | The game file (`game.json`): tick rate, modules, screens, settings, outcome | |
| 4 | The project shell: screens, strings, saved settings and pause for a module-built game | |
| 5 | Outcome and score as data; `game_state`; playtests and the balance lab read them | |
| 6 | What a project remembers: notes kept with the project, and the brief an agent is given at its first call | |
| 7 | `specula new` makes a project that builds and plays; the guide it writes | |
| 8 | The comparison, run again with the same harness | |

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

## Where the build stands (kept current while part 3 is built; removed when it is done)

**Built, with its own tests, the structural guards and clippy passing; the full gate runs on each commit** (2026-11-16, branch
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

**Still to build, in this order:**

1. **Done: a live check in Rust**, `new_project_check` (`tools/examen/src/native.rs`), the first check
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
2. **Done: the comparison's harness is Rust** (`examen leverage`, `tools/examen/src/leverage.rs`) and
   starts its engine arm from `specula new`. **To do: the comparison is run again** (slice 8).
3. **Not built, and said:** conditions over the world (`world:/path.prop`); a condition reads what the
   modules keep, the settings and the run's own state.

## What this does not do

- It does not make a module's first build instant: Cargo still compiles the SDK once a project.
- It does not move Sepulcra onto the shell.
- It does not sandbox a module: who may load native code is unchanged ([MODULES.md](MODULES.md)).
