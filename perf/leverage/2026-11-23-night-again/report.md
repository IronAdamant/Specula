# Measured leverage: the runs

Folder: `lv2q`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the night runs: 1dc2f432d0
- night: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| night-engine-1 | 563,427 | 1.02 | 81,175 | 2,311,563 | 46,164 | 37 | 40 | 19 | done | 37 | 4,742 / 9,066 | 34 / 34 | 2 / 2 | 4 | 2,756 (877 at the start) |
| night-engine-2 | 359,002 | 0.71 | 64,664 | 1,024,435 | 35,148 | 22 | 27 | 6 | done | 13 | 2,381 / 6,967 | 34 / 34 | 2 / 2 | 4 | 1,520 (877 at the start) |
| night-engine-3 | 289,548 | 0.60 | 58,177 | 613,992 | 31,087 | 15 | 14 | 5 | done | 9 | 1,372 / 9,353 | 34 / 34 | 2 / 2 | 4 | 1,856 (877 at the start) |
| night-engine-4 | 553,793 | 1.02 | 81,413 | 2,073,140 | 48,946 | 34 | 42 | 17 | done | 25 | 5,032 / 16,032 | 34 / 34 | 2 / 2 | 5 | 1,622 (877 at the start) |
| night-engine-5 | 590,538 | 1.09 | 88,566 | 2,225,028 | 51,469 | 35 | 50 | 12 | done | 30 | 4,583 / 12,180 | 33 / 34 | 2 / 2 | 5 | 1,790 (877 at the start) |
| night-plain-1 | 129,922 | 0.28 | 25,077 | 138,139 | 16,953 | 7 | 8 | 2 | done | - | - | 34 / 34 | 2 / 2 | 2 | 655 |
| night-plain-2 | 139,615 | 0.31 | 27,155 | 141,749 | 18,300 | 7 | 8 | 2 | done | - | - | 33 / 34 | 2 / 2 | 4 | 686 |
| night-plain-3 | 124,973 | 0.28 | 24,471 | 108,368 | 16,710 | 6 | 6 | 2 | done | - | - | 34 / 34 | 2 / 2 | 1 | 627 |
| night-plain-4 | 135,203 | 0.29 | 25,863 | 164,684 | 17,282 | 8 | 9 | 2 | done | - | - | 34 / 34 | 2 / 2 | 4 | 583 |
| night-plain-5 | 124,057 | 0.27 | 24,428 | 132,907 | 16,047 | 7 | 8 | 2 | done | - | - | 34 / 34 | 2 / 2 | 2 | 634 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| night | engine | 5 of 5 | 471,262 | 289,548 to 590,538 | 12 | 33.8 / 34 | 13,943 |
| night | plain | 5 of 5 | 130,754 | 124,057 to 139,615 | 2 | 33.8 / 34 | 3,868 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| night | engine | 471,262 | 130,754 | 3.60 | 4.09 | 33.8 / 34 | 33.8 / 34 |

Judging: 10 judging(s) of 10 run(s), 3,889,944 weighted tokens in all.

At the runner's list prices: 5.88 USD for the builds, 15.13 USD for the judging shown above (21.01 USD in all).

## Items that failed

- night-engine-5 **n28**: Replay at seed 7, Hard, stove moved to (0,1): replaying true, Replay text, arrivals of the kept night, Replay over with 24/3/49, ledger unchanged. But the state's grid during the replay is the current layout grid (stove at 0,1), not the night's grid (stove at 0,0, which the frame draws).
- night-plain-2 **n28**: Replay with seed 77, Easy setting and a changed grid: night, replaying true, Replay text, arrivals = kept, summary Replay over 28/3/43 = night, ledger and kept night unchanged, frame draws the kept grid; BUT the state's grid key during the replay shows the current layout grid (stove at (0,0) missing), not the grid the night had.

## What each judge found beyond the list

