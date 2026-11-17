# Measured leverage: the runs

Folder: `lr5`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change runs: 9450e0f4
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium runs: 9450e0f4
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: 9450e0f4
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 187,215 | 0.37 | 39,744 | 649,602 | 14,517 | 20 | 20 | 10 | done | 27 | 1,686 / 5,456 | 8 / 9 | 1 / 1 | 3 | 1,253 |
| change-engine-2 | 122,496 | 0.26 | 33,232 | 342,277 | 9,347 | 13 | 12 | 5 | done | 15 | 1,068 / 3,142 | 9 / 9 | 1 / 1 | 3 | 1,270 |
| change-engine-3 | 115,398 | 0.25 | 32,478 | 312,367 | 8,714 | 12 | 11 | 4 | done | 14 | 837 / 3,903 | 9 / 9 | 1 / 1 | 2 | 1,034 |
| change-plain-1 | 108,170 | 0.23 | 28,795 | 256,373 | 9,309 | 12 | 14 | 1 | done | - | - | 9 / 9 | 1 / 1 | 2 | 735 |
| change-plain-2 | 118,787 | 0.25 | 28,277 | 319,128 | 10,307 | 14 | 16 | 1 | done | - | - | 8 / 9 | 1 / 1 | 3 | 790 |
| medium-engine-1 | 167,638 | 0.35 | 38,494 | 406,420 | 15,777 | 14 | 17 | 3 | done | 9 | 488 / 2,115 | 19 / 19 | 2 / 2 | 6 | 1,011 (675 at the start) |
| medium-engine-2 | 198,110 | 0.41 | 44,727 | 530,144 | 17,839 | 16 | 22 | 4 | done | 13 | 636 / 3,045 | 19 / 19 | 2 / 2 | 5 | 1,040 (675 at the start) |
| medium-engine-3 | 223,545 | 0.46 | 46,340 | 578,788 | 21,550 | 17 | 27 | 5 | done | 13 | 535 / 4,026 | 19 / 19 | 2 / 2 | 4 | 934 (675 at the start) |
| medium-plain-1 | 142,408 | 0.31 | 25,420 | 145,617 | 19,215 | 8 | 11 | 2 | done | - | - | 19 / 19 | 2 / 2 | 6 | 654 |
| medium-plain-2 | 164,735 | 0.34 | 27,927 | 265,673 | 20,653 | 12 | 19 | 5 | done | - | - | 17 / 19 | 2 / 2 | 4 | 615 |
| small-engine-1 | 143,401 | 0.31 | 35,418 | 310,397 | 13,619 | 12 | 21 | 3 | done | 11 | 447 / 2,008 | 14 / 14 | 2 / 2 | 3 | 773 (675 at the start) |
| small-engine-2 | 120,842 | 0.26 | 28,825 | 235,610 | 12,251 | 11 | 17 | 3 | done | 9 | 293 / 2,098 | 14 / 14 | 2 / 2 | 2 | 731 (675 at the start) |
| small-plain-1 | 63,740 | 0.14 | 12,486 | 69,850 | 8,230 | 6 | 6 | 1 | done | - | - | 14 / 14 | 2 / 2 | 5 | 333 |
| small-plain-2 | 78,903 | 0.16 | 14,039 | 140,741 | 9,457 | 10 | 9 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 322 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 3 of 3 | 141,703 | 115,398 to 187,215 | 6 | 8.7 / 9 | 16,350 |
| change | plain | 2 of 2 | 113,478 | 108,170 to 118,787 | 1 | 8.5 / 9 | 13,350 |
| medium | engine | 3 of 3 | 196,431 | 167,638 to 223,545 | 4 | 19.0 / 19 | 10,338 |
| medium | plain | 2 of 2 | 153,572 | 142,408 to 164,735 | 3 | 18.0 / 19 | 8,532 |
| small | engine | 2 of 2 | 132,122 | 120,842 to 143,401 | 3 | 14.0 / 14 | 9,437 |
| small | plain | 2 of 2 | 71,322 | 63,740 to 78,903 | 1 | 14.0 / 14 | 5,094 |

Judging: 14 judging(s) of 14 run(s), 3,989,452 weighted tokens in all.

At the runner's list prices: 4.11 USD for the builds, 15.33 USD for the judging shown above (19.44 USD in all).

## Items that failed

