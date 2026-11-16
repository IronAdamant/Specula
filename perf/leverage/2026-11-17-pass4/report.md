# Measured leverage: the runs

Folder: `lr4`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change runs: d18e1131
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium runs: d18e1131
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: d18e1131
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 131,562 | 0.28 | 33,384 | 377,197 | 10,424 | 15 | 14 | 4 | done | 10 | 594 / 1,795 | 8 / 9 | 1 / 1 | 3 | 1,125 |
| change-engine-2 | 176,737 | 0.34 | 36,604 | 709,787 | 12,003 | 24 | 23 | 8 | done | 15 | 483 / 8,787 | 9 / 9 | 1 / 1 | 3 | 1,218 |
| change-engine-3 | 127,741 | 0.28 | 37,619 | 348,038 | 9,184 | 13 | 12 | 6 | done | 9 | 277 / 4,782 | 7 / 9 | 1 / 1 | 5 | 1,313 |
| change-plain-1 | 82,693 | 0.18 | 22,774 | 148,850 | 7,869 | 9 | 10 | 1 | done | - | - | 8 / 9 | 1 / 1 | 2 | 554 |
| change-plain-2 | 112,991 | 0.24 | 26,166 | 240,146 | 11,255 | 12 | 14 | 1 | done | - | - | 9 / 9 | 1 / 1 | 2 | 583 |
| medium-engine-1 | 205,269 | 0.43 | 41,420 | 454,267 | 21,615 | 15 | 24 | 4 | done | 14 | 953 / 3,195 | 19 / 19 | 2 / 2 | 4 | 851 (662 at the start) |
| medium-engine-2 | 208,160 | 0.42 | 42,628 | 553,684 | 19,903 | 17 | 18 | 4 | done | 9 | 401 / 2,988 | 19 / 19 | 2 / 2 | 6 | 986 (662 at the start) |
| medium-engine-3 | 207,743 | 0.43 | 42,817 | 519,548 | 20,455 | 16 | 17 | 4 | done | 14 | 746 / 3,270 | 19 / 19 | 2 / 2 | 4 | 1,001 (662 at the start) |
| medium-plain-1 | 184,304 | 0.40 | 32,184 | 203,186 | 24,752 | 9 | 14 | 2 | done | - | - | 17 / 19 | 2 / 2 | 5 | 500 |
| medium-plain-2 | 195,313 | 0.41 | 31,985 | 314,629 | 24,775 | 13 | 14 | 3 | done | - | - | 18 / 19 | 2 / 2 | 4 | 534 |
| small-engine-1 | 196,269 | 0.40 | 42,062 | 544,354 | 17,853 | 17 | 22 | 5 | done | 16 | 398 / 3,942 | 14 / 14 | 2 / 2 | 3 | 584 (662 at the start) |
| small-engine-2 | 221,450 | 0.42 | 38,078 | 776,802 | 19,237 | 25 | 27 | 5 | done | 18 | 524 / 4,167 | 13 / 14 | 2 / 2 | 2 | 842 (662 at the start) |
| small-plain-1 | 100,291 | 0.22 | 18,007 | 116,766 | 13,222 | 8 | 9 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 268 |
| small-plain-2 | 89,050 | 0.19 | 16,280 | 92,285 | 11,895 | 7 | 7 | 1 | done | - | - | 14 / 14 | 2 / 2 | 4 | 255 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 3 of 3 | 145,347 | 127,741 to 176,737 | 6 | 8.0 / 9 | 18,168 |
| change | plain | 2 of 2 | 97,842 | 82,693 to 112,991 | 1 | 8.5 / 9 | 11,511 |
| medium | engine | 3 of 3 | 207,057 | 205,269 to 208,160 | 4 | 19.0 / 19 | 10,898 |
| medium | plain | 2 of 2 | 189,808 | 184,304 to 195,313 | 3 | 17.5 / 19 | 10,846 |
| small | engine | 2 of 2 | 208,860 | 196,269 to 221,450 | 5 | 13.5 / 14 | 15,471 |
| small | plain | 2 of 2 | 94,670 | 89,050 to 100,291 | 1 | 14.0 / 14 | 6,762 |

Judging: 14 judging(s) of 14 run(s), 3,689,451 weighted tokens in all.

At the runner's list prices: 4.63 USD for the builds, 14.24 USD for the judging shown above (18.87 USD in all).

## Items that failed

