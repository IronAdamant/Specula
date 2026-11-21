# Measured leverage: the runs

Folder: `lv2e`. Every run is here, failed ones included.

## Settings these runs used

- 6 of these builders' briefs carry the lead's choice for the job (which of the engine's aids fit it, under "From the lead"): they are directed builders, not cold ones (change-engine-1, change-engine-2, medium-engine-1, medium-engine-2, small-engine-1, small-engine-2)
- engine build of the change runs: d663616f43+changes
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium runs: d663616f43+changes
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: d663616f43
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 232,978 | 0.44 | 43,828 | 944,976 | 16,742 | 29 | 32 | 4 | done | 32 | 5,542 / 14,861 | 9 / 9 | 1 / 1 | 3 | 1,169 |
| change-engine-2 | 108,468 | 0.24 | 32,128 | 248,135 | 8,700 | 11 | 11 | 2 | done | 10 | 1,335 / 5,607 | 8 / 9 | 1 / 1 | 5 | 1,167 |
| change-plain-1 | 101,429 | 0.22 | 27,526 | 228,273 | 8,840 | 11 | 12 | 1 | done | - | - | 9 / 9 | 1 / 1 | 2 | 786 |
| change-plain-2 | 81,210 | 0.18 | 23,276 | 180,245 | 6,819 | 10 | 15 | 1 | done | - | - | 9 / 9 | 1 / 1 | 4 | 694 |
| medium-engine-1 | 235,737 | 0.46 | 44,379 | 799,694 | 20,061 | 23 | 30 | 4 | done | 20 | 2,164 / 10,453 | 19 / 19 | 2 / 2 | 5 | 1,042 (832 at the start) |
| medium-engine-2 | 177,707 | 0.37 | 39,063 | 442,659 | 16,924 | 15 | 23 | 3 | done | 10 | 3,452 / 7,841 | 18 / 19 | 1 / 2 | 5 | 1,031 (832 at the start) |
| medium-plain-1 | 142,073 | 0.31 | 27,460 | 179,776 | 17,955 | 9 | 11 | 2 | done | - | - | 18 / 19 | 2 / 2 | 2 | 686 |
| medium-plain-2 | 145,038 | 0.31 | 25,414 | 177,948 | 19,096 | 9 | 12 | 2 | done | - | - | 19 / 19 | 2 / 2 | 4 | 620 |
| small-engine-1 | 114,090 | 0.24 | 28,543 | 279,220 | 10,099 | 12 | 18 | 2 | done | 8 | 588 / 2,575 | 14 / 14 | 2 / 2 | 2 | 871 (832 at the start) |
| small-engine-2 | 104,011 | 0.23 | 28,099 | 187,070 | 10,037 | 9 | 16 | 2 | done | 5 | 288 / 2,643 | 14 / 14 | 2 / 2 | 2 | 864 (832 at the start) |
| small-plain-1 | 66,862 | 0.14 | 13,129 | 86,893 | 8,353 | 7 | 8 | 1 | done | - | - | 14 / 14 | 2 / 2 | 4 | 278 |
| small-plain-2 | 61,920 | 0.14 | 12,370 | 54,354 | 8,205 | 5 | 5 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 322 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 2 of 2 | 170,723 | 108,468 to 232,978 | 3 | 8.5 / 9 | 20,085 |
| change | plain | 2 of 2 | 91,320 | 81,210 to 101,429 | 1 | 9.0 / 9 | 10,147 |
| medium | engine | 2 of 2 | 206,722 | 177,707 to 235,737 | 4 | 18.5 / 19 | 11,174 |
| medium | plain | 2 of 2 | 143,556 | 142,073 to 145,038 | 2 | 18.5 / 19 | 7,760 |
| small | engine | 2 of 2 | 109,050 | 104,011 to 114,090 | 2 | 14.0 / 14 | 7,789 |
| small | plain | 2 of 2 | 64,391 | 61,920 to 66,862 | 1 | 14.0 / 14 | 4,599 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| change | engine | 170,723 | 91,320 | 1.87 | 1.90 | 8.5 / 9 | 9.0 / 9 |
| medium | engine | 206,722 | 143,556 | 1.44 | 2.11 | 18.5 / 19 | 18.5 / 19 |
| small | engine | 109,051 | 64,391 | 1.69 | 1.75 | 14.0 / 14 | 14.0 / 14 |

Judging: 12 judging(s) of 12 run(s), 2,704,652 weighted tokens in all.

At the runner's list prices: 3.28 USD for the builds, 10.43 USD for the judging shown above (13.70 USD in all).

## Items that failed

- change-engine-2 **c01**: Passive run seed 1, gate_health=100, sampled every 10-40 ticks: after wave 5, wave 6 sends 15 runners (kind runner, health 1 at Normal, x drops 0.75 per 15 ticks = 3 u/s), but they spawn at ticks 8506, 8551, 8596... every 45 ticks (0.75 s), not every 1.5 s (90 ticks). The rules use RUNNER_GAP = 45 (modules/work/src/lib.rs), and CHANGES.md says so.
- medium-engine-2 **m01**: README's play command (from work/, --ticks 400 --seed 1 --settings ... --inputs 0:enter,...) runs, starts at screen 'title' (tick 0 state printed) and prints one JSON object with all the brief's keys, but during play it prints "screen":"hud" and when paused "screen":"pause"; the brief's JSON allows only title, settings, play, paused, won, lost.
- medium-plain-1 **m19**: README explains how to play and gives the commands. I ran its frame command exactly as written ('... --settings out/settings.json --script script.json --frame out/frame.png') in a fresh folder. It crashed with FileNotFoundError: 'out/frame.png', because the folder out/ does not exist and write_png does not create it. It wrote no PNG and printed no JSON. The headless command without --frame does work as written.

## What each judge found beyond the list

- change-engine-1: The state's "difficulty" reads "normal" on the title and settings screens even when the saved or just-chosen difficulty is Hard or Easy; it becomes right only once a run starts.
- change-engine-1: The Language and Difficulty settings stop at the ends (Right on French stays French) rather than wrapping round. The order is right.
- change-engine-1: The settings frame shows a faint copy of the title text behind the 'Réglages' heading (the title screen showing through the settings screen).
- change-engine-2: Runner spawn gap is 0.75 s (45 ticks); the job list asks for 1.5 s (this is c01).
- change-engine-2: On the Settings screen Left/Right on Language stop at the ends (French then Right stays French), while Enter wraps around. Minor.
- change-engine-2: The state's 'screen' still reads 'hud' and 'pause' instead of 'play' and 'paused' (the m01 defect from before the change, not fixed).
- change-engine-2: The report has an extra key 'work_difficulty' beside 'difficulty'. Harmless.
- change-engine-2: The game ships no checks/ folder, so 'leverage check' on the game itself has nothing to play.
- change-plain-1: As before the change, the README frame command as written ('--settings out/settings.json ... --frame out/frame.png') crashes with FileNotFoundError in a fresh folder, because write_png does not create out/. This repeats the earlier m19 failure.
- change-plain-1: After a run ends (won/lost), the walkers and bolts stay in the state until the next Start (CHANGES.md says so too).
- change-plain-2: README.md is partly stale: lists only en/es files, says settings store volume and language, the state field list lacks difficulty, and 'What I checked' still says 5 waves / 450.
- change-plain-2: The bitmap font draws every diacritic as a bar above the letter, so the French cedilla in 'Français' appears above the c rather than below it.
- change-plain-2: When stdout is piped on Windows the headless JSON is printed in the locale code page (cp1252), not UTF-8, so accented strings must be decoded as cp1252 (present before the change too).
- change-plain-2: On the won/lost screens the overlay box does not hide the guard and HUD behind it (cosmetic).
- medium-engine-1: The guard's centre is clamped to 0..12, so at the edges half the guard is drawn outside the field (at y=12 it is half off-screen under the HUD).
- medium-engine-1: After the lost screen, or after Title from pause, the report still lists the frozen walkers of the abandoned run (title screen state shows walkers).
- medium-engine-1: Start is drawn in a filled blue style on the title even when not selected, which competes with the focus outline as the 'chosen' marker.
- medium-engine-1: A faint leftover panel is visible behind the settings box in the settings frame.
- medium-engine-1: Any non-held input (enter, escape) releases held shoot/up/down, so holding shoot across a pause needs re-giving it (documented in README).
- medium-engine-2: The state's screen value is 'hud' during play and 'pause' when paused, not the brief's 'play' and 'paused'.
- medium-engine-2: On the title screen after leaving a run, the state still reports the last run's walkers, bolts, score and wave (stale values until Start).
- medium-engine-2: texts on settings, pause and end screens also list the strings of the screen beneath (e.g. the title's strings on settings, the HUD's on pause); the README admits this.
- medium-engine-2: The Start button is drawn in the primary blue style whether or not it has focus, so on the title screen two buttons look highlighted; focus is shown only by the outline.
- medium-engine-2: Default volume is 8 (the brief names no default; noted only).
- medium-plain-1: The frame writer does not create its folder. When the frame path's folder is missing, the run crashes before it prints the state, so the JSON is lost too.
- medium-plain-1: On the won, lost and paused screens the HUD (score, wave, gate) is drawn behind the panel. On won/lost those HUD strings are on screen but missing from 'texts' (paused does include them).
- medium-plain-2: On won/lost screens the frame still shows the HUD (score, wave, gate), but 'texts' lists only the 3 result strings, so texts is not every string on screen there.
- medium-plain-2: Headless JSON is printed with ensure_ascii=False in the console code page (cp1252 on this machine), not UTF-8. A reader that expects UTF-8 fails on Espanol/Titulo.
- medium-plain-2: Bolt hit range uses float guard_y: a guard at a nominal 4.6 (|dy| = 0.6) misses the lane-4 walker due to rounding (6 - 14*0.1 = 4.6000000000000005).
- medium-plain-2: The window mode (play, tkinter) was not run by the builder; its text uses Tk fonts rather than the bitmap font of the frame. No sound is produced although volume is stored.
- small-engine-1: A restart given during a run that is not over is ignored (README run with 500:restart in play: nothing reset). The brief does not require it, but the README's own example command implies it.
- small-engine-1: On the tick of the 5th catch, pieces handled later in that same tick already move at the new speed (the speed is re-read per piece in the loop). This is minor.
- small-engine-2: A piece whose bottom lands exactly on y 0 is counted as missed one tick late (tick 227 instead of 226) because the float sum leaves bottom at 3.5e-14 above 0; harmless.
- small-engine-2: README frame command writes frame.png into whatever folder it is run from (in the work folder it overwrites the shipped frame.png); not stated.
- small-plain-1: The game-over overlay box hides any piece behind it (the piece at (7.5,10.1) in the over frame is not visible).
- small-plain-1: On the tick that ends the run, pieces after the third missed piece in the list are not moved, while earlier ones are. This is a one-step inconsistency.
- small-plain-1: A piece whose bottom sits exactly on the bottom edge (y 0.5 at tick 225) is counted as missed one tick later, because of float rounding.
- small-plain-1: The interactive tkinter play mode was not tested by the builder. The judge did not open it either (no windows allowed).
- small-plain-2: The restart tick does not advance the game: the reset uses up that tick, so the new run's first piece appears one tick after the restart tick, not at it.
- small-plain-2: When the run ends, every piece on screen is removed at once, so the game-over frame shows no pieces where they were.
- small-plain-2: The play window (tkinter) was not opened by the builder or by the judge; keyboard play and the Enter restart in the window are untested.
