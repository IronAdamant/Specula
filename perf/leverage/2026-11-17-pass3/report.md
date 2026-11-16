# Measured leverage: the runs

Folder: `lr3`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change runs: 6cfff79b
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium runs: 6cfff79b
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: 6cfff79b
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 110,389 | 0.24 | 33,603 | 292,212 | 7,834 | 12 | 11 | 3 | done | 10 | 869 / 1,623 | 8 / 9 | 1 / 1 | 3 | 1,070 |
| change-engine-2 | 108,723 | 0.24 | 30,029 | 244,925 | 9,340 | 11 | 11 | 3 | done | 15 | 1,285 / 3,745 | 8 / 9 | 1 / 1 | 1 | 1,086 |
| change-engine-3 | 148,065 | 0.30 | 33,208 | 471,538 | 11,882 | 18 | 18 | 7 | done | 34 | 2,964 / 7,938 | 8 / 9 | 1 / 1 | 3 | 1,043 |
| change-plain-1 | 78,710 | 0.18 | 22,246 | 125,814 | 7,665 | 8 | 8 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 553 |
| change-plain-2 | 84,782 | 0.18 | 22,343 | 197,834 | 7,415 | 11 | 12 | 1 | done | - | - | 9 / 9 | 1 / 1 | 2 | 640 |
| medium-engine-1 | 216,414 | 0.44 | 43,565 | 597,217 | 20,449 | 18 | 19 | 4 | done | 15 | 1,407 / 3,694 | 19 / 19 | 2 / 2 | 5 | 971 (621 at the start) |
| medium-engine-2 | 242,012 | 0.49 | 47,942 | 663,641 | 23,146 | 19 | 21 | 5 | done | 20 | 1,259 / 5,151 | 19 / 19 | 2 / 2 | 5 | 975 (621 at the start) |
| medium-engine-3 | 198,595 | 0.40 | 39,975 | 530,252 | 19,122 | 17 | 20 | 3 | done | 14 | 1,199 / 3,610 | 19 / 19 | 2 / 2 | 5 | 939 (621 at the start) |
| medium-plain-1 | 228,257 | 0.47 | 35,413 | 437,734 | 28,045 | 15 | 18 | 3 | done | - | - | 19 / 19 | 2 / 2 | 5 | 514 |
| medium-plain-2 | 207,481 | 0.44 | 34,560 | 235,262 | 28,152 | 10 | 13 | 3 | done | - | - | 19 / 19 | 2 / 2 | 4 | 555 |
| small-engine-1 | 142,520 | 0.30 | 31,448 | 289,268 | 14,858 | 13 | 18 | 2 | done | 6 | 258 / 965 | 14 / 14 | 2 / 2 | 3 | 705 (621 at the start) |
| small-engine-2 | 149,332 | 0.32 | 31,351 | 305,297 | 15,924 | 13 | 17 | 3 | done | 13 | 645 / 2,106 | 14 / 14 | 2 / 2 | 1 | 727 (621 at the start) |
| small-plain-1 | 84,688 | 0.18 | 15,404 | 90,012 | 11,287 | 7 | 7 | 1 | done | - | - | 14 / 14 | 2 / 2 | 5 | 254 |
| small-plain-2 | 106,277 | 0.23 | 18,525 | 120,946 | 14,206 | 8 | 8 | 2 | done | - | - | 14 / 14 | 2 / 2 | 4 | 294 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 3 of 3 | 122,392 | 108,723 to 148,065 | 4 | 8.0 / 9 | 15,299 |
| change | plain | 2 of 2 | 81,746 | 78,710 to 84,782 | 1 | 9.0 / 9 | 9,083 |
| medium | engine | 3 of 3 | 219,007 | 198,595 to 242,012 | 4 | 19.0 / 19 | 11,527 |
| medium | plain | 2 of 2 | 217,869 | 207,481 to 228,257 | 3 | 19.0 / 19 | 11,467 |
| small | engine | 2 of 2 | 145,926 | 142,520 to 149,332 | 3 | 14.0 / 14 | 10,423 |
| small | plain | 2 of 2 | 95,482 | 84,688 to 106,277 | 1 | 14.0 / 14 | 6,820 |

Judging: 14 judging(s) of 14 run(s), 3,413,617 weighted tokens in all.

