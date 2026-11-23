# Measured leverage: the runs

Folder: `lv2h`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the large runs: ff4b129814
- large: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| large-batch-1 | 442,423 | 0.82 | 68,434 | 1,671,959 | 37,940 | 31 | 33 | 13 | done | 19 | 1,803 / 10,737 | 31 / 31 | 2 / 2 | 5 | 1,947 (875 at the start) |
| large-batch-2 | 530,236 | 0.99 | 83,968 | 1,973,482 | 45,589 | 34 | 42 | 20 | done | 21 | 2,012 / 22,380 | 31 / 31 | 2 / 2 | 4 | 1,896 (875 at the start) |
| large-batch-3 | 695,753 | 1.23 | 96,394 | 3,041,730 | 54,222 | 45 | 53 | 19 | done | 38 | 3,445 / 26,786 | 31 / 31 | 2 / 2 | 4 | 1,941 (875 at the start) |
| large-batch-4 | 798,332 | 1.40 | 107,040 | 3,560,354 | 61,704 | 46 | 56 | 17 | done | 36 | 5,454 / 22,138 | 30 / 31 | 2 / 2 | 3 | 1,603 (875 at the start) |
| large-batch-5 | 755,236 | 1.35 | 107,541 | 3,218,663 | 59,793 | 44 | 54 | 34 | done | 31 | 10,470 / 34,741 | 31 / 31 | 2 / 2 | 3 | 2,043 (875 at the start) |
| large-nodocs-1 | 614,579 | 1.10 | 88,268 | 2,597,332 | 48,906 | 39 | 50 | 11 | done | 28 | 5,613 / 17,732 | 31 / 31 | 2 / 2 | 3 | 1,594 (875 at the start) |
| large-nodocs-2 | 467,840 | 0.87 | 72,174 | 1,763,638 | 40,255 | 33 | 36 | 8 | done | 26 | 3,668 / 14,424 | 31 / 31 | 2 / 2 | 4 | 1,726 (875 at the start) |
| large-nodocs-3 | 526,823 | 0.98 | 80,914 | 1,980,816 | 45,523 | 33 | 39 | 11 | done | 45 | 8,102 / 16,544 | 30 / 31 | 2 / 2 | 3 | 1,472 (875 at the start) |
| large-nodocs-4 | 558,033 | 1.02 | 82,429 | 2,179,146 | 47,420 | 36 | 37 | 11 | done | 33 | 6,019 / 15,548 | 30 / 31 | 2 / 2 | 6 | 1,753 (875 at the start) |
| large-nodocs-5 | 493,425 | 0.94 | 79,314 | 1,610,967 | 46,640 | 28 | 44 | 9 | done | 18 | 3,390 / 13,803 | 30 / 31 | 2 / 2 | 3 | 1,468 (875 at the start) |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| large | batch | 5 of 5 | 644,396 | 442,423 to 798,332 | 21 | 30.8 / 31 | 20,922 |
| large | nodocs | 5 of 5 | 532,140 | 467,840 to 614,579 | 10 | 30.4 / 31 | 17,505 |

## Against the baseline arm, `nodocs` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| large | batch | 644,396 | 532,140 | 1.21 | 1.18 | 30.8 / 31 | 30.4 / 31 |

Judging: 10 judging(s) of 10 run(s), 6,724,973 weighted tokens in all.

At the runner's list prices: 10.70 USD for the builds, 22.94 USD for the judging shown above (33.65 USD in all).

## Items that failed

- large-batch-4 **l22**: Escape pauses and nothing moves (walker x 19.775 from tick 11 to 60); Title works; but Enter on Resume (focus 0) does nothing: screen stays paused 40+ ticks after (tried twice). Only Escape again resumes.
- large-nodocs-3 **l22**: Escape pauses (enemy x frozen at 19.625 for 40 ticks) and Title returns to the title, but Enter on Resume (menu_index 0) does nothing: screen stayed paused through ticks 101-140 in three runs; only Escape resumed.
- large-nodocs-4 **l22**: Escape pauses and nothing moves (enemy x 19.3 frozen ticks 31-90); Title (down, Enter) returns to title. But Enter on Resume (menu_index 0) does nothing: still paused 40 ticks later, in four scripts with and without shoot held; only Escape resumes.
- large-nodocs-5 **l22**: Escape pauses and nothing moves (enemies/bolts frozen 301-340); Escape again continues; Title returns to title. But Enter on Resume (menu_index 0) leaves the screen paused indefinitely (tick 310 enter, still paused at 330): paused.json button action 'resume' while lib.rs only accepts 'resume_game' or 'back'.

## What each judge found beyond the list