- change-engine-1 **c01**: Own --follow bot (judge-scratch/jbot.py, seeds 1/7/3, Easy/Normal/Hard): wave 6 sends 15 runners, kind runner, health 1 at Normal, speed 3.0 u/s, but spawn ticks 5564, 5609, 5654 ... gaps all 45 ticks = 0.75 s, not 1.5 s (RUNNER_GAP = 45 in modules/work/src/lib.rs; CHANGES.md says 0.75 s).
- change-engine-3 **c01**: Bot win (seed 5) replayed with play --ticks 5470/5515/5560: wave 6 sends 15 runners, kind runner, health 1, 0.05 units a tick (3 u/s), but new runners appear at ticks 5470, 5515, 5560: one every 45 ticks = 0.75 s, not 1.5 s (lib.rs sets spawn_cd 44 for runners). Same 45-tick gap in all four full runs.
- change-engine-3 **c08**: m01-m18 still pass on the changed game (see m-lines). m19 does not: README's headless-run, frame and check commands still name <runs>/medium-engine-3/work, which is the game before the change, so as written they do not run this game.
- change-plain-1 **c01**: Bot run seed 1 replayed via CLI: wave 6 sends 15 runners (14 killed + last let through -> won; full clear 450+225), kind 'runner', health 1 at Normal, x 19.95 -> 19.9 per tick = 3 u/s. But spawns are 91 ticks apart (1.517 s), not 1.5 s: runners appear at ticks 5801, 5892, ... (same off-by-one the old judge failed m09 for; trace shows 91 in every wave).
- medium-plain-1 **m09**: Seed 1, no input: 5 walkers in lanes 4,10,2,6,2 at 1.5 u/s (x drops 0.025/tick), but they spawn every 91 ticks (1.517 s), not 90: walker counts change after ticks 1, 92, 183, ..., 365; gap in x between walkers 2.275 not 2.25. Bot run confirms 91-tick gap in all waves.
- medium-plain-1 **m12**: Bot run seed 1: wave 1's last walker gone in tick 527 (wave becomes 2, HUD 'Wave 2 of 5'), first wave-2 walker spawned in tick 708 = 181 ticks (3.017 s) later, not 180; wave 2 sends 7 walkers. Wave count and HUD right, delay off by one tick.
- medium-plain-2 **m10**: Normal hits work (health 2->1 with the bolt gone; second hit removes the walker, score +10; a lane-6 bolt passes the lane-4 walker and leaves past x=20). But bolts sometimes pass through a walker in their own lane: inputs 0:enter,1:up,21:none,29:shoot,30:none (guard_y 4.0, walker y 4.0): tick 95 bolt x 17.4 / walker 17.65, tick 96 bolt 17.633 / walker 17.625, then the bolt flies off and the walker keeps health 2. This happened for 7 of 120 shot timings I tried.
- small-engine-2 **s14**: README says how to play (keys, scoring, Enter to restart) and the frame command works, but the headless-run command as written fails: `{"error":"usage","message":"'--inputs': the input 'restart' is at tick 420, after the run's last tick (300): it would never be given"}`.

## What each judge found beyond the list