At the runner's list prices: 4.41 USD for the builds, 13.13 USD for the judging shown above (17.54 USD in all).

## Items that failed

- change-engine-1 **c01**: Reacting bot via --follow, seed 1 Normal: wave 6 sent 15 walkers, all kind runner, health 1, moving 3.0 units/s, but spawned 45 ticks apart (0.75 s), not 90 ticks (1.5 s). lib.rs RUNNER_SPAWN_EVERY = 45; CHANGES.md also says 'spawn every 0.75 s'.
- change-engine-2 **c01**: Bot-played full runs (leverage play . --follow, seed 1, Easy/Normal/Hard): after wave 5, wave 6 sends 15 kind=runner, 1 health at Normal, x 20 -> 19.95 per tick (3 u/s), but new runners appear every 45 ticks (0.75 s), not every 1.5 s (90 ticks). Gaps logged: [45, 45, ... x14]. lib.rs RUNNER_CD = 45.
- change-engine-3 **c01**: Played a full Normal game tick by tick via --follow (seed 1; also Easy and Hard runs): wave 6 sends 15 runners, kind runner, health 1 at Normal, speed 3.0 u/s, but they spawn 60 ticks (1.0 s) apart (spawn ticks 5288..6128, every gap 60), not 1.5 s. lib.rs: spawn_cd = if runner { 60 } else { 90 }.

## What each judge found beyond the list

