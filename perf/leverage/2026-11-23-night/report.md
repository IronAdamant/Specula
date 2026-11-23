# Measured leverage: the runs

Folder: `lv2n`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the night runs: fdf25620d7
- night: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| night-engine-1 | 1,137,107 | 1.92 | 134,751 | 5,584,098 | 82,058 | 63 | 64 | 17 | done | 58 | 8,217 / 21,356 | 31 / 34 | 2 / 2 | 6 | 2,512 (876 at the start) |
| night-engine-2 | 887,580 | 1.51 | 115,827 | 4,405,589 | 60,453 | 56 | 67 | 13 | done | 30 | 4,029 / 16,582 | 32 / 34 | 2 / 2 | 5 | 1,581 (876 at the start) |
| night-engine-3 | 1,834,602 | 2.89 | 185,548 | 10,527,444 | 109,994 | 95 | 112 | 23 | done | 63 | 7,544 / 14,214 | 31 / 34 | 2 / 2 | 4 | 1,723 (876 at the start) |
| night-engine-4 | 523,399 | 1.03 | 90,247 | 1,505,594 | 52,009 | 28 | 34 | 9 | done | 19 | 2,573 / 7,317 | 33 / 34 | 2 / 2 | 4 | 1,708 (876 at the start) |
| night-engine-5 | 1,099,740 | 1.86 | 134,783 | 5,422,084 | 77,817 | 65 | 69 | 16 | done | 48 | 5,655 / 13,978 | 34 / 34 | 2 / 2 | 4 | 2,139 (876 at the start) |
| night-plain-1 | 167,306 | 0.36 | 29,940 | 189,347 | 22,190 | 8 | 8 | 2 | done | - | - | 34 / 34 | 2 / 2 | 3 | 681 |
| night-plain-2 | 140,823 | 0.30 | 27,045 | 173,157 | 17,941 | 8 | 10 | 2 | done | - | - | 34 / 34 | 2 / 2 | 2 | 721 |
| night-plain-3 | 148,026 | 0.32 | 27,945 | 148,730 | 19,645 | 7 | 9 | 2 | done | - | - | 34 / 34 | 2 / 2 | 2 | 598 |
| night-plain-4 | 154,928 | 0.34 | 28,802 | 151,791 | 20,750 | 7 | 9 | 2 | done | - | - | 34 / 34 | 2 / 2 | 2 | 745 |
| night-plain-5 | 130,807 | 0.29 | 25,595 | 136,314 | 17,037 | 7 | 9 | 2 | done | - | - | 33 / 34 | 2 / 2 | 4 | 651 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| night | engine | 5 of 5 | 1,096,486 | 523,399 to 1,834,602 | 15 | 32.2 / 34 | 34,052 |
| night | plain | 5 of 5 | 148,378 | 130,807 to 167,306 | 2 | 33.8 / 34 | 4,390 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| night | engine | 1,096,486 | 148,378 | 7.39 | 8.30 | 32.2 / 34 | 33.8 / 34 |

Judging: 10 judging(s) of 10 run(s), 3,943,843 weighted tokens in all.

At the runner's list prices: 10.82 USD for the builds, 15.00 USD for the judging shown above (25.83 USD in all).

## Items that failed

- night-engine-1 **n07**: Inputs 2:right,3:right,4:right moved the cursor only to (1,0), and 6:up,7:up only to (1,1). Even 5:right,7:right,... with nothing between moved once in total. Only with 'none' between each word did it reach (7,4) and back to (0,0), stopping at the edges, with menu_index 0 and still in layout. So the cursor does not move one cell for each word.
- night-engine-1 **n08**: 2:next_piece,4:next_piece,6:next_piece stopped at table (texts 'Piece: Table'), and three prev_piece did the same. Only with 'none' between did it cycle stove,table,lamp,stove and back.
- night-engine-1 **n24**: Escape pauses (texts Paused, Resume, Title; clock stays 8). Enter on Resume continues from clock 8, and Title gives up with the ledger unchanged. But Escape in Paused never resumes: 400:escape, also with 'none' before and after, and 500/501/502:escape, left the screen paused up to tick 700.
- night-engine-2 **n07**: With a none between words the cursor moves one cell a word, up raises y, stops at x 0/7 and y 0/4, menu_index stays null, screen stays layout. But the same word given twice in a script without a none between (2:right,3:right,4:right, or every 2 ticks 25:right..41:right, 11:up..19:up) moves only one cell: not one cell for each Up/Down/Left/Right.
- night-engine-2 **n28**: Replay run with seed 7, Hard and another grid: night, replaying true, "Replay", seed-1 arrivals, Replay over 24/3/49, ledger and kept night unchanged; the frame draws the kept grid. But the state's grid key during the replay is the current editor grid (table(1,1), stove(2,1)), not the night's grid, while customers sit at (1,0).
- night-engine-3 **n07**: With 'none' between words the cursor moves one cell each way (up raises y) and stops at x 0/7, y 0/4, menu_index unchanged, screen stays layout. But 0:enter,2:right,3:right,4:right leaves cursor at (1,0), not (3,0): the same word given again without a different word between counts once (README admits it), against the brief's 'each acts once ... even when the same word was given at the tick before'.
- night-engine-3 **n08**: With none between: next_piece table, lamp, stove; prev_piece lamp, table, stove; Piece: text follows. But 2:next_piece,12:next_piece (ten ticks apart, no other word between) gives table, not lamp; 2:next_piece,4:next_piece,6:next_piece stays table. Repeated words are swallowed, so the cycle does not happen with a plain script.
- night-engine-3 **n12**: With none between: next_slot 1->2->3->1, save_slot copies grid and leaves it, load_slot on empty slot 2 nothing, load_slot slot 1 restores grid and stock (table 3->4). But 2:next_slot,3:next_slot gives slot 2, not 3: the repeated word counts once, so the cycle fails with a plain script.
- night-engine-4 **n28**: Replay after editing the grid (stove (0,0) removed), Hard and seed 7: screen night, replaying true, 'Replay' text, seed-1 arrivals, summary 'Replay over' 40/4/47 same as the night, ledger unchanged, and the frame draws the kept grid. But the state's grid key during the replay is the edited current grid (no stove at (0,0)), not the night's grid, so the state does not show the same grid as the night had.
- night-plain-5 **n33**: title, settings and summary frames show their texts with the chosen entry in yellow; but the ledger frame does not mark its chosen entry Back (renderer only highlights title/settings/paused), and in paused.png the queued customer boxes are drawn over the 18:50 and Speed: Fast texts, which are partly hidden.

