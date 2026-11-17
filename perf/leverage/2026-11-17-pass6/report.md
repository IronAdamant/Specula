# Measured leverage: the runs

Folder: `lr6`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the change runs: 57f515d0
- change: builder claude-sonnet-5-5, cap 500,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the medium runs: 57f515d0
- medium: builder claude-sonnet-5-5, cap 2,000,000 tokens and 180 min; judge claude-opus-5-5; 2 run(s) a cell
- engine build of the small runs: 57f515d0
- small: builder claude-sonnet-5-5, cap 600,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| change-engine-1 | 184,989 | 0.36 | 37,821 | 702,047 | 13,504 | 23 | 25 | 5 | done | 27 | 3,265 / 4,632 | 9 / 9 | 1 / 1 | 2 | 1,132 |
| change-engine-2 | 123,769 | 0.27 | 33,008 | 319,258 | 10,118 | 13 | 15 | 4 | done | 18 | 1,782 / 5,437 | 9 / 9 | 1 / 1 | 3 | 1,162 |
| change-engine-3 | 112,479 | 0.25 | 29,829 | 241,779 | 10,204 | 11 | 11 | 3 | done | 15 | 1,511 / 4,197 | 9 / 9 | 1 / 1 | 3 | 1,017 |
| change-plain-1 | 101,750 | 0.22 | 27,216 | 228,108 | 8,985 | 11 | 16 | 1 | done | - | - | 9 / 9 | 1 / 1 | 3 | 838 |
| change-plain-2 | 89,155 | 0.20 | 26,205 | 163,486 | 8,011 | 9 | 17 | 1 | done | - | - | 9 / 9 | 1 / 1 | 4 | 790 |
| medium-engine-1 | 173,021 | 0.36 | 36,795 | 418,846 | 17,030 | 15 | 21 | 3 | done | 13 | 885 / 2,904 | 19 / 19 | 2 / 2 | 4 | 1,000 (683 at the start) |
| medium-engine-2 | 163,128 | 0.34 | 35,329 | 434,901 | 15,097 | 16 | 21 | 3 | done | 11 | 640 / 2,487 | 19 / 19 | 2 / 2 | 5 | 1,031 (683 at the start) |
| medium-engine-3 | 206,039 | 0.41 | 41,150 | 642,566 | 18,071 | 20 | 29 | 13 | done | 14 | 5,758 / 3,580 | 18 / 19 | 2 / 2 | 5 | 890 (683 at the start) |
| medium-plain-1 | 147,791 | 0.32 | 26,377 | 145,992 | 20,045 | 8 | 12 | 2 | done | - | - | 19 / 19 | 2 / 2 | 5 | 720 |
| medium-plain-2 | 149,576 | 0.32 | 25,792 | 174,008 | 19,988 | 9 | 13 | 2 | done | - | - | 18 / 19 | 2 / 2 | 4 | 692 |
| small-engine-1 | 111,068 | 0.23 | 26,140 | 282,644 | 10,027 | 13 | 16 | 2 | done | 9 | 473 / 1,591 | 14 / 14 | 2 / 2 | 3 | 741 (683 at the start) |
| small-engine-2 | 112,808 | 0.24 | 26,095 | 262,852 | 10,782 | 12 | 16 | 2 | done | 7 | 400 / 1,251 | 14 / 14 | 2 / 2 | 3 | 722 (683 at the start) |
| small-plain-1 | 72,384 | 0.16 | 13,874 | 90,047 | 9,208 | 7 | 7 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 354 |
| small-plain-2 | 66,839 | 0.14 | 12,544 | 86,822 | 8,496 | 7 | 7 | 1 | done | - | - | 14 / 14 | 2 / 2 | 3 | 284 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| change | engine | 3 of 3 | 140,412 | 112,479 to 184,989 | 4 | 9.0 / 9 | 15,601 |
| change | plain | 2 of 2 | 95,452 | 89,155 to 101,750 | 1 | 9.0 / 9 | 10,606 |
| medium | engine | 3 of 3 | 180,729 | 163,128 to 206,039 | 6 | 18.7 / 19 | 9,682 |
| medium | plain | 2 of 2 | 148,684 | 147,791 to 149,576 | 2 | 18.5 / 19 | 8,037 |
| small | engine | 2 of 2 | 111,938 | 111,068 to 112,808 | 2 | 14.0 / 14 | 7,996 |
| small | plain | 2 of 2 | 69,612 | 66,839 to 72,384 | 1 | 14.0 / 14 | 4,972 |