- change-engine-1 **c01**: Played to wave 6 with a reacting bot through `leverage play --follow` (seed 1, 1-tick steps): 15 runners, kind runner, health 1 at Normal, 3.0 units/s, but they appeared at x=20 at ticks 5336, 5396, 5456, 5516: one every 60 ticks (1.0 s), not every 1.5 s. Code: RUNNER_GAP = 60 in modules/work/src/lib.rs.
- change-plain-2 **c01**: Bot run via CLI (seed 1, Normal): after wave 5 clears, wave 6 sends 15 runners (score 450 -> 675 = 15 x 15), kind runner, health 1, x 20.0 -> 19.95 per tick (3 units/s). But runners spawn at ticks 5803, 5894, 5985, ... 7077: 91 ticks (1.517 s) apart, not 90 (1.5 s). CLI at t5893 shows no runner; the second appears at t5894. This is the same spawn_timer off-by-one that failed m09, now in the new wave too.
- medium-plain-2 **m09**: 5 walkers in wave 1, in lanes 4,10,2,6,2, moving 0.025 per tick (1.5/s), entering at x=20. But they spawn at ticks 1, 92, 183, 274, 365: 91 ticks (1.517 s) apart, not 90 ticks (1.5 s). At t91 there is still one walker; the second appears at t92. spawn_timer is set to 90 and spawning happens only when it reaches 0, one step too late.
- medium-plain-2 **m19**: The README explains how to play. Its headless command works as written. Its frame command 'python game.py --frame out/frame.png ... --settings out/s.json --script "0:enter,1:shoot"' run in a fresh folder crashes with FileNotFoundError: 'out/frame.png'. Nothing creates out/: no setting changes, so settings are never saved, and render_png does not create the folder. So the two commands do not both work as written.

## What each judge found beyond the list

