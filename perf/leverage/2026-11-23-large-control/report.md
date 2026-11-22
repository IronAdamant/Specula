# Measured leverage: the runs

Folder: `lv2i`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the large runs: ff4b129814
- large: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| large-engine-1 | 638,415 | 1.12 | 89,145 | 2,922,108 | 46,959 | 45 | 57 | 10 | done | 27 | 4,576 / 13,746 | 31 / 31 | 2 / 2 | 4 | 1,460 (875 at the start) |
| large-engine-2 | 535,700 | 0.98 | 82,787 | 2,178,247 | 42,882 | 36 | 47 | 10 | done | 31 | 36,829 / 16,507 | 30 / 31 | 2 / 2 | 4 | 1,535 (875 at the start) |
| large-engine-3 | 515,590 | 0.95 | 79,066 | 2,014,800 | 43,059 | 34 | 35 | 9 | done | 34 | 4,298 / 15,053 | 31 / 31 | 2 / 2 | 3 | 1,584 (875 at the start) |
| large-engine-4 | 481,473 | 0.88 | 74,990 | 1,948,633 | 38,578 | 35 | 45 | 8 | done | 22 | 2,768 / 12,067 | 31 / 31 | 2 / 2 | 5 | 1,445 (875 at the start) |
| large-engine-5 | 411,391 | 0.80 | 68,745 | 1,292,676 | 39,241 | 25 | 37 | 8 | done | 26 | 24,388 / 8,657 | 30 / 31 | 2 / 2 | 4 | 1,494 (875 at the start) |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| large | engine | 5 of 5 | 516,514 | 411,391 to 638,415 | 9 | 30.6 / 31 | 16,880 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| large | engine | 516,514 | no judged run | - | - | 30.6 / 31 | - |

Judging: 5 judging(s) of 5 run(s), 2,787,053 weighted tokens in all.

At the runner's list prices: 4.72 USD for the builds, 9.98 USD for the judging shown above (14.70 USD in all).

## Items that failed

- large-engine-2 **l22**: Escape at 50 -> paused, and enemies, bolts and guard stay the same through 300 ticks; Title (down, Enter) -> title. But Enter on Resume (menu_index 0) leaves the screen paused at every later tick (tested 3 times). Only Escape resumes.
- large-engine-5 **l21**: Not reached: I could not script a run through all 8 waves with the gate standing in the time given (the builder also says the full win was never played). Score formula seen only in code (score += 20*gate).

## What each judge found beyond the list

- large-engine-1: Shop entries at their last level read 'Quick hands 2/2 - - coins' / 'Heavy bolts 1/1 - - coins' (a dash where the price was), awkward in all three languages.
- large-engine-1: While the shop is open the play HUD stays drawn behind it in the frame (shows 'Wave 1 of 8' next to 'Next: wave 2'); settings frame shows the title's name partly behind the panel.
- large-engine-1: On the title the Start button is always drawn blue (primary style), so with focus on another entry two buttons look highlighted; the focused one is only outlined.
- large-engine-1: Up on the shop menu's first entry does nothing (no wrap); not wrong, but Down x3 is the only way to Next wave.
- large-engine-2: Pause menu: Enter on Resume does nothing (its action 'resume' never reaches the rules); only Escape resumes.
- large-engine-2: At tick 0 a saved run is not yet known to the rules, so a script input at tick 0 sees no Continue on the title (the README admits this).
- large-engine-2: A bolt cannot hit an enemy that has passed x = 2 (the README admits this).
- large-engine-2: The README commands use --settings ./kept and --frame frame.png relative to work/, so running them exactly as written writes into the game folder; I used scratch paths with the same flags.
- large-engine-3: Shop shows Mason as '10/10: done' with no price whenever the gate is at full health, though the brief asks for each thing with its price (Mason can be bought any number of times).
- large-engine-3: Start (title) and Next wave (shop) are always drawn as bright blue filled buttons, so the focused entry is marked only by a thin outline and can be mistaken for the unfocused primary button.
- large-engine-3: Bolts in flight are removed when the shop opens (README says so); the brief does not ask for this, but it is harmless.
- large-engine-4: Every frame (title, settings, shop, won, lost, under the play HUD) shows a button labelled with the raw string key '@hud.pause', in no language.
- large-engine-4: Kept best/saved run reach the state one tick late: at tick 0 the title has no Continue even with a saved run, so a Down at tick 0 lands on Settings once Continue appears at tick 1.
- large-engine-4: Enter on the Volume line cycles it and wraps 10 -> 0 (Left/Right clamp correctly).
- large-engine-4: On title and shop the first or last menu button is drawn in solid blue (primary style) whatever the focus, so in the shop 'Next wave' looks selected while the focus outline is elsewhere.
- large-engine-4: The README headless/frame commands put the kept folder inside work/.data and the frame in the current folder; harmless, but a judge must change the folder to keep work untouched.
- large-engine-5: State printed at tick 0 (--at 0) has null for most keys and the raw text 'Best: {work.best}'.
- large-engine-5: The play HUD panel (Score/Coins/Wave/Gate) is drawn on the title, settings and shop frames too.
- large-engine-5: The shop's selected entry on opening is inconsistent: after wave 2 it opened on Next wave (menu_index 3), after waves 1 and 3 on Quick hands.
- large-engine-5: Bolts in flight are removed when the shop opens (README admits it).