- change-engine-1: Wave 6 runners spawn every 0.75 s (RUNNER_SPAWN_EVERY = 45 ticks), not every 1.5 s like the other waves.
- change-engine-1: README 'How to play' still says 'Each walker destroyed is 10 points' and 'wave 6's last walker'. It does not mention that runners give 15 (only the Changes section at the end does).
- change-engine-1: Left/Right on Language and Difficulty stop at the ends (no wrap), while Enter wraps. Not against the list, but it is uneven.
- change-engine-2: Wave 6 runners are sent every 0.75 s (RUNNER_CD 45 ticks) instead of every 1.5 s.
- change-engine-3: Wave 6 runners spawn every 1.0 s (60 ticks), not 1.5 s.
- change-engine-3: In --follow mode the first state printed (tick 0, screen play) has HUD texts with raw placeholders ('Score: {game.score}', 'Wave {work.wave} of 6') and null wave/score before the rules have ticked.
- change-engine-3: checks/smoke.json is still the engine sample's 'tap' playtest and cannot pass for this game (CHANGES.md says so).
- change-plain-1: README.md was not updated: it still says 'Win by clearing wave 5', lists the languages as English / Español, does not mention Difficulty, and its 'Rules as built' still says walkers have 2 HP.
- change-plain-1: The gap between waves is 181 ticks, not 180 (wave advances on one step, the first spawn comes the step after). This was already there before the change.
- change-plain-1: A walker at x exactly 0.0 is still shown for one tick before it hits the gate (tick 801). This was already there before the change.
- change-plain-2: README.md was not updated for the change: its Settings section still lists only Volume and Language, gives the settings JSON as {volume, language}, and does not mention difficulty, wave 6, runners or French (CHANGES.md admits this).
- change-plain-2: Difficulty wraps around on Left/Right (Hard -> Easy, Easy -> Hard), while Volume clamps. This is allowed by the brief but inconsistent.
- medium-engine-1: While Settings is open, texts also lists the title screen's words (Hold the gate, Start, Settings, Quit) under it; the frame shows only a faint title behind the panel (README admits it).
- medium-engine-1: On the title, Start keeps its bright blue 'primary' fill when the focus moves to another entry, so two entries look highlighted; only the thin ring marks the chosen one.
- medium-engine-1: Enter on Quit in a headless run leaves the screen as title with nothing reported, so Quit could not be seen to do anything headless.
- medium-engine-1: Pause has no Settings entry (the brief only asks for Resume, Title; this is noted, not a fault).
- medium-engine-1: Starting a second run from the title in the same session does not reseed: its walker lanes differ from the seed's first run (still deterministic for the same inputs).
- medium-engine-2: `texts` on the settings screen also lists the title screen's four strings, which are hidden behind the settings panel in the frame (not 'on screen now').
- medium-engine-2: After Title from pause (or Enter from won), the JSON on the title screen still reports the old run's score, walkers and bolts (README admits it).
- medium-engine-2: Guard at y=12 (or 0) is drawn half outside the field; at the top it sits under the HUD panel.
- medium-engine-2: On the title screen Start keeps its blue 'primary' fill when not focused, so two buttons look highlighted; only the focus ring marks the chosen one.
- medium-engine-2: README headless and frame commands as written write into the work folder (./saved, frame.png).
- medium-engine-3: The next wave starts 181 ticks (3.017 s) after the previous wave's last walker is gone, one tick more than 3 s.
- medium-engine-3: The guard's centre is clamped to y 0..12, so at either edge half of the guard is drawn outside the field.
- medium-engine-3: After returning to the title from a run, the report still shows that run's score, wave and gate_health (e.g. gate 0) instead of null.
- medium-engine-3: Quit on the title does nothing visible in the headless run (screen stays title); could not see it in a window.
- medium-engine-3: README commands put settings_dir and frame.png inside work/ (I redirected them to my scratch folder).
- medium-plain-1: A walker at x exactly 0.0 stays on the field one tick (tick 801) before it is removed and costs the gate (tick 802).
- medium-plain-1: On return to the title (from paused or won), the state still reports the old run's score, walkers and bolts (e.g. title at tick 405 lists 2 walkers and 4 bolts).
- medium-plain-1: The first walker of a new wave enters one tick after the wave number changes (wave 2 at tick 1342, walker at 1343).
- medium-plain-1: The Spanish end-screen string 'Enter: título' keeps the English key name 'Enter'.
- medium-plain-1: Volume is only stored and shown; the game has no sound (README admits this).
- medium-plain-2: After returning to the title from play/won, the JSON state keeps the old score, wave, walkers and bolts (e.g. score 450 on the title screen).
- medium-plain-2: The menu marker bar is drawn above the chosen entry, so it sits between two entries; only the gold colour shows the choice clearly.
- medium-plain-2: Volume is stored and shown but the game has no sound (README says so).
- medium-plain-2: W/S keys and the tkinter window were not testable headless (no window may be opened); the builder also says the window was never run.
- small-engine-1: Piece y builds up floating-point drift (e.g. 0.5000000000000349 when it should be 0.5), so a miss is counted one tick after the piece reaches the bottom edge. paddle_x shows the same drift (8.500000000000009).
- small-engine-1: The score/misses HUD panel in the top-left draws over the field and hides pieces in columns 0-2 near the top (over.png: piece at x2.5 y14.83 is mostly covered).
- small-engine-1: Restart is honoured at any time, not only after the end (the brief only describes restart after the end; harmless).
- small-engine-2: The state printed before any tick (--ticks 0, or the first line of --follow) has null score/misses/over/paddle_x/fall_speed/pieces instead of the initial values.
- small-plain-1: The 3x5 bitmap 'M' glyph is a solid block that reads more like 'H' ('MISSES LEFT' looks like 'HISSES LEFT').
- small-plain-1: The HUD text is drawn over the top of the field, so a newly spawned piece (y ~14.8-15.5) at the left columns is partly hidden behind the 'MISSES LEFT' line.
- small-plain-1: The restart tick spawns nothing (step returns after reset), so the first piece of a restarted run appears one tick after the restart tick, not on it.
- small-plain-1: A piece whose bottom is already below the paddle top (beside the paddle) is still caught if the paddle slides into it before it reaches y-0.5<=0, so pieces can be caught from the side.
- small-plain-1: Headless 'restart' works at any time, mid-run included, but the interactive game only restarts after Game over. This is documented in the README but differs from the brief's restart-after-end.
- small-plain-2: The game-over box (px 40-440, 200-400) is drawn over the playfield and hides any piece behind it (in my end frame, the piece at (8.5,7.34) is not visible).
- small-plain-2: Pieces at the top of the field are drawn over the HUD text (the piece at x=11.5 near the top covers part of 'MISSES LEFT 3').
- small-plain-2: The tick that applies restart only resets the run and does not step it, so the new run's first piece appears on the tick after the restart input, not on the restart tick. This is minor and a matter of interpretation.
- small-plain-2: The interactive tkinter play mode was not run, by the machine's rules (no windows). Its builder also says they never tested it.
