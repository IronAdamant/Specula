# Measured leverage: the runs

Folder: `lv2g`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the large runs: 93c469831d
- large: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| large-engine-1 | 1,004,823 | 1.73 | 133,825 | 4,760,659 | 72,301 | 58 | 59 | 11 | done | 24 | 16,105 / 16,120 | 31 / 31 | 2 / 2 | 6 | 1,497 (867 at the start) |
| large-engine-2 | 2,319,609 | 3.60 | 221,316 | 13,711,976 | 134,364 | 107 | 125 | 21 | done | 45 | 8,201 / 21,777 | 31 / 31 | 2 / 2 | 5 | 1,728 (867 at the start) |
| large-engine-3 | 1,215,657 | 2.02 | 145,667 | 6,308,126 | 80,559 | 69 | 71 | 16 | done | 40 | 6,011 / 20,796 | 30 / 31 | 2 / 2 | 7 | 1,537 (867 at the start) |
| large-engine-4 | 1,856,988 | 3.03 | 200,012 | 9,860,050 | 124,202 | 83 | 86 | 21 | done | 46 | 10,128 / 25,270 | 29 / 31 | 2 / 2 | 5 | 2,072 (867 at the start) |
| large-engine-5 | 1,156,324 | 1.93 | 149,686 | 6,059,238 | 72,665 | 65 | 77 | 13 | done | 35 | 7,596 / 18,590 | 30 / 31 | 2 / 2 | 4 | 1,616 (867 at the start) |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| large | engine | 5 of 5 | 1,510,680 | 1,004,823 to 2,319,609 | 16 | 30.2 / 31 | 50,023 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| large | engine | 1,510,680 | no judged run | - | - | 30.2 / 31 | - |

Judging: 5 judging(s) of 5 run(s), 3,123,396 weighted tokens in all.

At the runner's list prices: 12.31 USD for the builds, 10.59 USD for the judging shown above (22.90 USD in all).

## Items that failed

- large-engine-3 **l13**: Shop opens at wave 1's end (screen shop, enemies and bolts empty, Escape does nothing) with texts [Shop, Coins: 5, Next: wave 2 of 8, Quick hands: level 0 of 2, 5 coins, Heavy bolts: level 0 of 1, 12 coins, Mason: gate 10 of 10, full, Next wave]: Mason's price (4 coins) is not shown whenever the gate is full, which is the usual case after wave 1.
- large-engine-4 **l22**: Escape pauses (enemies, bolts and guard frozen ticks 101-199) and Resume continues from the same state; but Title from the pause gives screen 'bounce' (not title), with enemies still in the state, and its Start resumes the saved run's shop instead of a new run.
- large-engine-4 **l25**: Start on the title removed the save (saved false, no Continue next run); lost and won runs removed it; Title from the pause kept it. But Start on the title shown after pause->Title did not remove the save: it opened the saved run's shop.
- large-engine-5 **l22**: Escape at tick 300 shows paused [Paused, Resume, Title]; enemies and bolts frozen through tick 400; Resume continues from the same positions. But Title from the pause gives screen 'bridge' (texts [name, Best: 0, Title]), a screen not in the brief. It needs a second Enter to reach the title screen.

## What each judge found beyond the list

- large-engine-1: Escape in the shop opens the Pause menu (the brief says Escape does nothing there); README admits it.
- large-engine-1: After Enter on a won screen, Enter on Start of the title that comes up does nothing (screen stays title for 200 ticks); after Title from the pause, Down+Enter on Continue also does nothing in the same run. A new run can only begin in a later headless run.
- large-engine-1: The play HUD ('Score : 0 Pieces : 0 Vague 0 sur 8 Porte : 10') is drawn over the title, settings and shop frames, but is not in texts; on the title it says wave 0.
- large-engine-1: On the title after giving up from the pause, the report still lists the old run's enemies and bolts.
- large-engine-1: In title and shop frames a non-chosen button is drawn filled blue (Start, Next wave) besides the focus ring on the chosen one, which makes the marked entry less clear.
- large-engine-1: The settings screen's dimmed title text shows through behind the settings panel.
- large-engine-2: Escape in the shop is not ignored: after Escape there, Down/Enter on Next wave did nothing for 220+ ticks (game stalls until Escape is pressed again); the brief says Escape does nothing there. README admits it.
- large-engine-2: Every frame of title, settings and shop (and under the HUD in play) shows an unresolved '@hud.pause' label top-left, a raw string key, untranslated in every language.
- large-engine-2: Title and shop frames paint one entry blue (Start, Next wave) besides the focus ring on the chosen entry, so two entries look marked.
- large-engine-2: menu_index is null on the play and paused screens.
- large-engine-2: Bolts still in flight when the shop opens stay frozen and carry into the next wave; enemies stay in the state on the lost screen.
- large-engine-3: The shop's Mason line drops its price ('full') whenever the gate is at its starting health, though Mason is always 4 coins.
- large-engine-3: Escape while paused does not resume (stays paused), though actions.json calls it 'Pause and resume' and the code means it to.
- large-engine-3: menu_index prints 0 on the play screen, though README says it is null there.
- large-engine-3: 'best' prints null at tick 0 (README admits it is set from tick 1).
- large-engine-3: In the settings frame the title's name shows through and over the edge of the settings panel (overlapping text).
- large-engine-3: On title and shop the Start / Next wave buttons are always filled blue (primary style), which can be mistaken for the chosen entry; the chosen one is marked only by an outline.
- large-engine-3: The full 8-wave win could only be reached here with the test option --set gate=N; README says the builder could not find an unaided win either.
- large-engine-4: Escape in the shop (and on the won/lost screens) opens a hidden engine pause: the next Enter is swallowed and a purchase is not made (Escape then Enter on Quick hands left coins 35; only the second Enter bought). The brief says Escape does nothing in the shop. The engine also warns that paused.json is drawn under shop/won.
- large-engine-4: Title from the pause leads to an engine 'bounce' screen: screen value not in the brief's list, its Start continues a saved run, and the previous run's enemies stay in the printed state.
- large-engine-4: In the menu frames a second button (Start on the title, Next wave in the shop) is always filled blue while the chosen entry only has an outline, so the mark is ambiguous.
- large-engine-4: Numbers in texts use thousands separators ('Best: 1,540', French '1 660') where the brief shows 'Best: 120'; harmless but differs from the brief's form.
- large-engine-4: Language and difficulty stop at the ends of their lists rather than cycling; allowed by the list item but noted.
- large-engine-5: Escape in the shop (and on title, won and lost) opens the pause instead of doing nothing, as the README admits; the brief says Escape does nothing in the shop.
- large-engine-5: Title from the pause leads to an extra 'bridge' screen (reported screen value 'bridge', not in the brief's list) that needs another Enter.
- large-engine-5: The settings screen is drawn over the title, and the game's name shows partly hidden behind the settings panel.
- large-engine-5: The first walker of a wave does not move on the tick it appears (x stays 20 for its first tick), so it trails its own 1.5 s spacing by one tick of walking (2.225 vs 2.25 units).
