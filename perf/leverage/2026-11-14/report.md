# Measured leverage: the runs

Folder: `m1`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change runs: 706837ff (round 2: tools NAME, refusals list arguments)
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium runs: 706837ff (round 2: tools NAME, refusals list arguments)
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: b0794cc2 (round 1)
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: 706837ff (round 2: tools NAME, refusals list arguments)

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 158,685 | 0.36 | 46,790 | 311,328 | 13,814 | 11 | 13 | 10 | done | 3,478 | 273,766 / 1,665,389 | 9 / 9 | 1 / 1 | 3 | 1,366 |
| change-engine-2 | 204,071 | 0.44 | 51,660 | 418,825 | 19,524 | 13 | 16 | 3 | done | 33 | 38,681 / 91,658 | 9 / 9 | 1 / 1 | 3 | 1,335 |
| change-plain-1 | 110,637 | 0.24 | 26,097 | 214,061 | 11,323 | 11 | 12 | 2 | done | - | - | 9 / 9 | 1 / 1 | 3 | 642 |
| change-plain-2 | 108,795 | 0.24 | 28,985 | 197,639 | 10,561 | 10 | 12 | 1 | done | - | - | 8 / 9 | 1 / 1 | 3 | 774 |
| medium-engine-1 | 616,657 | 1.19 | 106,487 | 2,059,997 | 55,513 | 32 | 38 | 18 | done | 4,606 | 366,938 / 2,168,460 | 17 / 19 | 2 / 2 | 8 | 1,245 |
| medium-engine-2 | 548,514 | 1.05 | 97,597 | 1,932,878 | 46,649 | 31 | 39 | 6 | done | 63 | 19,184 / 208,966 | 19 / 19 | 2 / 2 | 6 | 1,191 |
| medium-plain-1 | 177,732 | 0.38 | 30,520 | 168,008 | 24,557 | 8 | 10 | 3 | done | - | - | 18 / 19 | 2 / 2 | 4 | 518 |
| medium-plain-2 | 196,435 | 0.42 | 33,034 | 233,777 | 26,354 | 10 | 13 | 3 | done | - | - | 19 / 19 | 2 / 2 | 5 | 653 |
| small-engine-1 | 428,114 | 0.82 | 80,957 | 1,564,073 | 34,105 | 30 | 36 | 5 | done | 85 | 3,745 / 121,667 | 14 / 14 | 2 / 2 | 4 | 698 |
| small-engine-2 | 525,279 | 0.97 | 96,732 | 2,292,480 | 35,027 | 38 | 39 | 6 | done | 51 | 2,338 / 63,632 | 14 / 14 | 2 / 2 | 4 | 785 |
| small-engine-3 | 667,209 | 1.22 | 109,283 | 2,753,596 | 51,053 | 39 | 50 | 7 | done (past the token cap) | 81 | 3,683 / 128,893 | 14 / 14 | 2 / 2 | 3 | 805 |
| small-engine-4 | 412,794 | 0.80 | 83,633 | 1,508,012 | 31,493 | 27 | 33 | 5 | done | 41 | 1,820 / 76,903 | 14 / 14 | 2 / 2 | 4 | 736 |
| small-plain-1 | 95,994 | 0.21 | 17,466 | 114,555 | 12,542 | 8 | 8 | 1 | done | - | - | 14 / 14 | 2 / 2 | 4 | 241 |
| small-plain-2 | 97,986 | 0.21 | 17,642 | 116,420 | 12,859 | 8 | 8 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 256 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 2 of 2 | 181,378 | 158,685 to 204,071 | 6 | 9.0 / 9 | 20,153 |
| change | plain | 2 of 2 | 109,716 | 108,795 to 110,637 | 1 | 8.5 / 9 | 12,908 |
| medium | engine | 2 of 2 | 582,586 | 548,514 to 616,657 | 12 | 18.0 / 19 | 32,366 |
| medium | plain | 2 of 2 | 187,084 | 177,732 to 196,435 | 3 | 18.5 / 19 | 10,113 |
| small | engine, build 706837ff (round 2: tools NAME, refusals list arguments) | 2 of 2 | 540,002 | 412,794 to 667,209 | 6 | 14.0 / 14 | 38,572 |
| small | engine, build b0794cc2 (round 1) | 2 of 2 | 476,696 | 428,114 to 525,279 | 5 | 14.0 / 14 | 34,050 |
| small | plain | 2 of 2 | 96,990 | 95,994 to 97,986 | 1 | 14.0 / 14 | 6,928 |

