# Measured leverage: the runs

Folder: `lr2`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change runs: bbe9fa36
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium runs: bbe9fa36
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: bbe9fa36
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 305,298 | 0.55 | 50,689 | 1,405,604 | 20,279 | 38 | 37 | 19 | done | 167 | 163,499 / 72,711 | 8 / 9 | 1 / 1 | 2 | 1,059 |
| change-engine-2 | 425,285 | 0.74 | 64,386 | 2,073,247 | 27,500 | 45 | 46 | 41 | done | 241 | 450,931 / 108,428 | 8 / 9 | 1 / 1 | 4 | 1,039 |
| change-engine-3 | 445,466 | 0.77 | 66,417 | 2,194,743 | 28,599 | 49 | 48 | 10 | done | 71 | 33,846 / 23,409 | 7 / 9 | 1 / 1 | 5 | 1,107 |
| change-plain-1 | 101,174 | 0.22 | 25,254 | 160,763 | 10,707 | 9 | 10 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 647 |
| change-plain-2 | 97,497 | 0.21 | 24,502 | 205,797 | 9,259 | 11 | 12 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 573 |
| medium-engine-1 | 672,915 | 1.10 | 79,777 | 3,644,176 | 41,762 | 67 | 70 | 30 | done | 350 | 305,932 / 133,892 | 18 / 19 | 2 / 2 | 6 | 885 (584 at the start) |
| medium-engine-2 | 277,124 | 0.54 | 49,815 | 904,922 | 24,875 | 24 | 25 | 5 | done | 19 | 20,351 / 5,229 | 17 / 19 | 2 / 2 | 6 | 893 (584 at the start) |
| medium-engine-3 | 389,029 | 0.70 | 57,205 | 1,600,818 | 31,492 | 39 | 42 | 7 | done | 43 | 7,348 / 22,228 | 19 / 19 | 2 / 2 | 7 | 965 (584 at the start) |
| medium-plain-1 | 179,349 | 0.39 | 30,529 | 163,670 | 24,965 | 8 | 11 | 2 | done | - | - | 18 / 19 | 2 / 2 | 7 | 530 |
| medium-plain-2 | 163,197 | 0.35 | 28,700 | 174,469 | 21,976 | 9 | 12 | 2 | done | - | - | 19 / 19 | 2 / 2 | 5 | 465 |
| small-engine-1 | 178,372 | 0.37 | 39,338 | 447,721 | 16,887 | 16 | 20 | 3 | done | 10 | 493 / 1,800 | 14 / 14 | 2 / 2 | 3 | 685 (584 at the start) |
| small-engine-2 | 160,854 | 0.33 | 33,259 | 399,478 | 15,868 | 16 | 20 | 3 | done | 10 | 506 / 1,816 | 14 / 14 | 2 / 2 | 4 | 661 (584 at the start) |
| small-plain-1 | 90,971 | 0.20 | 16,477 | 92,434 | 12,227 | 7 | 7 | 1 | done | - | - | 14 / 14 | 2 / 2 | 4 | 275 |
| small-plain-2 | 103,396 | 0.22 | 17,742 | 139,076 | 13,463 | 9 | 9 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 280 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 3 of 3 | 392,016 | 305,298 to 445,466 | 23 | 7.7 / 9 | 51,133 |
| change | plain | 2 of 2 | 99,336 | 97,497 to 101,174 | 1 | 9.0 / 9 | 11,037 |
| medium | engine | 3 of 3 | 446,356 | 277,124 to 672,915 | 14 | 18.0 / 19 | 24,798 |
| medium | plain | 2 of 2 | 171,273 | 163,197 to 179,349 | 2 | 18.5 / 19 | 9,258 |
| small | engine | 2 of 2 | 169,613 | 160,854 to 178,372 | 3 | 14.0 / 14 | 12,115 |
| small | plain | 2 of 2 | 97,184 | 90,971 to 103,396 | 1 | 14.0 / 14 | 6,942 |

Judging: 14 judging(s) of 14 run(s), 4,023,082 weighted tokens in all.

At the runner's list prices: 6.69 USD for the builds, 15.09 USD for the judging shown above (21.79 USD in all).

## Items that failed