- large-batch-1: Adding --at print points can change how a long run unfolds: same inputs, 12720 ticks, --at 6000 vs --at 5000,6000 gives wave-5 runners 0.2 units (4 ticks) apart at tick 6000, and with --at at every 100 ticks the Next-wave Enter after wave 3 was missed; plain runs and repeated identical commands are deterministic. Whether the cause is the game or the engine is unclear.
- large-batch-1: On lost/won and then the title, the report still lists the last run's enemies (state not cleared after a run ends).
- large-batch-1: Shop and title frames draw the first or last button (Start, Next wave) in the blue 'primary' style besides the focus outline, so two entries look highlighted.
- large-batch-1: Mason's shop line does not say how far it is bought (brief: each thing with its price and how far it is bought); it is unlimited, so this is minor.
- large-batch-1: README's headless command writes the kept folder into work/kept; the judge used a scratch folder instead.
- large-batch-2: After buying Heavy bolts (and when a line changes to its 'full' text) the shop focus jumps back to Quick hands, so a second Enter meant for the same line buys Quick hands instead (seen: 35 -> 23 -> 18 coins, quick 1).
- large-batch-2: The play HUD (Score/Coins/Wave/Gate) is drawn behind the title, settings and shop menus in frames, though those strings are not in the state's texts.
- large-batch-2: With the start_gate test aid the shop shows 'Mason: gate 86/10' (gate above its max); only reachable through the test aid.
- large-batch-2: An input at tick 0 is lost (the title is not up yet); README says so.
- large-batch-3: What is kept (best, saved run) reaches the rules only at tick 1, so an input at tick 0 on the title meets a menu without Continue (README says so).
- large-batch-3: After Title from the pause, the report still carries the abandoned run's score, coins, wave and gate_health while on the title screen.
- large-batch-3: Spanish end screens read 'Enter: titulo', using the English key name 'Enter' (French uses 'Entree').
- large-batch-3: On the title the Start button is always drawn blue (primary style), so it can look chosen when focus is on another entry; the focused entry is shown only by an outline.
- large-batch-4: Enter on Resume in the pause menu does nothing; the game can only be resumed with Escape.
- large-batch-4: Spanish end screens say 'Enter: título' (the English key name, not translated as on the French 'Entrée : titre').
- large-batch-4: The lost screen's state still lists the enemies that were on the field when the gate fell.
- large-batch-5: Start (title) and Next wave (shop) are always drawn in the bright primary style, so with focus elsewhere two entries look highlighted; the focused entry is marked only by an outline.
- large-batch-5: README says only that a windowed player's kept data is in 'a folder of their own, per user' and does not name the path.
- large-batch-5: Before Start the report gives wave 0 and gate_health 0 (README notes this).
- large-nodocs-1: The shop reopens with the focus left where it was last (on Next wave) instead of the first entry; the first shop opens at Quick hands.
- large-nodocs-1: Buying Heavy bolts swaps its button and throws the focus back to the top entry (Quick hands).
- large-nodocs-1: Spanish end screens read 'Enter: titulo', keeping the English key name 'Enter'.
- large-nodocs-2: At tick 0 the state is not ready: the title shows the unresolved text 'Best: {work.best}' and no Continue even when a save exists, so Down at tick 0 lands on Settings instead of Continue.
- large-nodocs-2: Positions are rounded to 4 decimals every tick, so movement drifts: a brute goes about 1.002 units/s (x 18.4469 where 18.45 is expected), and bolts a little under 14/s.
- large-nodocs-2: Bolts still in flight when a wave ends stay frozen through the shop and carry on into the next wave in an unbroken run.
- large-nodocs-2: Volume changes nothing that can be heard: no sounds are attached, as the README admits.
- large-nodocs-3: The pause menu's Resume button does nothing (its action 'resume' never reaches the rules); only Escape leaves the pause.
- large-nodocs-3: A run of 0 ticks shows best 0 and saved false whatever is kept (kept values arrive at tick 1); README admits it.
- large-nodocs-3: On the settings screen, 'texts' also lists the title's strings under the overlay (title name, Best, Start, Settings, Quit).
- large-nodocs-4: Pause menu: Enter on Resume does nothing; only Escape leaves the pause.
- large-nodocs-4: The gate is drawn red (low-health colour) behind the title and settings screens, because gate_health is 0 before any run.
- large-nodocs-4: Each Continue restarts the lane source, so every continued wave uses the same lane sequence (4,10,6,2,6,6,2,4,8,2... for seed 1).
- large-nodocs-4: A saved run is read without bounds: a save with gate 99 is accepted and the shop shows 'gate 99 of 14'.
- large-nodocs-4: After buying a shop item, the marked entry sometimes jumps (to Quick hands after Heavy was bought).
- large-nodocs-4: Spanish end screens say 'Enter: titulo' (the key's English name).
- large-nodocs-5: Pause menu Resume button does nothing (action name 'resume' not handled); only Escape resumes.
- large-nodocs-5: At tick 0 (--at 0) the screen is reported as 'hud' with empty texts and saved null, a value outside the brief's screen list; title appears from tick 1.
- large-nodocs-5: Numbers over 999 are shown with a thousands separator ('Best: 1,540', 'Score : 1 480') rather than the brief's plain form.