## What each judge found beyond the list

- night-engine-1: Headless input: a game word given again (at the next tick or later, with no different word between) acts only once. The brief says each word acts every time it is given; the README admits this.
- night-engine-1: Escape on the Paused screen does nothing (should resume).
- night-engine-1: During a replay the state's 'grid' key shows the current Layout grid, not the replayed night's grid. The frame and the play use the kept grid.
- night-engine-1: The night HUD panel shows a stray empty focused button below the speed line.
- night-engine-1: README notes that a word at the last tick (tick N with --ticks N) is refused, and that with --ticks 0 the kept things are not yet loaded.
- night-engine-1: On the summary, customers' 'wait' shows clock 360 minus arrival, a minute that was never played (cosmetic).
- night-engine-2: The headless run does not honour 'each acts once, even when the same word was given at the tick before': a Layout/Night word (right, up, place, enter, ...) repeated in a script counts once unless a none is put between, even when 2 or more ticks apart (README admits it). Enter in Layout right after the title's Enter is also ignored without a none between.
- night-engine-2: A word at tick 0 does nothing (0:enter leaves the title); the state at tick 0 has screen null.
- night-engine-2: Quit in a headless run does not end the run: the printed tick is the full --ticks count (30), not the tick of Quit.
- night-engine-2: During a replay the state's grid key shows the editor grid, not the grid being replayed.
- night-engine-2: On the title and paused screens the first button is always drawn filled blue (primary style) while the focus ring is elsewhere, so two entries look marked.
- night-engine-3: Any input word given again without a different word in between is ignored (cursor words, next_piece, prev_piece, next_slot, place, erase, save/load, speed words), even many ticks apart; the brief requires each to act. The README admits this.
- night-engine-3: On the title and paused frames the first entry is always drawn filled blue (primary style) while the focus is shown only by a ring, so two entries look highlighted when the focus is not on the first.
- night-engine-3: menu_index is null on layout and night screens (the brief does not say what it should be there).
- night-engine-3: Cooking customers (tan on a brown table) are hard to tell from an empty table in the night frame.
- night-engine-4: Quit in a headless run does not end the run there: down x3, enter logged 'quit' at tick 3 but the run printed tick 100 (the --ticks given) rather than stopping at the Quit.
- night-engine-4: During a replay the reported grid is the current layout, not the replayed night's grid, so the state contradicts the frame and the customers' tables.
- night-engine-4: The Layout cursor in the state keeps its last layout value during a night (e.g. cursor (1,0) shown in a night); harmless but not reset.
- night-engine-4: A stray empty button (the hidden ping button) is drawn at the bottom right of every frame (README admits it).
- night-engine-5: Every frame shows a column of 8 small unlabelled buttons at the bottom right: the game's hidden counter buttons (its way of making repeated words count) are drawn on screen.
- night-engine-5: A headless run with --ticks 0 and a kept folder prints defaults (ledger [], replay_ready false, no Replay on the title), not the kept things; kept values only arrive at the first tick (the README admits this).
- night-engine-5: The kept folder also holds engine bookkeeping (engine.instances, projects.json) and the c1..c7 press counters in settings.json.
- night-engine-5: During a replay the state's grid and stock show the replayed night's grid, not the layout grid; the README says so.
- night-plain-1: Customer squares cover the table's letter in night frames (table shown only by its brown border); no legend for the customer state colours.
- night-plain-1: Arrival generator is seeded once per game start, so a second night in the same headless run gets a different list from the first with the same seed (defensible reading of the brief).
- night-plain-1: Window mode untested by the builder (stated in README); not checked by the judge either.
- night-plain-2: Eating-state colour was only seen in the legend; no captured frame happened to contain an eating customer (drawing code uses the same colour table).
- night-plain-2: README's own example headless command defaults the save file next to the game script if --save is left out, so a casual run writes into work/.
- night-plain-3: Customer numbers on the night frame are drawn as id mod 10, so customers 1 and 11 look the same apart from their state colour.
- night-plain-3: On the paused screen the frame shows only the menu, not the night underneath it (allowed by the brief, but the night is hidden).
- night-plain-4: Menus are drawn with a '> ' marker, but the bitmap font has no '>' glyph, so only the colour shows the chosen entry.
- night-plain-4: Opening Layout also resets the selected piece to Stove; the brief only asks for the cursor and slot to reset (the README states this choice).
- night-plain-5: Paused frame: night texts are moved to the top-left where the queue column is drawn, so queue boxes and numbers overlap the time and speed text.
- night-plain-5: Ledger frame does not mark its chosen menu entry (Back).
- night-plain-5: The > menu marker is not in the 5x7 font and draws as a blank; the chosen entry is shown only by colour and indent.
- night-plain-5: Ledger entries in the state hold difficulty capitalised (Normal) while the top-level difficulty key is lowercase.