- change-engine-1 **c01**: Normal, seed 1, full bot run replayed in leverage play: wave 6 sends 15 runners (kind runner, health 1, 0.05 u/tick = 3 u/s; score 450->675 = 15x15). But they come 46 ticks apart (about 0.77 s), not 1.5 s: at tick 5180 runners are at x=17.6 (born 5132) and x=19.9 (born 5178). spawn_cd=45 in lib.rs.
- change-engine-2 **c01**: Runners are kind runner, 1 health at Normal, 3 u/s (x 19.9->17.9 in 40 ticks), and 15 spawn in wave 6 (15 lanes learned, 225 = 15x15 points). But they come one every 1 s, not 1.5 s: Easy seed 1, no shooting in wave 6, tick 6100 runners at x 7.9, 10.9, 13.9, 16.9, 19.9 (3.0 apart at 3 u/s = 60 ticks). lib.rs has RUNNER_EVERY = 60; CHANGES.md says so too.
- change-engine-3 **c01**: Win script, seed 1, Normal: wave 6 starts tick 5645; runners (kind runner, health 1, x falls 0.05/tick = 3 u/s) spawn at ticks 5646, 5706, 5766, 5826, so one every 60 ticks = 1.0 s, not 1.5 s. The 15 count is right (wave 6 adds 225 = 15 x 15). lib.rs: spawn_t += 1.0 for runners.
- change-engine-3 **c08**: m01-m18 all still pass (read with 'of 6' and 675). m19 does not: README's check command now names another folder (see m19).
- medium-engine-1 **m09**: Wave 1 sends 5 walkers in lanes from {2,4,6,8,10}, walking 0.025/tick (1.5 u/s), and no 6th; but spawns are 91 ticks apart, not 90: at tick 92 walker 1 is at x=17.725 (91 ticks of travel) when walker 2 appears at x=20; spacing 2.275 units at tick 400. Interval is 1.517 s (spawn_cd=90 counted down then spawn on the next tick).
- medium-engine-2 **m05**: Settings in Spanish: [Ajustes, Volumen: 8, Idioma: Español, Atrás], play [Puntos: 0, Oleada 1 de 5, Puerta: 5], pause [Pausa, Continuar, Título], lost [Has perdido, Puntos: 0, Enter: título]. But the Spanish title screen is [Hold the gate, Empezar, Ajustes, Salir]: the game's name stays in English (it comes from game.json {game.title}, not the string tables), so English is left on the title screen (also seen in es_title.png).
- medium-engine-2 **m10**: Hits usually work (bolts y=4 at walker y=4: health 2->1 at tick 98 and the bolt is gone; second hit on tick 120 removes it, score 10; y=6 bolts pass y=4/10 walkers). But some same-lane bolts pass straight through: '1:enter,2:down,22:none,39:shoot,40:none' -> tick 107 bolt x=17.63 y=4 has passed walker x=17.375 y=4 health 2, and the bolt leaves the field without a hit. The same happens for shoot at tick 49 (2 of 17 shot timings tried). The hit test only checks the bolt's own step and ignores the walker's movement in the same tick.
- medium-plain-1 **m10**: Normal case works (guard at y 2, bolts at ticks 41/65: walker health 2->1, bolt gone; second bolt kills, score 10; a lane-4 bolt passed a lane-2 walker). But a single bolt fired at tick 48 (seed 1, guard y 2.0) passes through the lane-2 walker: tick 113 bolt x 17.1667 / walker 17.175, tick 114 bolt 17.4 / walker 17.15 health 2; same for shots at ticks 58, 79, 89 (4 of 58 shot ticks tried).

## What each judge found beyond the list