Judging: 18 judging(s) of 14 run(s), 4,879,909 weighted tokens in all (4 of them earlier judgings of runs judged again).

At the runner's list prices: 8.55 USD for the builds, 14.15 USD for the judging shown above, and 5.01 USD for the earlier judgings of the runs judged again (27.70 USD in all).

## Judged more than once

The figures above are the last judging. Before it:

- change-engine-1: 8 / 9 (failed: c02). Judged again because: The first judging held c08 without the game's own verdict from before the change and read c02 three ways (review R1 of the comparison's lane): judged again with the list of what passed before the change, and with c02 saying what the brief left open.
- change-engine-2: 8 / 9 (failed: c02). Judged again because: The first judging held c08 without the game's own verdict from before the change and read c02 three ways (review R1 of the comparison's lane): judged again with the list of what passed before the change, and with c02 saying what the brief left open.
- change-plain-1: 7 / 9 (failed: c02, c08). Judged again because: The first judging held c08 without the game's own verdict from before the change and read c02 three ways (review R1 of the comparison's lane): judged again with the list of what passed before the change, and with c02 saying what the brief left open.
- change-plain-2: 8 / 9 (failed: c01). Judged again because: The first judging held c08 without the game's own verdict from before the change and read c02 three ways (review R1 of the comparison's lane): judged again with the list of what passed before the change, and with c02 saying what the brief left open.

## Items that failed

- change-plain-2 **c01**: Normal, seed 1, planned full-clear script run through the CLI: wave 6 starts at tick 5384; states sampled every 20 ticks give 15 runners (kind runner, health 1, x 19.8 -> 19.75 per tick = 3 u/s) spawned at ticks 5385, 5445, ..., 6225, every 60 ticks = 1.0 s, not 1.5 s (game.py:239 spawn_cd = 60 if runners).
- medium-engine-1 **m12**: Traced my winning run 1 step at a time. Wave 1's last walker was gone at tick 514, and wave 2's first walker (wave becomes 2) came at tick 695. That is 181 steps, 3.017 s, not 3 s (180 steps). Waves 3, 4 and 5 were also 181 steps late. The code sets wave_wait=180 in the same step, then counts 181 steps down. The rest of the item passed: 7 walkers in wave 2 (then 9, 11, 13), and texts showed 'Wave 2 of 5'.
- medium-engine-1 **m16**: Play frame at tick 200 (guard y=5): gate bar and 5 health bricks, guard at (80,280) px, walkers and bolts at x*40 and 480-y*40 exactly as the state says, HUD 'SCORE: 0 / WAVE 1 OF 5 / GATE: 5'. But a play frame with guard_y=12.0 (up held to the top edge, which the game allows) shows no guard at all. frame.py draws it at pixel y -16..16, under the HUD bar.
- medium-plain-1 **m10**: Hits usually work: with one shot at tick 1, the lane-6 walker went 2->1 and the bolt was gone; a 2nd bolt killed it (score 10); bolts from y=5 and y=6.7 passed it at health 2. But a bolt can tunnel through a walker in its own lane. With seed 1 and '0:enter,9:shoot,10:none', at run 78 the bolt is at x=18.1 and the walker at x=18.1 (y 6), and at run 79 the bolt is at 18.333 and the walker at 18.075, still health 2. 5 of 59 single-shot timings (ticks 9, 19, 29, 40, 50) pass through. Cause: the sweep checks [old x, new x] after the walker has moved, so a walker just ahead of the bolt slips behind it.

## What each judge found beyond the list

