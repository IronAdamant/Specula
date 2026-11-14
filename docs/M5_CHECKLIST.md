# M5: what only a person can judge

M5's exit proof is automated (`scripts/m5_exit.py`, [ROADMAP.md](ROADMAP.md) "Exit proof") and its live
checks read pixels, packets and loudness. What they cannot do is hear, read comfortably, or feel a control.
This list is for the owner, at the window, whenever convenient: nothing in M5 waits on it, and anything
that fails here is an M6 item (M6 is the testing phase: owner, 2026-11-14).

Build once: `cargo build --release -p specula -p sepulcra-app`.

## The engine, opened by hand

- [ ] `target\release\specula.exe` with no argument shows the start screen: the projects worked on, the
      last one first. "New project" makes one; choosing it opens it in the same window.
- [ ] `specula projects` in a terminal lists the same projects; `specula open --last` opens the last one
      behind the other windows, without taking the keyboard.

## Sepulcra: the shell, sound and text

Run `target\release\sepulcra.exe --port 7486 -- --seed 7`
([games/sepulcra/TESTPLAN.md](../games/sepulcra/TESTPLAN.md) has the full in-game plan).

- [ ] The title screen reads well on the laptop screen and on the 49" monitor (text size, contrast).
- [ ] Settings: change the language to German and back; every word on the screen changes at once.
- [ ] Settings are kept: close the game, start it again, and they are as you left them.
- [ ] Sound: map 1's music plays and loops without a seam you can hear; effects are louder than the music
      without hurting; nothing crackles or drops out during a busy wave.
- [ ] Losing a run plays the defeat sting over ducked music.
- [ ] A gamepad pauses the game, and moves the focus through the pause menu and the settings.
- [ ] Rebinding a key works and is kept.

## The editor, shared with agents

In the same window, F7 opens the editor ([design/EDITOR.md](design/EDITOR.md)).

- [ ] Click selects; the gizmo drags, rotates and scales; Ctrl+Z takes each back.
- [ ] The draw tools: paint a prop's colours, sketch a path, sculpt and splat the ground. Each stroke is one
      undo.
- [ ] The journal names your edits as yours (`window:owner`) beside the agents'.
- [ ] "Highlight and ask" (the Swarm tab): select something, ask for a change, and an agent's answer comes
      back as a change you can undo.
- [ ] While you type or drag, no agent takes your pointer or your keyboard.

## The exit proof's pictures

- [ ] `python scripts/m5_exit.py` passes, and the pictures in `target\m5-exit\integrated\` look right to
      you: `yard_1.png` (three lanterns and a creature), `board_1.png` (a 2D map with a character),
      `annex.png` (the prop, its changed copy in iron, the creature and a 2D sprite standing in 3D),
      `walker_walk.png` beside `walker_idle.png` (the same creature under two clips).
