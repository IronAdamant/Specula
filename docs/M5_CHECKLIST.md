# M5: what only a person can judge

M5's exit proof is automated (`scripts/m5_exit.py`, [ROADMAP.md](ROADMAP.md) "Exit proof") and its live
checks read pixels, packets and loudness. What they cannot do is hear, read comfortably, or feel a control.
This list is for the owner, at the window, whenever convenient: nothing in M5 waits on it, and anything
that fails here is an M6 item (M6 is the testing phase: owner, 2026-11-14).

Build once: `cargo build --release -p specula -p sepulcra-app`.

**Filled in by Claude, 2026-11-15** (owner: "Since you can modify these files, they should be filled in").
Under each item:

- **Checked** is what a live check or a test verified, and which one. All of them passed in the final
  audit of 2026-11-15 (`scripts/live_checks.py`, 70 of 70); the text, interface and launcher checks were
  run again after the last engine change of that day.
- **Looked at** is Claude's own reading of a picture from those runs. It is an opinion, not a measurement.
- **Left for you** is the part no check and no agent can do: your ears, your eyes at your desk, your hands.

A box is ticked only where nothing is left for a person. **8 of 15 are ticked; 7 have a part left for you**,
most of them a minute each.

## The engine, opened by hand

- [ ] `target\release\specula.exe` with no argument shows the start screen: the projects worked on, the
      last one first. "New project" makes one; choosing it opens it in the same window.
  - **Checked** (`launch_check.py`): the start screen with an empty list offers a new project, made and
    opened from the keyboard alone; the project last worked on is first and Enter resumes it; the window
    becomes that project on the same port. The check opens it with `--launcher` and a scratch settings
    folder.
  - **Left for you:** open it once with no argument. No script ever does, because that reads your own
    per-user list, which scripts are kept away from: that your real projects show there is unseen.
- [x] `specula projects` in a terminal lists the same projects; `specula open --last` opens the last one
      behind the other windows, without taking the keyboard.
  - **Checked** (`launch_check.py`): `projects`, `new`, `open`, `open --last`, `list`, `tools`, `call`,
    `stop` and `forget` with the real executable; a window that `open` starts opens behind the others and
    takes no keyboard. The terminal and the start screen read one list.

## Sepulcra: the shell, sound and text

Run `target\release\sepulcra.exe --port 7486 -- --seed 7`
([games/sepulcra/TESTPLAN.md](../games/sepulcra/TESTPLAN.md) has the full in-game plan).

- [ ] The title screen reads well on the laptop screen and on the 49" monitor (text size, contrast).
  - **Checked** (`a11y_check.py`, `ui_check.py`): the title, the HUD, the pause menu and the settings pass
    the legibility and contrast rules at the Steam Deck's size, 1080p, 4K and a 175% laptop window, at
    interface sizes 0.8, 1 and 1.5, in every colour theme and text size.
  - **Looked at** (the title at 1280 x 800): the name, the line under it and the five buttons read
    clearly against the dark graveyard; "New run" stands out as the first choice; "Continue" is dimmed,
    as it should be with no run to continue.
  - **Left for you:** whether it is comfortable at the distance you sit from the 49" monitor.
- [x] Settings: change the language to German and back; every word on the screen changes at once.
  - **Checked** (`l10n_check.py`): the language is switched with the settings' own Language button, and
    every screen passes the interface check in English, German and the pseudo-locale at every display
    and size, in the start and a busy state. A word a screen writes itself, which would stay English, is
    a failure of that check.
  - Not judged: whether the German reads naturally to a German speaker. An agent translated it.
- [x] Settings are kept: close the game, start it again, and they are as you left them.
  - **Checked** (`a11y_check.py`, step 8): a new engine with the same settings folder has the colour
    theme, text size, captions, subtitle size and the key binding back.
- [ ] Sound: map 1's music plays and loops without a seam you can hear; effects are louder than the music
      without hurting; nothing crackles or drops out during a busy wave.
  - **Checked** (`audio_check.py`, on the real audio device): map 1's theme streams on the music bus; its
    loop is 58.2 s at -29 LUFS (the target); the effects' loud moments are 3 LU or more above the music;
    no starvation, no clipped samples and no overs on the master output through a busy run. The decoder's
    loops are sample-exact against a reference decode.
  - **Left for you:** listening. A seam, a harsh effect or a crackle on your speakers is something only
    ears find; the numbers say there is no reason to expect one.