- change-engine-1: Wave 6 runners spawn every 1.0 s (RUNNER_GAP 60 ticks), not every 1.5 s like the other waves.
- change-engine-1: The README Settings section still says only volume and language are kept; difficulty is kept too but not mentioned there.
- change-engine-1: On the Language button Right stops at Français and Left stops at English; only Enter cycles back to English.
- change-engine-2: CHANGES.md says a French frame was written to checks/out/fr_play.png; no such file is in work/checks/out (only lost.png and play.png).
- change-engine-2: On the Language button Left/Right stop at the ends (English, French) instead of wrapping; only Enter cycles round. Not against the brief, noted only.
- change-engine-2: Unused `count` in lib.rs tick (already noted by the builder); harmless.
- change-engine-3: Bolts hit any walker in the lane with x <= the bolt's x, so a walker already past the guard (between guard and gate) is hit by a fresh bolt at once; builder found this and left it as is.
- change-engine-3: Left/Right on Language and Difficulty stop at the ends (no wrap), while Enter wraps; harmless but inconsistent.
- change-plain-1: README 'What I checked' still says the bot won with score 450 (from before the change); 'Settings' still says 'Volume and language are saved', although difficulty is saved too.
- change-plain-1: In the paused French frame the HUD items 'VAGUE 1 SUR 6' and 'PORTE : 5' nearly touch (cosmetic).
- change-plain-2: Spawn spacing is 91 ticks, not 90 (spawn_timer reaches 0 one step late). This is pre-existing (m09) and now also affects the new runner wave (c01). CHANGES.md does not mention it among the things that were wrong before.
- change-plain-2: README frame command still crashes with FileNotFoundError 'out/frame.png' in a fresh folder, because render_png does not create the folder (pre-existing m19, unchanged).
- change-plain-2: PNG font upper-cases all text, so French strings like 'Entrée : titre' lose their case (CHANGES.md notes this).
- medium-engine-1: Bolt positions are rounded to 0.001 and stored every tick, so a bolt moves 0.233 per tick = 13.98 units a second, not exactly 14 (after 1 s it is at 15.98, not 16).
- medium-engine-1: The next wave starts 179 ticks (2.98 s) after the previous wave's last walker is gone, one tick short of 3 seconds.
- medium-engine-1: On the settings screen `texts` also lists the title screen's strings (name, Start, Settings, Quit), which are hidden behind the settings panel in the frame.
- medium-engine-1: On the title screen Start is always drawn filled blue (primary style), so when Settings or Quit is chosen two buttons look highlighted; the chosen one only has an extra outline.
- medium-engine-1: After returning to the title, the state still reports the previous run's score, wave, gate_health and walkers.
- medium-engine-1: Language does not wrap: Left on English and Right on Spanish do nothing (the other direction changes it).
- medium-engine-2: On the settings screen, texts also lists the title screen's strings beneath it (Hold the gate, Start, Settings, Quit), which the frame mostly hides.
- medium-engine-2: Before the first run, score, wave, gate_health, guard_y, walkers and bolts are printed as null rather than values.
- medium-engine-2: After pause then Title, the title screen's state still reports the abandoned run's walkers, score and wave.
- medium-engine-2: Volume is only stored; the game plays no sound it scales (the README admits this).
- medium-engine-2: The Spanish end-screen button keeps the key name 'Enter' (Enter: título).
- medium-engine-3: Hit test is only bolt.x >= walker.x in the lane, with no lower bound: a walker that has passed the guard (x < 2) is hit at once by a newly fired bolt in its lane (the README admits this).
- medium-engine-3: After returning to the title, the report still prints the last run's bolts and gate_health (e.g. title screen with 2 bolts after the win; gate_health 0 on title after a loss).
- medium-engine-3: The guard can move to y 0 and y 12, so at the field's edge it is drawn half off the frame, and at the top it sits under the HUD panel.
- medium-engine-3: texts on Settings and pause also lists the screen underneath (the title's words on the settings screen), which is not visibly shown in the frame (the README says so).
- medium-plain-1: The tkinter window title 'Hold the gate' is written into the code (play_window), not taken from the string tables, so it stays English in Spanish.
- medium-plain-1: The Spanish won/lost line is 'Enter: titulo' - the key name stays English (Spanish keyboards usually say Intro).
- medium-plain-1: In the play frame the HUD text sits over the top of the field (y about 11.1 to 12), so a guard at the top edge is partly under the HUD.
- medium-plain-1: Quit in a headless run only sets a flag; the screen stays title (documented in README).
- medium-plain-1: The pause overlay hides walkers and bolts in the middle of the field in the paused frame.
- medium-plain-1: The interactive window ('play') was not tested by the builder (README says so) and was not opened by the judge either.
- medium-plain-2: Walker spawn interval is 91 ticks (1.517 s) instead of 90. Off-by-one in the spawn_timer countdown, so later waves drift (wave 5: 12 ticks late on its last walker).
- medium-plain-2: The --frame path does not create its parent folder, so the README's example frame command fails unless out/ already exists.
- medium-plain-2: The tkinter window title 'Hold the gate' is hard-coded in English and does not come from the string tables (seen in the code only; the window was not opened).
- medium-plain-2: The Spanish won/lost hint reads 'Enter: título' and keeps the English key name 'Enter' (Spanish keyboards usually say 'Intro'). Minor.
- small-engine-1: The catch test (piece bottom <= 0.5 while overlapping) also fires for a piece already below the paddle top if the paddle slides under it sideways. Touching edges also count as a catch. Both are defensible as 'touches', and the README states the edge rule.
- small-engine-1: README's own example sends restart at tick 300 while the run is still in play, where it has no effect. Harmless, but it does not show a restart.
- small-engine-1: The headless wrapper catch.py needs Python and must be run from the work folder (relative path). The README notes the Python need but not the folder.
- small-engine-2: README says how to play in a window (`specula play . --as-is`) but marks it untried; I did not open a window, so window play is unverified.
- small-engine-2: README's 'What I checked' says the speed-up after 5 catches was never run; it does work (I checked it), but the builder did not verify it.
- small-plain-1: HUD text is drawn over the top of the field: newly spawned pieces (y 15-16) overlap 'SCORE'/'MISSES LEFT' text (seen in the tick-230 frame where a piece covers part of 'MISSES LEFT 2').
- small-plain-1: Because of float rounding, a piece can sit with its bottom exactly on the bottom edge (y=0.5 at tick 226) for one tick before the miss is counted at tick 227.
- small-plain-1: The game-over view writes 'GAME OVER' in capitals, not 'Game over', and labels the final score only 'SCORE' (the frame font has only capital letters).
- small-plain-1: The third miss wipes every piece still falling at once; the brief does not ask for this (it only freezes the run).
- small-plain-1: The interactive tkinter play mode was never run by its builder, and I did not run it either, because it would open a window.
- small-plain-2: At game over the pieces still falling are removed from the state, so the game-over frame no longer shows them (documented in README).
- small-plain-2: Because of floating point, a piece can sit with its bottom exactly on the bottom edge (y 0.5) for one tick before the miss is counted the next tick.
- small-plain-2: The live tkinter window (keys, Enter restart, Consolas text) could not be exercised by me; only its code was read.
