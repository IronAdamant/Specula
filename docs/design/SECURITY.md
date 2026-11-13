# Security: the input-integrity monitor

The owner asked for this on 2026-11-07:

> "You may want to have a subroutine, monitoring the keyboard. If the keyboard is taken over without the other
> owner's knowledge, the subroutine must alert you, halting processes, then you need to advise whoever's the project
> owner of what has occurred, while you're working on other items."

This page covers the threat model, what the monitor sees, how it classifies what it sees, what it does about it, and
where it stops. The code is `crates/specula/src/inputguard.rs` (the monitor and the gate), `inputguard_engine.rs`
(the engine's side: freezes, the swarm pause, the toast) and `inputguard_tools.rs` (the `security_*` tools).

## Threat model

- **Every agent is untrusted.** That includes our own agents (Claude) and other vendors' agents (Grok, OpenAI and the
  rest). No agent is trusted by its vendor, its client name or the name it gives.
- **Identity comes only from authority grants** (AGENTS.md rule 14). The operator grants owner, lead and reviewer with
  `--owner`, `--lead` and `--reviewer`, and `authority::authority` answers. A role an agent names for itself carries
  nothing.
- **The human at the window is the owner.** Their keys, clicks and pads are theirs. They are never frozen and never
  locked out (F5).
- **What an attacker wants:**
  - the human's keystrokes, which could hold a password typed into the wrong field, or a paste;
  - the clipboard;
  - the human's clicks, which an engine screen turns into decisions written as `window:owner`;
  - the window itself, so it shows what the attacker chose while the human thinks they are choosing.
- **Out of scope:** local malware with the user's rights, and the OS. The operator, and the organisation they work for,
  secures the machine (AGENTS.md rule 12).

## What the monitor sees

One monitor (`inputguard::Monitor`, global, like the incident store) is fed from the places that do these things.
Nothing else does them:

| Channel | Where it is fed | What counts |
|---|---|---|
| `keyboard` | `text_world` (`text_edit {window_focus}`, on the caller's thread and again where it commits) | pointing the window's keyboard at a text, and every refused try |
| `clipboard` | `text_window` (the clipboard thread's requests) | every read (paste) and write (copy, cut) of the system clipboard |
| `screen` | `taste` (`gallery_open`, `gallery_create {open}`, `style_onboard {open}`, the human's F8), `review_engine` (`review_screen open`, the human's F9) | every engine screen put on the window, and every refused try |
| `editor_input` | `editor_engine` | simulated window input as the window's human, and every refused try |
| `screen_input` | `ui_engine` (`ui_act` on surface review or gallery) | driving an engine screen's buttons, and every refused try |
| `module_input` | `logic` (`logic_input`) | an agent's hold that would override an action the human is using at the window, and every hold the human took back |
| `native_code` | `logic` (`logic_build`, `logic_reload`, `logic_unload`) | every refused try at loading or unloading native module code (docs/design/MODULES.md) |

**It can't be bypassed.** The functions that point the keyboard (`WindowTyping::focus_on`), reach the clipboard
thread (`text_window::clipboard`) and open a screen (`GalleryScreen::open`, `ReviewScreen::open`) take an
`inputguard::Seen`. Only the monitor makes one, and it makes one only after it has recorded the event. A guard test,
`crates/specula/tests/inputguard_guard.rs`, checks three things:

- `Seen` is constructed only in `inputguard.rs`;
- the platform's clipboard calls appear only in the clipboard thread;
- every refusal site of the channels above reports to the monitor.

## How it classifies

Identity comes from the operator's grants and the swarm's identity key (what a resume token carries), never from a
name: a name is a label any agent can say.

| Who | Done | An explicit refused try |
|---|---|---|
| the human at the window, or an agent granted `--owner` | **fine** (recorded) | (never refused for authority) |
| a granted lead | **alert**, unless the owner allowed this lead for this session (then **allowed**) | **alert** |
| a granted reviewer (screens only, R4; the owner confirmed on 2026-11-13 that reviewers may put the review screen and galleries on the window) | **alert** | **incident** |
| any other agent | **incident** | **incident** |
| the engine itself, following the human's own action (the next onboarding step) | **fine** | none |

**Granted means granted by the operator** (`--owner`, `--lead`, `--reviewer`). Nobody becomes lead by asking
(owner, 2026-11-12; AUTHORITY.md): with no `--lead`, a run has no lead, and an incident waits for the owner. Until
then the first agent to say lead held it, was never frozen, and could clear every incident, its own included
(review R2). Every check on the monitor's channels also reads `authority::trusted` / `trusted_of_session`, which
ask for the grant itself: a second lock, so that no other road to lead authority could ever reach the human's
input, native code or the monitor.

**Only explicit tries count.** An explicit try is a call that names the human's input and is refused for authority
or ownership (`lead_only`, `owner_only`, `not_reviewer`, `not_allowed`). Argument mistakes and missing objects aren't
tries, with one exception: `text_edit {window_focus}` is a try whatever its target, because its authority is
checked before the text is looked up (so a refusal never says whether a text exists). The tool's schema says a
refused try freezes the caller.

Ordinary work that happens to touch a channel is no try either, and isn't counted:
- `style_onboard {action: "start"}` without `open`: for a caller who may not put screens on the window, the
  onboarding starts without one. Only an explicit `open: true` asks for the window.
- a builder's plain `logic_build`: it files a proposal, or files none when its folder can't be digested. Only
  `approve`, `logic_reload` and `logic_unload` without authority are tries at native code.

Neither is a refusal caused by state the agent didn't choose. Those are **noted** (class `noted`): on the record
(in a log of their own, so they can't push alerts out of the lead's feed), never counted, nothing halted:
- `window_typing`: a reviewer's screen refused because the human is typing;
- `engine_screen_open`: `ui_act` with no `surface` while the human (or a reviewer) has an engine screen open. The
  call meant the game; only `surface: "review"` or `"gallery"` named explicitly is a try at the screen;
- `human_input`: a `logic_input` hold on an action the human is using, and a hold the human took back (below);
- `owner_closed`: reopening a screen the human closed moments ago;
- `players_only`: a press the hosted game refused because only the player may make it (Quit, a button that would
  listen for the player's next key), from `ui_act` or a game tool (`Game::take_refused_presses`). An agent walking
  a menu presses every button, so this is never counted.

**A granted lead is never frozen.** Its refused tries are alerts. A swarm with a lead and no `--owner` agent can
therefore never lock itself out, and the owner's and the lead's `security_*` tools always answer.

**Holds never take the human's input over.** `lex_live` holds are kept apart from the window's input per action.
A `logic_input` hold records who placed it. Placing one on an action the human is using is refused (`human_input`,
noted) for anyone below lead; the owner's goes on, a lead's goes on with an alert. Whoever holds, **the window wins
afterwards**: the human's own input on a held action ends the hold at that tick (recorded as `logic_input.hold_ended`
under the holder), and a freeze releases every hold the frozen identity placed.

A paste or copy through a keyboard that a lead pointed (and the owner didn't allow) is an **alert**. The clipboard's
text lands in a world text, and every agent can read world texts.

## What it does

**On an alert:**
- a `security.alert` trace event;
- an entry in the monitor's log (`security_incidents`, class `alert`);
- a toast in the window with **Dismiss** (and, for a lead, **Allow this lead for this session**). It stays a minute
  unless answered. Repeats by the same identity on the same channel merge into one toast with a count;
- nothing is halted.

**On an incident:**
1. **The offender freezes: its identity and its session.** Every later call from that session, and from that
   identity in any session (a resume carries the identity), is refused with `session_frozen`. The refusal names the
   incident. Only `agent_hello` is allowed: it answers with the freeze **and changes nothing** (no new name, no
   new role, no other identity resumed, no new tool list), so a frozen session can't say "lead" or take a granted
   name to get out. A frozen identity resumed in a new session comes back frozen, with the role it had.
   Taking a frozen agent's *name* in a new identity is not frozen, and taking another agent's name can't freeze
   that agent. The engine thread refuses
   requests that were already in flight too, and the session's playtest steps. The identity's claims, leases and
   input holds are released at once, so the swarm isn't blocked.
2. **Agents are counted.** If a second distinct identity offends while an incident is still open, the whole swarm
   pauses:
   - every agent call is refused with `swarm_paused`, except the owner's and the lead's `security_*` tools and their
     reads;
   - the simulation clock pauses, as `time_control` would, and `set_by` is `engine:security`;
   - playtests hold where they are.
3. **The lead agent is alerted.** The monitor sends a `security.incident` trace event and adds a `trace_incidents`
   entry with code `security.incident`, one per offending identity. `security_incidents {wait_s}` wakes on it.
4. **The owner is told in the window.** An engine-drawn toast appears, always on top (only the typing banner draws
   over it) and never modal. The window title says `SECURITY` too. The toast says what was tried, then who on a line
   of their own, and what was halted, and offers **Clear** and **Keep frozen**. The agent's name is shown short and
   in quotes, here and in the title: a session that never said hello is named after its client, which chooses its
   name freely, so only the characters an agent's own name may have are kept (`swarm::default_name`).
   - **Incidents show before alerts and are never dropped:** an incident's toast stays until it is answered, however
     many alerts arrive (at most 16 alert toasts wait; past that the oldest alert goes, counted).
   - **A button acts on a press and release on that same button, and only once the toast has been still for 0.4 s.**
     A toast that appears or changes under the cursor can't take a click meant for something else.
   - Presses inside the toast are the toast's; every other click goes to the game as before, and a drag that ends
     over the toast is still released where it began.
   - **Text an agent chose** (an entity's name in the call) is shown short, in quotes and labelled ("Its call
     named: ..."), so it can't read as the engine's own words.
5. **It fails closed.** If one offender floods the monitor (more than 1,024 frozen sessions, frozen identities or
   open incidents), the swarm pauses. The monitor never forgets a freeze to make room. There is one trace incident
   per offender: a repeat after a clear counts on it again (`recurring`).
6. **Everything is recorded.** The incident's evidence holds the monitor's event, its trace event id and the journal
   entries. The freeze, the swarm pause, the allowance and the clear are each journaled as kind `security`. These are
   records only: no world change, not undoable.

**Clearing:**
- `security_clear` (owner or lead) and the toast's Clear (the human) unfreeze the identities and sessions, lift the
  swarm pause and resume the clock, if the pause stopped it and nobody has moved it since.
- **Keep frozen** is the owner's choice. After it, only the owner (the window, or an `--owner` agent) can clear that
  incident; a lead's `security_clear` is refused with `owner_kept`.
- **Allow this lead for this session** (`security_allow_lead`, owner only, or an alert toast's button) covers that
  lead's current protocol session. A resume in another session asks again.

## Tools (family `security`)

- `security_incidents` (a granted lead, the owner): the lead's feed after a cursor: alerts, incidents, allowed
  leads, and the monitor's own records (clears and allowances, channel `monitor`). Each event has its class,
  channel, agent, authority, outcome and what was halted. The answer also gives the state: frozen identities, the
  swarm pause, allowed leads, and the toast.
  - `wait_s` (0..60) blocks until something new arrives. It answers at once when the engine stops (a pending wait
    never holds the shutdown), and at most 32 calls wait at a time.
  - `all: true` adds the fine and noted events (the human's own F8, focus drops, uncounted refusals).
  - `missed: true` says events after the caller's cursor were dropped before it read them (the feed keeps the newest
    512, and the uncounted events' log its own newest 512). Freezes, pauses, clears and allowances are still in the
    journal (`kind: security`), incidents in `trace_incidents`, and alerts in the trace (`security.alert`).
- `security_clear` (a granted lead, the owner): one incident, or all of them.
- `security_allow_lead` (owner only): allow, or revoke, a lead for its current session.

## The input sweep's findings (fixed here)

- **R1:** a decision the human makes at the window, on a gallery an agent made, is journaled as `window:owner` with
  the gallery's maker as `via`. It is never bare owner attribution. Putting a gallery on the window needs owner, lead
  or reviewer authority; being its maker isn't enough. A right-click Dislike waits for the same 0.4 s settle as every
  other decision.
- **R3:** the human's in-progress typing is the owner's alone:
  - `editor_state`'s `typing` text and outliner `search`. While the search holds any text (focused or not), the
    outliner's rows, their count, its paging and the rows' widget boxes are withheld too: what a search found says
    what was typed;
  - `gallery_get` / `gallery_open`'s screen `note`;
  - `ui_tree`'s note field on the gallery surface.

  Everyone else gets `redacted: true` and nothing more: not even the length, which polled per key gives the
  typing's rhythm and a secret's length away.
- **R4:** reviewers keep putting screens on the window, and every such open is an alert. This waits on the owner's
  answer, in HANDOFF's list.

## System DLLs load from System32 only

`specula_sys::dynlib::Library::open(name)` takes a bare file name and loads it with
`LoadLibraryExW(LOAD_LIBRARY_SEARCH_SYSTEM32)`, so a `bcrypt.dll`, `vulkan-1.dll`, `xinput1_4.dll`, `iphlpapi.dll`,
`shcore.dll` or `avrt.dll` planted beside the executable is never loaded. There are two other deliberate paths:

- `Library::open_beside_exe(name)`, for DLLs that ship with the game (`steam_api64.dll` later). It searches the
  application folder and System32.
- `Library::open_path(path)`, for an absolute path the engine made itself (game modules' copies). Its dependencies
  come from that folder and System32.

The test plants a copy of a System32 DLL beside the test executable. It checks that `open` loads System32's and that
`open_beside_exe` would have loaded the plant.

## The launcher's files

The project list, the files that say which engines are running, and the two requests an engine reads from disk
(stop, and a project handed to a waiting start screen) live in the engine's per-user folder
([LAUNCH.md](LAUNCH.md)).

- **They grant nothing new.** Whoever can write a person's per-user folder can already start and end that
  person's programs. A stop request does what Ctrl+C does. A hand-over makes the engine start **its own
  executable** again in a folder that exists, and only while it is still nothing but a start screen.
- **The list of a person's projects is served by no tool.** It says what they work on. The start screen is
  no UI-tools surface, hears the window's own input only, and is never drawn in a capture;
  `engine_info.project` names the one project an engine has open, to the agents working in it. The
  launcher's traces name no folder and no count (agents read traces).
- **The start screen's paste is the person's own.** Ctrl+V in its folder field reads the clipboard on
  `text_window`'s clipboard thread, recorded by the monitor as the human's paste, like every other read.
- **A command is never sent to the wrong engine.** An instance file counts only while the process it names
  owns its port, so a program that took a dead engine's port is never called or stopped in its name.
- **`specula open` never takes the keyboard.** The engine it starts opens its window behind the others
  unless a person passes `--front`.
- **`specula call` speaks to loopback only**, as every client does. `--as NAME` keeps a resume token beside
  the engine's instance file, readable by the person's own account, removed when the engine is gone.
- **`--settings off` keeps none of it:** such an engine is not listed and cannot be asked to stop from
  outside (every script's and test's engine).

## Limits, said plainly

- **Two sessions can pause the swarm.** Any client with two identities can make two refused tries and pause every
  agent, again and again after each clear. That is the owner's escalation rule (a second offender means the swarm
  can't be trusted to carry on), and it costs availability, never integrity. Until per-agent auth tokens (M6), the
  operator's answer is to see who connects.
- **A lead's clear removes the toast.** When the lead clears an incident, its toast goes at once: the owner may
  never have seen it in the window. The journal (`kind: security`), `security_incidents` and `trace_incidents` keep
  it. **Keep frozen** is how the owner takes an incident out of the lead's hands.
- **A dropped focus isn't blamed on anyone.** When an agent's write to the focused text drops the human's typing
  focus, the monitor records the drop as the engine's (`focus_dropped`). The write itself is in the world's journal
  under the agent's name.
- **Modules are native code that the operator runs.** A module loaded through `logic_build` / `logic_reload` runs
  in the engine's process. It could read keys or the clipboard through the OS itself, and nothing here can stop it.
  The monitor sees what goes through the engine, not what a module's own code does. Only build and load modules
  whose source you trust (docs/design/MODULES.md).
- **Names until M6.** Grants bind a name to the first identity that takes it. Until per-agent auth tokens exist, an
  agent that learns a granted lead's resume token *is* that lead, and an agent that says hello under a granted
  name before its holder connects *is* that lead too. A frozen session can't do it (its hello changes nothing), but
  nothing stops the same program from opening a new connection and trying. The operator's answer until M6: start
  the granted agents first, and see who connects. The engine binds to loopback only (AGENTS.md rule 12).
- **World text is shared.** What the human types into a world text the operator pointed the keyboard at becomes
  world data, and every agent reads world data. That is why a lead's pointing is always an alert.
- **Hosted games.** A hosted game's own code (Sepulcra) is linked into the engine and trusted as the engine is.
  Its own text fields, if it ever has any, would need feeding to the monitor too.
- **Other processes on the desktop** can read the clipboard through the OS. Only the engine's own accesses are seen.