- [x] Losing a run plays the defeat sting over ducked music.
  - **Checked** (`audio_check.py`, step 4): in a run lost with no defences the sting plays on the voice
    bus, the music ducks under it and crossfades into the defeat cue.
- [ ] A gamepad pauses the game, and moves the focus through the pause menu and the settings.
  - **Checked**, with a gamepad's presses sent through the engine, never a real one: Start opens the pause
    menu and pauses, Start again resumes (`a11y_check.py`, step 7); a pad reaches every menu and every
    HUD button has a pad route (`sepulcra-app`'s `a_gamepad_reaches_every_menu`, `specula-ui`'s tests).
  - **Left for you:** a real controller in your hands. No physical gamepad has been connected in any run.
- [ ] Rebinding a key works and is kept.
  - **Checked** (`a11y_check.py`, steps 3 and 8): a binding set by name (`call_wave` to G) works and is
    back after a restart.
  - **Left for you:** rebinding by pressing the key yourself. The game refuses to listen for "the next
    key" on an agent's behalf, by design, so no check has done it the way a player does.

## The editor, shared with agents

In the same window, F7 opens the editor ([design/EDITOR.md](design/EDITOR.md)).

- [x] Click selects; the gizmo drags, rotates and scales; Ctrl+Z takes each back.
  - **Checked** (`editor_check.py`): F7 and a click through the window's own message queue; click and box
    select; move, rotate and scale drags with snapping; nothing is written until the release; Ctrl+Z and
    Ctrl+Y.
  - Not judged: how the gizmo feels under your hand.
- [x] The draw tools: paint a prop's colours, sketch a path, sculpt and splat the ground. Each stroke is one
      undo.
  - **Checked** (`eddraw_check.py`): a sculpt drag is one terrain edit and one journal entry, taken back by
    Ctrl+Z; the same for a paint drag on a model; splat strokes land on the layer in hand; the path tool
    redraws a swept part; a stroke by a real mouse is journaled as the person's alone.
  - Not judged: how the brushes feel.
- [x] The journal names your edits as yours (`window:owner`) beside the agents'.
  - **Checked** (`editor_check.py`): every edit made at the window is journaled as `window:owner`, and
    agents see it in `world_history` and `world_changes`.
- [ ] "Highlight and ask" (the Swarm tab): select something, ask for a change, and an agent's answer comes
      back as a change you can undo.
  - **Checked** (`edswarm_check.py`): selecting a statue's head and asking posts one task as
    `window:owner`; an agent takes it, changes the statue and submits it; it is opened from the Swarm tab
    and approved in the review screen. What is typed in the box is shown to no agent.
  - **Left for you:** undoing the change after it comes back. That last step is not part of the check.
- [x] While you type or drag, no agent takes your pointer or your keyboard.
  - **Checked** (`inputguard_check.py`, `edswarm_check.py`, `editor_check.py`): an agent that reaches for
    the window's keyboard or the editor's input is refused and frozen, and a second one pauses the swarm;
    while the ask box is open even the owner's own agent is refused; an agent's change made while you type
    does not replace your value. No script moves the pointer or presses keys for you (a guard holds that).

## The exit proof's pictures

- [ ] `python scripts/m5_exit.py` passes, and the pictures in `target\m5-exit\integrated\` look right to
      you: `yard_1.png` (three lanterns and a creature), `board_1.png` (a 2D map with two characters),
      `annex.png` (the prop, its changed copy in iron, the creature and a 2D sprite standing in 3D),
      `walker_walk.png` beside `walker_idle.png` (the same creature under two clips).
  - **Checked:** it passes on both adapters, and reopening the saved world draws the same pictures
    (held to 0.05). An independent reviewer checked the pictures when the proof was built.
  - **Looked at** (the integrated adapter's): `yard_1` is three lantern posts, each lit by its own
    candle, and the creature at the left with its shadow; `board_1` is grass, two paths, a pond, bushes
    and two small characters; `annex` has a wooden post beside a darker iron one, the creature in front
    and the 2D character standing at the left; in `walker_walk` the legs are apart in a stride and in
    `walker_idle` they are together. Nothing looks broken, cut off or unlit.
  - **Left for you:** whether they look right to you. That is taste, and it is yours.