- change-engine-1: tools/headless.py decodes the engine's output with the Windows code page (subprocess text=True, no encoding). If PYTHONIOENCODING=utf-8 is set, accented strings come out double-encoded in the printed state and in the frame ('ParamÃ¨tres', drawn as 'PARAMÃ?TRES'). It works in a plain environment.
- change-engine-1: The wave gap is still 181 steps, not 180, the bug already reported under m12. It also applies to wave 6: wave 5's last walker was gone at tick 4941 and wave 6 began at tick 5122.
- change-engine-1: README.md is partly stale: 'a bot plays all 5 waves to a win (450 points)', 'Wave 5 is won when its last walker is gone', and '--settings where volume and language are kept' (it now keeps difficulty too).
- change-engine-2: Frame: with the guard at y=12 (top edge), the guard and its bolts are drawn mostly under the HUD bar and are hard to see. This looks inherited from before the change.
- change-engine-2: Frame font: the acute accent (É) is drawn as a single dot above the letter, so it reads like Ė (cosmetic).
- change-engine-2: hold.py's default --settings writes settings.json inside the game folder next to hold.py (documented, but a run without --settings leaves a file in work/).
- change-plain-1: The pre-existing bolt tunnelling (m10) remains and is easy to see with runners: seed 1 Hard --ticks 6400 has a bolt at x=14.83 and a runner at x=14.6 in the same lane (y=4), runner still at health 2. CHANGES.md does not list it among the bugs found before the change.
- change-plain-1: README's example uses --settings /tmp/s.json, which on Windows writes to C:\tmp; and with no --settings the game writes ~/.hold_the_gate/settings.json (I always passed a scratch path).
- change-plain-1: By its own CHANGES.md, the builder wrote scratch files to /tmp outside the work folder, which the brief forbade.
- change-plain-2: Wave 6 runners spawn every 1.0 s (60 ticks), not every 1.5 s.
- change-plain-2: README 'What I checked' is stale: it still says the bot 'wins all 5 waves with score 450', and the Settings section mentions only volume and language as saved (the JSON example includes difficulty).
- change-plain-2: The wave gap between the last walker leaving and the next wave is 179 ticks between states (180 counting the leaving tick), so it is at the edge of 3 s.
- medium-engine-1: The wave gap is 181 fixed steps instead of 180 (off by one step on every wave transition).
- medium-engine-1: Frame: the guard can move to y=12 (and y>~11.2); there it is hidden under the HUD bar, partly or fully. At y=0 half of it is drawn off the bottom.
- medium-engine-1: Frame: the Spanish game name 'Defiende la puerta' is wider than 800 px at scale 8 and is cut off at the left and right edges of the title frame.
- medium-engine-1: The engine window draws no text at all (no menus or HUD), as the builder admits. The game is only fully seen through the Python frame, which is drawn by tools/frame.py, not by the engine.
- medium-engine-1: In the window path, settings are kept only in engine memory; only the headless command writes the settings file.
- medium-engine-1: Quit on the title screen does nothing visible (it only emits an event).
- medium-engine-1: headless.py: if the --script path does not exist, it treats the path as script text and crashes with IndexError instead of saying the file is missing.
- medium-engine-1: Spanish 'Enter: título' keeps the English key name 'Enter' (Spanish keyboards often say 'Intro'). Minor.
- medium-engine-2: There is no interactive play at all: the game exists only as the headless run plus a software-drawn frame. The W/S keys in the brief exist nowhere, and the headless script rejects 'w'.
- medium-engine-2: Quit on the title only sets `quit: true` in the state; nothing quits.
- medium-engine-2: Volume and language are kept by the driver script hold.py writing a JSON file after the run. The game module itself does not keep them.
- medium-engine-2: The Spanish strings keep the English key name 'Enter' ('Enter: elegir', 'Enter: título').
- medium-engine-2: On won, lost and title the state still reports the leftover walkers, bolts, score and guard_y of the last play (for example, title after a win shows score 450 and 3 bolts).
- medium-engine-2: The default --settings path is settings.json inside the work folder, so running the README commands as written writes into the game folder.
- medium-plain-1: The run is declared won when wave 5's last walker reaches the gate (gate still >0), not only when it is destroyed. My bot that let the last walker through ended on 'won' with 440 points and gate 4, but the brief says won when wave 5's last walker is destroyed.
- medium-plain-1: A bolt can pass through a walker in its own lane without hitting it (about 1 shot timing in 12); this is the cause of the m10 failure.
- medium-plain-1: The game name is translated on the Spanish title ('Defiende la puerta') and the Spanish 'Enter: título' keeps the English key name 'Enter'. Both are arguable rather than wrong.
- medium-plain-1: Without --settings the game writes to ~/.hold_the_gate/settings.json; the README example's /tmp/s.json path is POSIX-style on a Windows machine. Neither affected checking.
- medium-plain-2: The run counts as won when wave 5's last walker reaches the gate rather than being destroyed (gate still standing): a seed 1 run that let that walker through ended won with 440 points and gate 4. The brief says the run is won when that walker is destroyed.
- medium-plain-2: On the won/lost frames the top HUD bar repeats the result texts (e.g. 'YOU LOST / SCORE: 0 / ENTER: TITLE') instead of score/wave/gate.
- medium-plain-2: Spanish 'Enter: título' keeps the English key name 'Enter' on the won/lost screens.
- medium-plain-2: After returning to the title from pause or won/lost, the printed state still carries the old score, wave, walkers and bolts until Start is chosen again.
- medium-plain-2: The tkinter window title 'Hold the gate' is hard-coded in game.py and not taken from the string tables (from reading the code; I did not open the window).
- small-engine-1: Pieces are drawn on top of the game-over text and the HUD bar. In the seed 3 over-frame a piece covers part of the word 'Game'.
- small-engine-1: Each headless run makes the engine write capture files under work/captures and work/.specula-cache. The checks leave files in the game folder.
- small-engine-1: The paddle and pieces are drawn in dimmer colours than the code sets (the README notes the engine's tone mapping). This is cosmetic only.
- small-engine-1: Windowed play with real keys (A/D, Enter) is untested by the builder and by me. The live tick path does do 60/tick_hz steps, so it only matches 1/60 steps when tick_hz divides 60.
- small-engine-2: In the game-over frame the frozen pieces stay drawn and overlap the text: a piece covers part of 'Game over' and another covers 'Misses left 0' in over.png. The README admits this.
- small-engine-2: A miss lands one tick late because of floating-point build-up: at tick 226 the state prints y 0.5 (bottom exactly on the bottom edge) but the miss counts only at tick 227.
- small-engine-2: The frame check leaves a new work/captures folder (written by the engine's sight_frame) and uses work/.catch-tmp during the run.
- small-engine-2: Live play with a window and keys is untested by the builder, and I did not check it either (no window allowed).
- small-engine-3: How to play in a window is vague ('open this folder as a project in Specula, start the clock'), and the builder says they never tried it. Only the headless path was verified.
- small-engine-3: Float rounding: a piece at exactly y = 0.5 (bottom on the bottom edge) is still in play for one tick and counted as a miss one tick later.
- small-engine-3: Cosmetic: a piece near the top can be drawn over the 'Score' text (seen in the seed-7 frame).
- small-engine-4: After the end the frozen pieces stay drawn and can overlap the 'Game over' title and the HUD (seen in over800.png: a piece behind the 'G', another under 'Misses left'). Cosmetic, and the README says so.
- small-engine-4: Each check command starts and then stops the engine for the project unless --keep-engine is given, so it would stop an engine someone else had running for it.
- small-engine-4: catch.py writes a temporary .catch_args.json inside work/ on every call (it removes it afterwards).
- small-engine-4: Playing in a window (keys, Enter restart, 1/60 s stepping at the engine's tick rate) was not checked: no window was allowed. The builder says the same.
- small-plain-1: The game-over frame's restart hint says only 'PRESS ENTER', without the word restart. The tkinter window says 'Press Enter to restart'.
- small-plain-1: At game over the pieces still falling are wiped off the field, so the end frame shows no pieces.
- small-plain-1: The README's first example command uses '300:restart' while the run is still in play, so that restart does nothing. This is documented, but it is a confusing example.
- small-plain-1: The interactive tkinter window is untested by the builder and by me, because I may not open a window here.
- small-plain-2: The 5x3 pixel-font 'M' (rows 101/111/111/101/101) reads more like 'N'/'H', so the frame's 'MISSES LEFT' looks like 'NISSES LEFT'.
- small-plain-2: A piece can show at y=0.5 (bottom edge exactly at 0) for one tick before the miss counts on the next tick. This is probably float accumulation; it is harmless but off by a tick.
- small-plain-2: The interactive tkinter window is untested by the builder (README says so). I did not open it either, under this machine's rules.