- change-engine-1: Runner spawn interval is 46 ticks (0.767 s); even the builder's stated 0.75 s is off by one tick, the same off-by-one as walkers (91 ticks).
- change-engine-1: Left/Right on Language and Difficulty stop at the ends instead of wrapping (Enter wraps); not against the list, noted only.
- change-engine-2: Wave 6 runners spawn every 1 s (RUNNER_EVERY = 60) instead of every 1.5 s.
- change-engine-2: README 'Checks' still says checks/win-script.txt wins with seeds 1 and 2; a later README section says it no longer wins (it doesn't: wave 6 was added).
- change-engine-2: The bolt pass-through bug from m10 is still there (hit test ignores the walker's movement in the same tick); this failed before the change and is not held against it.
- change-engine-2: The Language setting stops at French and doesn't wrap back to English (step, not cycle); Left goes back.
- change-engine-3: README.md line 15 still names <runs>/medium-engine-3 as the folder; CHANGES.md says this was fixed, which is false.
- change-engine-3: README.md 'What I checked' still says the winning run scores 450 (now 675).
- change-engine-3: README says 'Run from anywhere' but its examples use paths relative to <runs>/change-engine-3 (work/frame.png, $(cat work/checks/win_inputs.txt)).
- change-engine-3: State 'difficulty' reads 'normal' on title/settings until a run starts, even when the setting is hard or easy (documented in CHANGES.md).
- change-engine-3: Module description in lib.rs still says 'five waves'.
- change-plain-1: README's example headless command uses examples/script_spanish_play.json, which was written for the old 3-row Settings menu; on the changed game it ends on the Settings screen in French (not Spanish play), so the example no longer shows what its name says, and examples/frame_play.png is stale (CHANGES.md mentions this).
- change-plain-1: Pre-existing bolt tunnelling (m10) is still present: walkers move before the bolt-sweep test, so a bolt can pass through a walker in its lane; with runners at 3 units/s the miss window per tick is larger.
- change-plain-1: Difficulty Left/Right wraps (Hard -> Easy) instead of stopping at the ends; the brief does not forbid it.
- change-plain-2: README.md 'What I checked' still reports the pre-change bot result (won, score 450, ~5240 ticks), not the 6-wave game.
- change-plain-2: Running the headless check without --settings writes settings.json into the game folder (the documented default); harmless but easy to leave behind.
- change-plain-2: Quit on the title does nothing in headless mode (pre-existing, noted by the builder).
- medium-engine-1: Walkers spawn every 91 ticks (1.517 s), not 90 (1.5 s): off by one in the spawn countdown.
- medium-engine-1: Without --seed the run does not match --seed 1 (first lanes 4,10,4,2,10 vs 2,4,8,6,6), so the brief's 'default 1' is not honoured.
- medium-engine-1: README's settings example (0:down,2:none,4:enter,8:right,12:escape) is said to switch to Spanish but actually sets volume 6 and leaves English (focus starts on Volume).
- medium-engine-1: On the settings and pause screens 'texts' also lists the strings of the screen beneath (title or HUD), which the frame does not show as such.
- medium-engine-1: After returning to the title from a run, the report still shows that run's score, wave, walkers and bolts (e.g. score 450 and 3 bolts on the title).
- medium-engine-1: The title frame before any key is pressed shows no focus mark on Start; the focus ring only appears after a navigation key.
- medium-engine-2: No --seed does not mean seed 1 as the brief says: a run with no seed gives lanes [4,10,4,2,10] and --seed 1 gives [2,4,8,6,6].
- medium-engine-2: The game's name 'Hold the gate' is not in the string tables and stays English in Spanish.
- medium-engine-2: Bolts can pass through a walker in their own lane, about 1 shot timing in 10 (the hit test only uses the bolt's step; the walker moves first).
- medium-engine-2: On the first title frame the focused Start has no focus ring. Start is always styled filled blue, so once another entry is chosen, two entries look highlighted.
- medium-engine-2: The README's win script wins only with 410/440 points, not with every walker destroyed. Its 'What I checked' does not show a 450 run.
- medium-engine-2: Guard can go to y=0 and y=12, so it is drawn half outside the field.
- medium-engine-3: Without --seed the game does not use seed 1 (the brief's default): an unseeded run's first walker is in lane 2, seed 1's is in lane 4. README admits it.
- medium-engine-3: The first spawn interval of each wave is 89 ticks, not 90 (spawn_t starts at 0 and the decrement happens before the spawn).
- medium-engine-3: The gap between waves is 181-182 ticks, not 180 (the wave counter flips on tick 1342 after the last walker is gone on 1161).
- medium-engine-3: Left on Language when English is chosen does nothing (no wrap-around); only Right switches to Spanish there.
- medium-engine-3: On the title screen before any key is pressed, Start (index 0) has no focus ring; it looks the same as when it is not chosen (it is always the blue primary button), so with Settings chosen two buttons look emphasised.
- medium-engine-3: Walkers and bolts stay in the reported state after the run ends (won: 2 bolts; lost: 6 walkers listed).
- medium-engine-3: Volume is stored only; there is no sound.
- medium-plain-1: Bolt-walker hit test misses ~7% of shots: the walker moves first, then the test needs old_bolt_x <= walker_x <= new_bolt_x, so a walker that slips just behind the bolt's old x in that tick is never hit (bolt passes through in its lane).
- medium-plain-1: Won/lost and paused frames draw the HUD (score, wave, gate); paused texts list those strings, but won/lost 'texts' leave them out.
- medium-plain-1: Bolts still in the state (and the frame) on the won screen, frozen.
- medium-plain-1: Wave number changes to the next wave one tick (179 ticks) before that wave's first walker spawns (180 ticks).
- medium-plain-1: The tkinter window title 'Hold the gate' is hard-coded in the code, not taken from the string tables (the window is only used in play mode).
- medium-plain-1: Weak seed mixing: seeds 1, 2 and 7 all put the first walker in lane 2.
- medium-plain-1: README's headless command uses relative paths and writes into work/examples, but it doesn't say to run it from work/.
- medium-plain-2: Won/Lost screens draw their text straight over the field with no backdrop; in a lost frame walkers overlap 'YOU LOST' and 'ENTER: TITLE'.
- medium-plain-2: At y = 0 or 12 the guard is drawn half outside the 800x480 frame (its centre is on the field edge).
- medium-plain-2: The tkinter window title 'Hold the gate' is written in the code, not taken from the string tables, so it stays English in Spanish.
- medium-plain-2: Running the game as documented writes __pycache__ into the game folder (font.py import); I deleted the one my runs created.
- medium-plain-2: Quit on the title does nothing visible in a headless run (the screen stays title); the README says so.
- small-engine-1: With no --seed the game does not use seed 1 as the brief's default: columns are 10.5,5.5,3.5,0.5 without --seed but 3.5,2.5,3.5,2.5 with --seed 1.
- small-engine-1: The score/misses HUD panel (top-left, about 155x30 px) covers part of the top row of the field, so a new piece in columns 0-3 is partly hidden for its first few ticks.
- small-engine-1: README says the script word 'enter' does not restart; only the module word 'restart' does (the brief's script words are left/right/none/restart, so the checks are not affected). The Enter key in a real window was not tried by the builder.
- small-engine-2: A piece whose edge exactly meets the paddle's edge (e.g. piece x3.5 vs paddle 0..3) is counted as a miss: the overlap test is strict (<), so edge contact is not 'touching'.
- small-engine-2: The HUD panel in the top-left covers part of the field, hiding pieces that spawn in columns 0-2 near the top (visible in over.png/frame.png).
- small-engine-2: checks/smoke.json is the sample's leftover check (start/tap actions, outcome 'won') which no longer matches the game; unused sample screens (title, pause, settings, won, lost) and strings remain in assets.
- small-engine-2: The README's frame command writes frame.png into whatever folder it is run from (its stated folder is the one above work), i.e. outside the work folder.
- small-plain-1: Game-over frame: the black overlay box (y 200-400 px) hides any frozen piece behind it (piece at y 10.03 not visible in the over frame).
- small-plain-1: Frame text is all caps ('GAME OVER', 'MISSES LEFT') from a built-in pixel font, not the brief's 'Game over' casing; the tkinter window uses 'Game over'.
- small-plain-1: From reading code (not observed): in the tick of the third miss, pieces later in the list are still processed, so a simultaneous catch could add score in the same tick the run ends.
- small-plain-1: Interactive tkinter window not exercised (judge rules forbid opening a window); the builder also says it was never opened.
- small-plain-2: A miss comes one tick late on the exact boundary because of float build-up: the piece's bottom reaches 0 (y=0.5) at tick 226 but the miss only counts at tick 227.
- small-plain-2: The HUD text is drawn over the top of the field, so a newly spawned piece near the top (y around 14.8-15.5) is partly hidden behind 'MISSES LEFT'.
- small-plain-2: The interactive tkinter window was not checked by the builder (README says so), and the judge could not check it under the rules; only the headless path is verified.
