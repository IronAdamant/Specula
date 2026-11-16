# The launcher: projects, the start screen, and finding a running engine

Status: built 2026-11-13 (lane m5-launch). Code: `crates/specula/src/launch_*.rs`
(`launch_list` the list, `launch_store` its files, `launch_instances` running engines,
`launch_cli` and `launch_run` the commands, `launch_client` the commands' MCP client,
`launch_engine` the engine's side, `launch_screen` and `launch_window` the start screen).
Live check: `launcher_check` (`tools/examen/src/native/launch.rs`; `examen live --only launcher_check`).

## Why

The owner, 2026-11-13: the engine "must be openable, and have list of items readily available.
If no project exists before, then the list is empty, but if the previous projects were worked
on, then that project itself must be listed. And users should be able to either use the UI or
ask you to resume the last worked on project, or select a different project", "with last worked
project moved to #1"; and "in terminal, you can call that tool. Or have it be opened via .exe,
and with it running, have you or other AI agents find it then run it."

So there are three ways in, and they agree with each other:

| Who | How |
|---|---|
| A person | Double-click the executable (or run `specula` with no argument): the **start screen** lists their projects, the last one worked on first. |
| An agent or a person with a shell | `specula projects`, `specula open --last`, `specula open NAME`, `specula new NAME`. |
| An agent with only a shell and a running engine | `specula list` finds it; `specula tools` and `specula call TOOL '{json}'` use it with no MCP client. |

## What a project is

A project is **a folder**. The engine's working directory is that folder, so its worlds,
captures, saves and caches are the project's own, and its assets are the folder named in its
marker.

- The marker is `specula.project.json` in the folder: `{"schema": "specula.project.v1", "name":
  "...", "assets": "assets"}` (`assets` optional: without it the folder itself, as an engine
  started there has always read it).
- `specula new NAME [--in FOLDER]` makes `FOLDER/NAME` as a project that builds and plays: its marker, a
  game file, screens and words, a module, this engine's SDK, a check and two notes
  ([FIRST_HOUR.md](FIRST_HOUR.md), slice 7). `--bare` makes only the folder, its marker and an `assets` folder.
  Without `--in` it is made in `Specula Projects` in the person's home folder. A folder that is
  not empty is never taken.
- Any folder can be opened as a project (`specula open FOLDER`, `--project FOLDER`, the start
  screen's "Open a folder"). It gets the marker, named after the folder; nothing else in it is
  touched.
- An engine started in a folder that holds a marker has that project open, with no flag.
- A name is a folder's name on every system: 1 to 64 characters, none of `/ \ : * ? " < > |`,
  no dot or space at its ends, and not one Windows keeps for a device (`CON`, `NUL`, `COM1`...).

A project carries nothing the machine doesn't already have: copy the folder to another machine
and open it there.

### One engine a project

Two engines on one project would write the same worlds and caches. An engine holds a lock named
after its project's folder for as long as it runs, and a second one is refused at start-up
("is open in another engine"). `specula open` and the start screen ask for the lock before they
open a project, and answer with the engine that holds it.

- The lock is a file in the system's temporary folder (`specula-locks/<hash of the folder>.lock`,
  per user on Windows), **not in the project**: nothing is added to a folder a person may keep
  in version control, and engines started with different `--settings` still see each other.
- It is held as an open handle that shares the file with nobody, so the system drops it when
  the process ends, however it ends. A crash leaves nothing to clean up.
- Windows only so far. Elsewhere no engine runs yet; the lock says it is unavailable there and
  the engine says so at start-up.

## The list

`projects.json` in the engine's per-user folder (`specula.projects.v1`): each project's folder,
name, the executable that opened it last (a game's own, or the stock engine), the world it had
open, and when. **The last one opened is first.** At most 64; a project whose folder is gone
stays listed (as missing) until it is forgotten, because a drive that is unplugged today is back
tomorrow. A list file that cannot be read is moved aside, never overwritten.

A project is put first when it is made (`specula new`) and whenever an engine **starts
serving** on it (not when a command merely asks), and it keeps the world it had open when that
engine stops.

### Who may read it

The list says what a person works on. **No MCP tool serves it**, and the start screen is no
UI-tools surface and is never drawn in a capture. `engine_info.project` says the one project an
engine has open, to the agents working in it. An agent that can run commands on the machine can
run `specula projects`, as it can read any of the person's files: that is the operating
system's line, not the engine's.

## Running engines

An engine that serves writes `engine.instances/<pid>.json` in the per-user folder: the port it
is bound to, its URL (loopback), its project, its world, whether it has a window, its version.
The file is written whole or not at all (a temporary file, renamed). The engine removes it when
it stops.

`specula list` reads the folder and keeps the engines that are **serving**: the process the
file names owns a listening socket on that port (the system's listener table says who owns each
one). "Something answers on that port" is not enough: a stopped engine's port goes to the next
program, and a command sent there in its name would reach the wrong engine. A file whose engine
is gone is removed, with what it left beside it; a file that does not read as an instance is
left alone while its process runs.

All of this follows `--settings`:

| The engine was started with | The list and its instance file |
|---|---|
| nothing | the engine's per-user folder |
| `--settings DIR` | in `DIR` (a script's or a test's own) |
| `--settings off` | none: it is not listed and not found (every repo script's engine) |

## The commands

The same executable. A first argument that is a word is a command: it answers on standard
output as JSON and exits, in a process that never becomes an engine. Arguments that start with
a flag (or none) are the engine's own, as before.

```
specula                          the start screen
specula projects                 the projects worked on, most recent first
specula open --last              open the project last worked on
specula open NAME|FOLDER [--front] [-- engine flags]
specula play NAME|FOLDER|--last [--as-is]
                                 open a project in front, with its modules loaded as their owner last
                                 built them (--run); --as-is: whatever is on disk now (--run-as-is)
specula run [NAME|FOLDER] --ticks N [--inputs T:WORD,...|@FILE] [--seed N] [--settings FOLDER]
            [--frame FILE.png [--size WxH]] [--follow]
                                 play a project's game with no window, from its start, by a script
                                 of inputs, and print how it stands as one JSON object; its modules
                                 are built first, as they are on disk (as play --as-is). Nothing is
                                 left running (FIRST_HOUR.md, "A headless run"). Its answer is one
                                 line, a refusal as much as a state; a module whose call failed is
                                 a refusal. --follow: then a step a line from standard input
                                 (TICKS, or TICKS WORD), the state printed after each
specula check [NAME|FOLDER] [--only NAME] [--file FILE]
                                 play every run the project's checks/runs.json lists and hold
                                 each state against what the check expects: a line a check,
                                 then a count; exit code 1 when one did not hold
                                 (FIRST_HOUR.md, "The game's own checks")
specula new NAME [--in FOLDER] [--bare]
                                 make a project that builds and plays, and list it
specula forget NAME|FOLDER       take a project off the list (its folder stays)
specula list                     the engines running on this machine
specula stop [--port N | --project NAME]
specula tools [FILTER] [--port N | --project NAME]
specula call TOOL ['{json}' | @FILE] [--port N | --project NAME] [--as NAME [--role ROLE]]
specula help
```

- **Exit codes:** 0 done; 1 refused (the answer says why: `{"error": code, "message": ...}`);
  2 no engine was reached. A command line that is not one is answered the same way
  (`"error": "usage"`, exit code 1).
- **`open`** answers once the engine serves, with its port and URL. An engine that already has
  the project open is found, and none is started: two `open` of one project at once start one
  engine, and both are answered with it. The port is 7471 when it is free (a configured MCP
  client finds the engine there), else one the system has free; `-- --port N` names one.
  The executable that opens it is the one the command was run from, unless a game's own
  executable opened the project last.
- **Its window opens behind the others** and takes no keyboard, unless `--front`: an agent
  opening a project never interrupts whoever is using the machine (owner, 2026-11-13).
- **`open` and a start screen that is waiting:** the project is handed to it, and the window
  the person already has open becomes that project, on its port (below).
- **`stop`** asks the engine to shut down as Ctrl+C does (worlds saved) and answers once its
  process has ended. An engine started in the background has no console to interrupt.
- **`tools`** with no word answers with every tool's **name, by family** (`"world": "world_spawn
  world_set ..."`), and says how to see more: `tools NAME` shows one tool whole, its description and the
  arguments it takes (its schema), and `tools WORD` lists the tools whose name or
  description has the word, each with the start of what it does. The whole list with
  descriptions is about 46,000 bytes, which an agent pays for in its context every time it
  asks; the names are a tenth of that.
- **`call TOOL @FILE`** reads the arguments, one JSON object, from a file (named against the
  folder the command is run in; at most 1 MiB, as on the command line; a byte order mark is
  skipped). Quoting JSON for a shell is the hard part of calling a tool from a terminal, and
  a command line holds about 32,000 characters on Windows: a recipe or a module's source goes
  in a file. A file that is missing, too large or not one JSON object is refused as `usage`
  before any engine is looked for.
- **`call ... --as NAME --role ROLE`** says the role with the name. An identity's authority is
  what its latest hello was admitted with ([AUTHORITY.md](AUTHORITY.md)), and a command is a
  session of its own, so a lead the operator granted (`--lead NAME` when the engine started)
  says `--role lead` on the commands that need it: building a module, forcing a claim. A role
  nobody granted that name is refused, as over MCP. Without `--role` the agent is a builder.
- **`call`** opens an MCP session, calls one tool and closes the session, named or not.
  `--as NAME` keeps one identity across commands: an agent's identity and claims outlive its
  session, and the next command resumes them with a token kept beside the engine's instance
  file (it goes when the engine does). With one engine running no target is needed; with
  several the command asks which.
- **A command's own `--settings DIR`** reads and writes there instead of the per-user folder,
  and an engine it starts is given the same one. A relative folder is the one the command was
  run in. The same holds for an engine's own flags: `--project` makes the project's folder the
  working directory, and every other folder the command line names (`--settings`, `--captures`,
  `--worlds`...) is still read against the folder the engine was started in.

## The start screen

The executable opened with no argument at all, or with `--launcher`, is an ordinary engine
(it serves MCP, its world is empty) with the start screen over its window. Not when it has no
window, hosts a game (a game's executable is its own project), has a project already, or
keeps no list (`--settings off`).

- **The list**, the last project worked on first, as the primary button with the focus: Enter
  resumes it. Each row says where the project is and when it was last opened, or that its
  folder is gone, or that an engine has it open now. Six rows a page (PageUp / PageDown, or
  LB / RB).
- **New project:** a typed name; it is made in the home folder's `Specula Projects` and opened.
- **Open a folder:** a typed path, or pasted with Ctrl+V.
- **Forget:** asked first, with the focus on "Keep it"; the folder is not touched.
- **Close** (or Esc): the engine goes on with no project, as it always could.
- An empty list says so, and the focus is on "New project".
- Mouse, touch, keys and gamepads all work; buttons keep the 44 px touch height (F5, F6).

**Choosing a project stops this engine and starts the executable again** with `--project
FOLDER` and the same flags. A project is a working directory, a set of caches and a world, and
an engine has exactly one of each from start to stop; starting again is the one path that is
already tested (it is what `specula open` does). The window closes and opens again a moment
later, on the same port. The folder is checked on a worker at that moment (it is there, and no
engine has it open), whatever the rows said; closing the screen while that check runs means
"no project", and nothing opens.

The rows' own facts (a folder that is gone, an engine that has the project open) are checked on
a worker from the first frame, never at start-up: a project on a drive that is not there can
take many seconds to say so, and that wait is never the window's.

Double-clicked while another program has port 7471, the engine takes a free port (its instance
file says which) rather than showing no window at all.

**It is the person's screen.** It has a UI instance of its own, hears the window's own input
only (never an agent's `ui_act`), is no UI-tools surface, and is drawn in window frames only.
If the engine's world is not empty (an agent built something with no project), the screen says
how many entities would be left behind.

### A project handed to a waiting start screen

A person opens the engine, sees the start screen, and asks their agent to resume the last
project. The agent runs `specula open --last`. Starting a second engine beside the start screen
would leave the person looking at a list while the work happens in another window, on another
port than the one their MCP client is configured for. So:

1. A start screen's instance file says `"launcher": true`.
2. `specula open` (with no engine flags of its own) writes `engine.instances/<pid>.open` with
   the project's folder (an absolute path), and `front` when the command said `--front`. One
   request waits at a time: a second command's is not written over it, and that command starts
   an engine as always.
3. The engine's watcher thread takes the file once the start screen has drawn its first frame
   (before that nobody knows whether the engine is only a start screen, and the request waits).
   If the engine is **still only a start screen** (the screen is open, the world is empty), the
   folder is there and no engine has it open, the engine stops and the executable starts again
   in the project. The command answers with that engine (`"from_start_screen": true`).
4. Otherwise the watcher answers `<pid>.refused`, and the command starts an engine as always.

The window a hand-over opens comes up **behind the others** unless the command said `--front`:
a project chosen at the window is the person's own act, one handed over by a command is not.

The same thread reads `<pid>.stop`. These files are the whole protocol: whoever can write the
person's per-user folder can already start and end their processes, so they grant nothing new
([SECURITY.md](SECURITY.md)). The engine only ever starts **its own executable** again, in a
folder that exists.

## What was found on the way

- **`specula open` left its engine holding the command's output.** On Windows a child inherits
  every inheritable handle of its parent, whatever its own standard handles are set to. Whoever
  read the command's answer through a pipe (an agent's shell) waited for the engine to end: the
  first live run waited 40 minutes. The command now marks its own standard handles as not
  inherited before it starts the engine (`specula_sys::process::keep_std_handles`), and the live
  check reads every command through pipes with a time limit.
- **`stop` answered when the engine stopped serving**, while it was still saving and freeing
  the GPU. It answers when the process has ended.
- **The first review (R1) found** that two engines could open one project (the lock above);
  that a dead engine's instance file passed for a running engine when another program took its
  port (serving is now "its own process owns the port"); that 65 resume tokens hid an engine and
  the sweep then deleted every token (instance files are listed before the cap, and only what
  belongs to an engine whose file is gone is swept); that a relative `--settings` landed inside
  the project; that a hand-over before the first frame was refused; that a refused hand-over
  put another project's path in the engine's traces (traces carry no folder now); that every
  `call --as` left a session open; and that the start screen's paste read the clipboard outside
  the thread the input monitor watches (it goes through `text_window`'s clipboard thread, seen
  as the person's paste).

## Known limits

- **A project's lock is this machine's, in its temporary folder.** A project on a shared
  drive, opened from two machines at once, is not caught; nor is a second engine started with
  another `TMP` / `TEMP` than the first.
- **A hosted game's own arguments** (after `--`) are the game's: a relative path among them is
  read by the game against the project's folder once `--project` has made it the working
  directory, not against the folder the engine was started in.
- **Windows first** (owner, 2026-11-13). The commands and the list are portable code; the
  per-user folder and the home folder are found from the environment on every platform. The
  macOS and Linux variants come with those ports (an `.app` bundle and a `.desktop` entry are
  packaging, not engine code).
- **A folder is typed or pasted**, not picked in the operating system's folder dialog. The
  dialog needs COM (`shell32`, `ole32`) on the window's thread; it is a later addition.
- **The start screen cannot switch project in place.** It starts the executable again (above).
- **`--settings off` engines are not found.** That is the point of the flag; a script that
  wants its engine found passes `--settings DIR`.
- **The executable is a console program:** double-clicked, it opens a console window beside
  its own. A release build for players (`specula export`, M6) is where that is decided.