Judging: 14 judging(s) of 14 run(s), 3,717,217 weighted tokens in all.

At the runner's list prices: 3.81 USD for the builds, 14.48 USD for the judging shown above (18.28 USD in all).

## Items that failed

- medium-engine-3 **m16**: Play frames do not show what the state says: tick 200 state 3 walkers, frame shows none; tick 400 state 5 walkers + 3 bolts, frame shows 1 walker, no bolts; tick 600 seed 3 shows 2 of 3 walkers, 1 of 3 bolts. Lanes y=6,8,10 never drawn. HUD (score, wave, gate), gate and guard are shown.
- medium-plain-2 **m16**: At tick 3337 (seed 1) the frame matches the state: walkers (16.825,8) and (19.075,4), bolts at x 10.4 and 4.8 in y 8, guard at 6.9, orange gate on the left, Score 280, Wave 4 of 5, Gate 5. But the field's top (y about 11.15-12) is drawn under the 34 px status bar: holding up 100 ticks gives guard_y 12.0 and the frame (top.png) shows no guard at all. The guard is hidden from y about 11.55 up, a position it can reach.

## What each judge found beyond the list

- change-engine-1: On the Language row, Right stops at French instead of wrapping to English (Enter does wrap); not against the brief.
- change-engine-1: During the 3 s wait between waves the HUD still shows the old wave number (noted by the builder as existing before).
- change-engine-2: README.md is partly stale: 'How to play' says each walker has 2 health (only at Normal); 'What I checked' still reports a 450-point win; 'Not done' says walkers are drawn green/red, but runners are drawn yellow.
- change-engine-2: README.md 'Settings' section says only volume and language are kept, though difficulty is kept too.
- change-engine-2: CHANGES.md says it never looked at the frame image, and that Spanish/French menus were not walked screen by screen.
- change-engine-3: Play frames do not draw what the state holds (most walkers and all bolts missing): the pre-change m16 defect, not repaired.
- change-engine-3: The reported "difficulty" (work.difficulty) shows normal on the title and settings screens even when the setting is hard or easy; it only follows the setting once a run starts (setting_difficulty is right).
- change-engine-3: After a run ends (title, lost), the state still lists walkers and bolts from the run (README notes bolts).
- change-plain-1: README.md headless section is stale. The state field list omits 'difficulty' and walker 'kind', and the language codes name only en and es. The default settings example lacks difficulty. 'What I checked' still says the bot wins all 5 waves with 450.
- change-plain-1: Difficulty is read only when a walker spawns. That is harmless because it can only be changed outside a run (noted in CHANGES.md).
- change-plain-1: Not judged: the tkinter window (keyboard, timing, sound). Opening a window is not allowed on this machine; the builder did not test it either.
- change-plain-2: README 'What I checked' still says the bot wins seed 3 'with 450 points (45 walkers + 15 runners)'. That is stale: the full clear is 675, and selfcheck.py asserts 675.
- change-plain-2: Pre-existing: the guard at y above about 11.55 is hidden under the top status bar (m16).
- change-plain-2: French settings labels use the code's fixed ': ' ('Volume: 5', 'Difficulté: Normal'), while the French tables use ' : ' elsewhere ('Score final : 675'), so the typography is inconsistent.
- change-plain-2: Accent marks are single pixels, so É/È/Ê render as a dot over E (looks like Ė). Cosmetic.
- medium-engine-1: In frames the lane lines are drawn over walkers and bolts: each walker shows as two split boxes and most bolts as two thin hollow lines (README calls it a faint band; it is plainly visible).
- medium-engine-1: texts on paused, won and lost lists only the top screen's strings, although the HUD (score, wave, gate) is still drawn beneath; the brief asks for every string on screen.
- medium-engine-1: After going back to the title (from pause or after won), the printed state still carries the old run's score, wave, walkers and bolts.
- medium-engine-1: Volume is only a stored setting; the game plays no sound (README says so).
- medium-engine-2: On the settings screen, `texts` also lists the title screen's strings (name, Start, Settings, Quit), which are hidden behind the settings panel in the frame.
- medium-engine-2: The raw engine command the README also gives reports screen 'hud' / 'pause', not the brief's 'play' / 'paused'; only the holdgate.py wrapper renames them.
- medium-engine-2: holdgate.py prints in the console's encoding: when another Python program captured it without PYTHONIOENCODING set, 'Espanol'/'Atras' with accents came out as non-UTF-8 bytes.
- medium-engine-2: After returning to the title, the state keeps the last run's score/wave/walkers instead of null.
- medium-engine-2: Quit on the title does nothing visible in a headless run (screen stays title); its window behaviour was not checked.
- medium-engine-3: Field shapes are all cubes at z=0, the same depth as the field background: in frames walkers, bolts and the upper lane lines vanish or show split (z-fighting), so the drawn field is unreliable.
- medium-engine-3: On the settings screen, texts also lists the hidden title screen strings (Hold the gate, Start, Settings, Quit), which are not visible in the frame.
- medium-engine-3: The Start button is always drawn in the blue 'primary' style, so on the title screen two buttons look highlighted when another entry is focused (the focus ring still marks the chosen one).
- medium-engine-3: Guard may sit at y=0 or y=12, half off screen; bolts in flight stay in state and on screen after won/lost.
- medium-engine-3: Spanish end screens say 'Enter: titulo' (key name left in English).
- medium-plain-1: On the won screen the state still lists bolts in flight (2 bolts with x values); they are frozen leftovers from play.
- medium-plain-1: The first walker of a new wave appears 1 tick (1/60 s) after the wave counter changes, so it arrives 181 ticks after the previous wave ended, not 180.
- medium-plain-1: The Spanish end-screen line is 'Enter: inicio', which keeps the English key name 'Enter'.
- medium-plain-1: In headless mode, Quit on the title screen does nothing visible: it sets an internal flag and the screen stays title (as the README admits).
- medium-plain-1: The default settings file is written next to holdthegate.py inside the game folder when --settings is not given.
- medium-plain-2: The status bar covers the top 34 px of the field, so a guard at y 11.55 to 12 (and its bolts) is not drawn; the README says so itself.
- medium-plain-2: Accents are drawn as a single dot above the letter, so 'Título' reads like 'TİTULO' in the frame.
- medium-plain-2: After leaving a run (pause -> Title), the title-screen state still reports the abandoned run's score, walkers and bolts (e.g. 3 walkers and 3 bolts on screen 'title'). Start resets them.
- medium-plain-2: A walker at x 0.0 is still listed for one tick and only removed on the next step (tick 800 vs 801).
- small-engine-1: The HUD panel at top-left is drawn over the field and hides part of any piece near the top of columns 0-3 (seen in over.png: piece at x 1.5, y 14.83 partly covered).
- small-engine-1: In the tick the third miss happens, the loop still handles the other pieces, so another catch or miss could still be counted after over is set. The 4.8-unit piece spacing means this cannot happen at the speeds I saw, so it is minor.
- small-engine-1: The README's window command was never tried by the builder, as the README itself says. I did not try it either, because the rules forbid opening a window.
- small-engine-2: README 'Play' section gives no exact command for window play: a play line ending in '...' and 'specula play . --as-is', with the seed set 'via the engine's own option'.
- small-engine-2: Restart (Enter) is accepted at any time during a run, not only after the end. README says so, and the brief does not forbid it.
- small-engine-2: Floating-point drift leaves a piece at y 0.5000000000000349 for one tick, so a miss is counted one tick later than exact arithmetic would give (tick 227 instead of 226). The effect is minor.
- small-plain-1: When the third miss ends the run, every piece still falling disappears from the state and the frame (the step loop breaks with an empty keep list), so the game-over view shows an empty field.
- small-plain-1: Because fall positions build up floating-point error, a piece rests at y=0.5000000000000349 (bottom edge ~3e-14 above 0) for one tick and the miss is counted one tick late (tick 226 instead of 225 for seed 1).
- small-plain-1: The tkinter play window is untested by the builder (README says so). I did not open it either, because of this machine's rules.
- small-plain-2: The score and misses-left text is drawn over the top of the field, so a newly spawned piece is partly hidden behind it (frame at seed 1, tick 220: piece at x 4.5 under MISSES LEFT).
- small-plain-2: Piece y builds up floating-point drift (e.g. 0.5000000000000349 where 0.5 is exact), so a miss can register one tick late (seed 1, no input: piece reaches the bottom at tick 369, miss counted at 370).
- small-plain-2: The interactive tkinter window was not opened by the builder or by me (the rules forbid a focus-taking window), so the key handling and window drawing are not verified.