- night-engine-1: The input word none does not clear Needs a stove and a table, although the brief says the words stay until the next word is given in Layout (any other word clears them).
- night-engine-1: During a replay the state's grid key shows the player's current layout, not the grid being replayed (the frame and the result use the kept night's grid).
- night-engine-1: Ledger lines are wider than the menu panel in the frame (Hard sticks out past its right edge).
- night-engine-1: Every frame shows the engine's own Pause button at the top left, which the game does not use.
- night-engine-2: A headless run with --ticks 0 prints every key as null (plus engine warnings) instead of the title state.
- night-engine-2: Quit does not end a headless run: the game freezes with quit true while tick keeps counting (README admits this); in a window the program does not close.
- night-engine-2: During a replay the state's grid key shows the current layout grid, not the replayed night's grid (the frame and the result use the right one).
- night-engine-2: speed is none on the summary and customers is emptied there; queue is 0 on the summary (defensible, but the summary is part of the night).
- night-engine-3: During a replay the state's grid key shows the current Layout grid, not the replayed night's grid (the frame and the result use the night's grid).
- night-engine-3: The Layout cursor is drawn with an extra horizontal bar through the middle of the cell.
- night-engine-3: On the Settings frame the large chosen entry 'Difficulty: Normal' overflows the text panel.
- night-engine-3: A cooking customer (orange) on a table (brown) has low contrast in the night frame.
- night-engine-4: With --ticks 0 (or --at 0) the printed state has every value null and texts [] instead of the title screen's state.
- night-engine-4: During a replay the state's 'grid' key shows the current layout grid, not the grid the replayed night uses (the frame and the result use the right one).
- night-engine-4: Customer ids count turned-away arrivals, so ids of customers in the stall skip numbers (e.g. 1,2,3,4 then 6).
- night-engine-4: Enter on Volume/Difficulty in Settings also steps the value (the brief names only Left/Right); README admits it.
- night-engine-4: Menus and texts are drawn in a small panel at the top right on every screen rather than as a screen of their own; title not centred (cosmetic).
- night-engine-5: With --ticks 0 the state prints screen null, menu_index null and texts [] instead of the title screen.
- night-engine-5: During a replay the reported grid key is the current layout grid, not the replayed night's grid (the frame draws the night's grid).
- night-engine-5: Enter on Volume or Difficulty in Settings also changes the value (README admits it); the brief gives only Left/Right.
- night-engine-5: Settings Left/Right show in the state at the tick they are given, but game words show one tick later, so input timing is not the same everywhere.
- night-engine-5: The arrivals random stream carries on between nights in one run, so the same seed gives different arrivals for a second night in the same run.
- night-plain-1: The default --data is save/ inside the game folder, so a run without --data writes inside work/.
- night-plain-1: Selected piece is shown only as the Piece: text in the Layout frame, with no picture of the piece.
- night-plain-2: During a replay the state's grid (and stock) report the current layout grid, not the grid being replayed.
- night-plain-2: Layout frame draws Enter: open the stall twice, and Escape: back to title overlaps grid cell (0,0).
- night-plain-2: Summary frame draws the stall unscaled at x 460 so its right columns are cut off the image edge.
- night-plain-2: Each headless run takes about 3 s even for a few ticks (store rewritten after every word).
- night-plain-3: During a replay the state key grid shows the current layout grid, not the grid being replayed (frame and results use the kept grid correctly).
- night-plain-4: The frame font is uppercase only, so every text on the PNG is drawn in capitals (README admits it).
- night-plain-4: There is no real-time play mode; play is a text console driven by words and tick counts.
- night-plain-4: In the night frame a customer marker covers the table letter T, so an occupied table is told only by its brown color.
- night-plain-4: Nothing is kept unless --store is given; without it every run starts fresh.
- night-plain-5: Frame text is drawn in upper case only (e.g. 'NIGHT MARKET'), not as the strings in texts.
- night-plain-5: The paused frame shows only Paused/Resume/Title, not the night's coins, time and speed.