- change-engine-1: Runners spawn every 0.75 s; the builder chose this where the brief gave no spacing, but the change list asks for 1.5 s.
- change-engine-1: Walker colour marks 'hurt' by health < 2 (lib.rs), so on Easy every walker looks hurt from the start, and on Hard a 3-health walker looks the same as a 2-health one (the builder noted this in CHANGES.md and left it).
- change-engine-1: Spanish end-screen button still reads 'Enter: título' (English key name), as before the change.
- change-engine-2: README.md body is partly stale: says 'Each walker has 2 health' and 'Destroyed walker = 10 points' and that volume and language are kept, without the difficulty/runner details (only the appended Changes section has them).
- change-engine-2: README.md window command line is garbled ('specula play ... --as-is (from examen.exe leverage ... form as below)').
- change-engine-2: Right on Hard / Left on Easy do not wrap while Enter cycles; harmless, but undocumented outside CHANGES.md.
- change-engine-3: Wave 6 runners spawn every 45 ticks (0.75 s), not every 1.5 s.
- change-engine-3: README.md still points every command at <runs>/medium-engine-3/work, the game before the change. CHANGES.md notes this as 'found wrong' but it was left as is.
- change-engine-3: README 'What I checked' still says the bot won 'with score 450', and its list of 20 checks describes the old 11.
- change-engine-3: CHANGES.md says runners spawn 'every 44 ticks'. The real gap is 45 ticks.
- change-engine-3: ui.texts on the settings screen still include the title screen's texts beneath it (known before the change).
- change-plain-1: Spawn interval is 91 ticks (1.517 s) for walkers and runners and the wave gap is 181 ticks; inherited, but CHANGES.md says nothing else broken was found.
- change-plain-1: HUD: in French/Spanish the wave text nearly touches the gate text ('VAGUE 6 SUR 6 PORTE: 5', about 16 px apart).
- change-plain-2: A bolt can still pass straight through an enemy in its own lane (hit test ignores that the target moves after the bolt). With runners at 3 u/s it happened for 8 of 48 single-shot timings (tick 5843: bolt and runner both at x 16.7, no hit).
- change-plain-2: README.md is partly stale: it says `language` is "English"/"Spanish" (French is missing) and gives the default settings file without "difficulty".
- medium-engine-1: `texts` holds only the top screen's strings: on the pause screen the HUD (score, wave, gate) still drawn behind is not listed, and on Settings the title behind it is not listed, though the brief says every string on screen.
- medium-engine-1: After leaving a run (Title from pause, Enter on lost/won) the title screen's report still carries the old run's values (e.g. gate_health 0, score 450, old walkers/bolts) until Start is chosen.
- medium-engine-1: Volume has no effect: the game plays no sound (README says so).
- medium-engine-1: Enter on Quit in the headless run leaves the screen on title with nothing changed; cannot tell from headless whether it quits the window.
- medium-engine-2: After Title from the pause menu, the state printed on the title screen still lists the abandoned run's walkers (stale play state until Start resets it).
- medium-engine-2: On the won screen the state still lists bolts in flight (they are frozen there).
- medium-engine-2: The Start button keeps its blue 'primary' style when not focused, so with Settings focused two title buttons look highlighted; the focus is only the outline.
- medium-engine-2: The guard's centre is clamped to y 0..12, so at the edges half its body is drawn outside the field.
- medium-engine-2: On the settings screen 'texts' also lists the title screen's four strings underneath it (they are mostly hidden behind the settings panel in the frame).
- medium-engine-2: The first walker of a new wave appears 181 ticks after the previous wave emptied (wave number changes at 180); one tick late, within tolerance.
- medium-engine-3: Bolt and walker positions are rounded to 0.001 when stored each tick, so a bolt advances 0.233 a tick (13.98 u/s instead of 14): about 0.03 units behind after a full crossing.
- medium-engine-3: On the Spanish title frame, Empezar keeps its blue 'primary' fill while the focus ring is on Ajustes, so two entries look highlighted. The unfocused Start looks highlighted in English too.
- medium-engine-3: After returning to the title, the printed score/wave/gate_health/walkers still hold the last run's values (e.g. score 450, wave 5 on the title).
- medium-engine-3: The first walker of each new wave arrives 1 tick (1/60 s) after the 3-second gap, not exactly at 3 s.
- medium-plain-1: Spawn and wave timers count one tick too many (timer set to 90/180 then decremented before the next spawn), so walkers come every 91 ticks and waves 181 ticks apart.
- medium-plain-1: After returning to the title (from won/lost or pause), the state still reports the old round's score, wave, gate_health, guard_y, walkers and bolts (e.g. title after a win shows score 450, wave 5, 2 bolts).
- medium-plain-1: In Spanish the play HUD texts nearly run together ('OLEADA 1 DE 5 PUERTA: 5' with one space between them) because the three items are placed at fixed 250 px steps.
- medium-plain-1: Default settings location is settings.json beside the script, so a plain run writes into the game folder.
- medium-plain-1: The tkinter window mode is untested by the builder (README says so); not checked by the judge either (no windows allowed).
- medium-plain-2: Bolt and walker collision is checked only against the walker's position before it moves, so a bolt can slip past a same-lane walker between two steps (about 6% of shot timings in my test).
- medium-plain-2: On the lost screen the state still lists the walkers left on the field (harmless, but stale).
- medium-plain-2: Volume is stored and shown but does nothing (no sound), as README admits.
- medium-plain-2: The first walker of wave 2+ appears 181 ticks after the previous wave empties (wave number changes at 180).
- small-engine-1: paddle_x carries floating-point drift (8.500000000000009, 3.9999999999999996, 0.49999999999999956) instead of clean values; the builder's own check pins the drifted value with exact equality.
- small-engine-1: The README's frame command writes frame.png into the game's own folder.
- small-engine-1: Game over is kept only in the module's state (game.json has no won/lost outcome), so the engine's own phase never becomes 'lost'; the brief does not require it.
- small-engine-2: README headless-run example is invalid as written (restart input at tick 420 with --ticks 300); the engine rejects it with a usage error.
- small-engine-2: README frame example writes checks/out/over.png relative to the current folder, so run from work/ it overwrites the builder's own check frame.
- small-plain-1: At the third miss all pieces still falling are removed from the state and the frame (pieces becomes []), so the end frame shows no pieces; the brief does not ask for this.
- small-plain-1: Headless 'restart' given while the run is not over is silently ignored (documented in README), and it also resets the held input to none.
- small-plain-1: The interactive tkinter window was not exercised by me (rules forbid opening a window); the README says it was never tested by the builder either.
- small-plain-2: A piece whose bottom is exactly on the bottom edge (y=0.5 printed at tick 226, seed 1) is counted as a miss only one tick later. The fall is added up in floating point, so the y=0.5 compare misses by a tiny amount.
- small-plain-2: At the end of a run, the pieces still falling are removed from the state and the frame (a design choice; the brief does not say what should happen to them).
- small-plain-2: The game-over text in the PNG is upper case ('GAME OVER', 'ENTER TO RESTART'), not the brief's 'Game over' wording. The window code has 'Game over' but it was not opened.
- small-plain-2: The interactive window (tkinter) was not run, so the keyboard controls and frame-rate independence in play were not seen.
