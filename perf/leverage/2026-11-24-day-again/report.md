# Measured leverage: the runs

Folder: `lv2t`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the day runs: 5c416dc37b
- day: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| day-engine-1 | 721,140 | 1.38 | 107,794 | 2,287,669 | 71,529 | 28 | 31 | 11 | done | 27 | 3,989 / 20,585 | 60 / 61 | 2 / 2 | 3 | 2,766 (879 at the start) |
| day-engine-10 | 948,992 | 1.74 | 133,967 | 3,636,316 | 83,584 | 37 | 45 | 14 | done | 32 | 5,154 / 15,310 | 61 / 61 | 2 / 2 | 2 | 3,098 (879 at the start) |
| day-engine-2 | 1,193,377 | 2.17 | 164,949 | 4,666,692 | 104,109 | 46 | 60 | 16 | done | 33 | 7,765 / 52,631 | 53 / 61 | 2 / 2 | 4 | 2,947 (879 at the start) |
| day-engine-3 | 1,311,905 | 2.16 | 146,480 | 6,848,187 | 88,804 | 67 | 78 | 17 | done | 36 | 4,695 / 15,538 | 58 / 61 | 2 / 2 | 2 | 2,751 (879 at the start) |
| day-engine-4 | 870,648 | 1.59 | 121,749 | 3,374,157 | 76,213 | 38 | 43 | 13 | done | 30 | 6,580 / 26,462 | 60 / 61 | 2 / 2 | 5 | 3,162 (879 at the start) |
| day-engine-5 | 884,732 | 1.65 | 130,926 | 3,142,327 | 81,372 | 37 | 41 | 13 | done | 34 | 6,718 / 25,813 | 56 / 61 | 2 / 2 | 3 | 2,893 (879 at the start) |
| day-engine-6 | 719,780 | 1.34 | 106,347 | 2,547,587 | 66,421 | 34 | 39 | 10 | done | 28 | 5,473 / 7,302 | 60 / 61 | 2 / 2 | 3 | 2,793 (879 at the start) |
| day-engine-7 | 680,966 | 1.31 | 112,909 | 2,163,549 | 64,698 | 31 | 35 | 10 | done | 18 | 3,647 / 26,919 | 60 / 61 | 2 / 2 | 5 | 2,083 (879 at the start) |
| day-engine-8 | 733,026 | 1.41 | 113,931 | 2,233,960 | 73,446 | 27 | 30 | 14 | done | 20 | 3,742 / 23,069 | 61 / 61 | 2 / 2 | 3 | 3,103 (879 at the start) |
| day-engine-9 | 962,742 | 1.72 | 126,786 | 3,998,929 | 80,878 | 47 | 53 | 14 | done | 28 | 3,461 / 15,699 | 61 / 61 | 2 / 2 | 0 | 2,063 (879 at the start) |
| day-plain-1 | 330,693 | 0.69 | 54,717 | 520,284 | 42,055 | 13 | 20 | 5 | done | - | - | 61 / 61 | 2 / 2 | 3 | 1,317 |
| day-plain-10 | 306,120 | 0.65 | 52,536 | 448,358 | 39,124 | 12 | 16 | 4 | done | - | - | 60 / 61 | 2 / 2 | 2 | 1,413 |
| day-plain-2 | 337,625 | 0.70 | 55,044 | 592,466 | 41,916 | 14 | 18 | 5 | done | - | - | 60 / 61 | 2 / 2 | 2 | 1,474 |
| day-plain-3 | 396,568 | 0.79 | 60,246 | 888,249 | 46,489 | 19 | 28 | 5 | done | - | - | 61 / 61 | 2 / 2 | 2 | 1,547 |
| day-plain-4 | 336,144 | 0.70 | 55,747 | 536,366 | 42,566 | 13 | 22 | 5 | done | - | - | 61 / 61 | 2 / 2 | 2 | 1,334 |
| day-plain-5 | 387,489 | 0.80 | 63,388 | 669,413 | 48,264 | 15 | 23 | 5 | done | - | - | 58 / 61 | 2 / 2 | 4 | 1,329 |
| day-plain-6 | 387,968 | 0.79 | 60,253 | 775,200 | 47,028 | 17 | 23 | 5 | done | - | - | 61 / 61 | 2 / 2 | 3 | 1,475 |
| day-plain-7 | 272,054 | 0.58 | 47,971 | 330,155 | 35,816 | 10 | 14 | 4 | done | - | - | 61 / 61 | 2 / 2 | 2 | 1,434 |
| day-plain-8 | 338,140 | 0.71 | 56,225 | 546,102 | 42,651 | 13 | 20 | 5 | done | - | - | 61 / 61 | 2 / 2 | 2 | 1,366 |
| day-plain-9 | 331,216 | 0.69 | 54,535 | 524,692 | 42,117 | 13 | 20 | 5 | done | - | - | 61 / 61 | 2 / 2 | 3 | 1,298 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| day | engine | 10 of 10 | 902,731 | 680,966 to 1,311,905 | 13 | 59.0 / 61 | 15,301 |
| day | plain | 10 of 10 | 342,402 | 272,054 to 396,568 | 5 | 60.5 / 61 | 5,660 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| day | engine | 902,731 | 342,402 | 2.64 | 2.82 | 59.0 / 61 | 60.5 / 61 |

Judging: 45 judging(s) of 20 run(s), 38,204,448 weighted tokens in all (25 of them earlier judgings of runs judged again).

At the runner's list prices: 23.57 USD for the builds, 60.74 USD for the judging shown above, and 50.49 USD for the earlier judgings of the runs judged again (134.80 USD in all).

## Judged more than once

The figures above are the last judging. Before it:

- day-engine-1: 48 / 61 (failed: d06, d10, d14, d18, d37, d39, d40, d41, d42, d43, d50, d59, d60). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-engine-10: no verdict. Judged again because: the first judge ran out of its 30 minutes and left no verdict; 60 minutes this time
- day-engine-10: 59 / 61 (failed: d14, d37). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-engine-2: 58 / 61 (failed: d48, d49, d50). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-engine-3: 52 / 61 (failed: d14, d37, d39, d41, d47, d50, d58, d59, d60). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-engine-4: no verdict. Judged again because: the first judge ran out of its 30 minutes and left no verdict; 60 minutes this time
- day-engine-4: 61 / 61. Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-engine-5: 61 / 61. Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-engine-6: no verdict. Judged again because: the first judge ran out of its 30 minutes and left no verdict; 60 minutes this time
- day-engine-6: 60 / 61 (failed: d60). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-engine-7: 43 / 61 (failed: d10, d14, d32, d36, d37, d38, d39, d42, d43, d44, d47, d50, d52, d53, d54, d58, d59, d60). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-engine-8: 58 / 61 (failed: d14, d37, d39). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-engine-9: no verdict. Judged again because: the first judge ran out of its 30 minutes and left no verdict; 60 minutes this time
- day-engine-9: 59 / 61 (failed: d37, d43). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-1: 61 / 61. Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-10: 61 / 61. Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-2: 60 / 61 (failed: d07). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-3: 60 / 61 (failed: d07). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-4: 61 / 61. Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-5: 58 / 61 (failed: d09, d16, d60). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-6: no verdict. Judged again because: the first judge ran out of its 30 minutes and left no verdict; 60 minutes this time
- day-plain-6: 59 / 61 (failed: d07, d26). Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-7: 61 / 61. Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-8: 61 / 61. Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes
- day-plain-9: 61 / 61. Judged again because: judged again with the shared input pack, the same for both arms, and 60 minutes

## Items that failed

- day-engine-1 **d18**: S23-a: Paused texts [Paused, Resume, Title]; clock stands (1 over 200 paused ticks); Title leaves the slot as saved (coins 50, no apples). But Resume from Paused over the Morning returns with menu_index 0, not the 1 it had (m: menu_index 1 -> p1 -> m2: menu_index 0; Escape twice same): not the same state.
- day-engine-2 **d14**: 'Not enough coins' part seen (S06 c3: coins/stock unchanged, line added, cleared by the next word left). The 99-unit cap (S08, a long bank search) was not run.
- day-engine-2 **d32**: S26 (cook/tourist price thresholds) not run; taken prices equalled the stall price in the checker but the 4/5/6/7 thresholds were not observed.
- day-engine-2 **d38**: Clock 480 gives evening with aisle/queue/tills empty and walked_out/passed unchanged from 479, but on seeds tried nobody was present at 479, so the 'gone without paying' case was not seen.
- day-engine-2 **d39**: S18d5 seed 1: tills has two entries, both busy in one minute (max 2); the queue never formed, so 'till 1 takes the front first' was not observed.
- day-engine-2 **d40**: S19 (Sign) not run.
- day-engine-2 **d41**: Failed order seen (S09-failed: rep 50->45, coins 30, 'Order failed'); delivery (search scenario) and not-taken not run.
- day-engine-2 **d47**: S21: Sign bought coins 30->0, 'Sign: owned', again no change, Second till/Cool box refused with 'Not enough coins'. Extra crates -> crate_limit 12 (S20) not run.
- day-engine-2 **d50**: In-run ordering right (S15-a r6: 65,59,53,0 hard 1,0 normal 3). A later run on the same folder (S15-b, S13-b) shows 4 bogus records 'Score 0, 0 days, Easy' instead: the kept records [[0,1,2,1]] are read back wrong.
- day-engine-3 **d58**: S07 frame with 8 crates (6 front, 2 back, cursor (3,1)): the drawn stall is 4 coloured squares in one row, no 6x2 grid, no back row, no drawn cursor; only the text lines (Front/Back with [..]) carry the layout. The drawing does not show the crates where the state says.
- day-engine-3 **d59**: S17d6 seed 1 at clock 95 (6 front crates, 3 visitors at x 1,2,3, a queue of 1, till busy): drawing shows 4 crates and 2 visitor squares, not where the state says; visitor types only in text codes (Co/To). Time, speed, coins, taken, served, reputation shown as text.
- day-engine-3 **d60**: Title, paused frames show their texts, but the chosen entry is not marked (title frame menu_index 0, nothing highlighted; paused frame likewise; morning frame shows no marked row).
- day-engine-4 **d04**: S02-a: starts English/Normal/7 days and Back/Escape go to the title, but at the bounds the texts break: Left at Easy keeps difficulty easy while the text turns 'Difficulty: Normal'; Right at 3 days keeps length 3 while the text says 'Season: 7 days'; Left at 7 days keeps 7 while the text says 'Season: 3 days'. Also, Enter on Settings from the title at tick 36 flipped language en->es with no Left/Right (tick 37 shows es).
- day-engine-5 **d06**: Seen in Spanish only: Settings (switches at once: Ajustes, Idioma: Espanol...) and title (Nueva temporada, Records, Sellos, Ajustes, Salir). The other screens in Spanish were not run (judge budget).
- day-engine-5 **d17**: S24: Continue with seed 7 on the folder saved at Morning 3 reproduced day, coins, stock, stall, prices, wholesale, order, forecast exactly (hashes equal to seed-1 unbroken run). Enter on an empty slot under Continue was not exercised.
- day-engine-5 **d59**: f_day.png (S17d6 clock 200): stall and texts drawn, but the one visitor is a plain grey square; no type colouring in assets/ui/main.json or lib.rs, so the four types are not told apart; queue/tills not distinguishable.
- day-engine-5 **d60**: Viewed only the Morning frame (texts drawn, chosen row in bold); title, slots, settings, upgrades, paused, evening, season over frames not taken (judge budget).
- day-engine-5 **d61**: README says how to play and where kept things are, but its commands name <repo>/target/debug/examen.exe on the work folder with --settings <runs>/kept (outside where I may write); not run as written.
- day-engine-6 **d60**: Frames of title, morning (menu_index on a good row) and upgrades (menu_index 1) show the texts, but no entry or row is marked (no highlight, cursor or arrow). The brief asks for the chosen entry marked.
- day-engine-7 **d60**: Frames of title, slots, settings, morning, upgrades, paused, evening and season over show their texts, but no screen marks the chosen entry: paused frame with Title chosen (menu_index 1) looks the same as with Resume; README admits 'no marker for the chosen menu row'.
- day-plain-10 **d26**: Same seed/day gives the same list (S04 seed 1 twice) and the list is well formed (ids 1.., minutes increasing, 2/1/2/3 distinct wants); seed 1 vs 2 differ, but seeds 3 and 4 give the identical day-1 visitor list (73 visitors), as do 7/8 and 10/11 in S26: another seed does not give another list.
- day-plain-2 **d26**: List form holds (ids 1..n, minutes strictly increasing in 0..479, types valid, distinct wants 2/1/2/3) and same seed gives same list, seeds 1 and 2 differ; BUT among seeds 1..20 the day-1 lists of seeds 14 and 15 are identical (78 visitors), and of 17 and 18 too: another seed does not always give another list (Rng(seed) state is seed*G and splitmix adds G per draw, so seed+1 is the same stream shifted by one).
- day-plain-5 **d09**: S04-a, Enter on Slot 1: Morning has coins 50, rep 50, empty stock, reference prices, crate_limit 8, order null, 'Day 1 of 7', but state slot is 0 (not 1) and the season is saved in slots[2] (slot 3). game.py stores the 0-based row as slot and saves to slots[slot-1].
- day-plain-5 **d16**: S04-b on the folder run a left: title shows Continue, but the rows read 'Slot 1: Empty', 'Slot 2: Empty', 'Slot 3: Day 1 of 7, 50 coins' for a season started in Slot 1.
- day-plain-5 **d60**: Frames of settings, morning, paused, evening, season over show their texts and mark the chosen row; but fr_upgrades.png (menu_index 1) marks two rows, 'Next day' and 'Second till: 80', so the chosen row cannot be told.

## What each judge found beyond the list

- day-engine-1: Resuming from Paused resets the Morning's menu_index to 0 (the row the player was on is lost).
- day-engine-1: Visitors on the aisle are drawn as thin split bars over the aisle cells; the till's occupant is not clearly drawn in the day frame.
- day-engine-1: The 'none' word never reaches the game through the engine's runner, so 'Not enough coins' is not cleared by it (runner limitation noted in PACK.md, not counted against the game).
- day-engine-10: Frames are dim, flat boxes; the four visitor types and bread/honey crates are told apart only by colour (README says so).
- day-engine-10: In S23 the clock reached minute 2 one tick earlier (t284) than the pack's checkpoint (t285) after resuming from Paused; the pause still holds the clock and the part-minute is kept.
- day-engine-2: An ended season's slot is not removed from the kept file (kept.slot1 still holds the Morning save after bankruptcy), so a later run on the same place shows the slot and Continue again (S13-b).
- day-engine-2: Kept records are mis-read on load: [[0,1,2,1]] comes back as four records {score 0, days 0, easy, false} (model.rs merge of 'records' reads the list's first element as the item list).
- day-engine-2: Spanish 'Letrero: comprada' (gender mismatch), cosmetic.
- day-engine-2: Cheese crate colour and tourist colour are both yellow on the Day frame.
- day-engine-3: The frame's drawn stall has 4 cells in one row for a 6 x 2 stall, so 2 front crates and the whole back row never appear in the drawing.
- day-engine-3: No menu entry or row is marked in any frame (title, paused, morning, upgrades): menu_index is invisible in the view.
- day-engine-4: Settings: Left/Right at the end of a setting leaves the value unchanged but flips its text (Difficulty shows Normal while easy; Season shows 7 days while 3, and 3 days while 7).
- day-engine-4: Settings: opening Settings a second time with Enter in one run toggled the language (en -> es) with no Left/Right given.
- day-engine-4: The Settings frame shows an unresolved '@hud.pause' key in a box at the top left.
- day-engine-4: Large coin values are printed with a thousands separator ('Coins: 1,123'), a form the brief does not show.
- day-engine-4: The view is text-only (stall, aisle, queue and tills drawn as letters in a proportional font, so columns do not line up).
- day-engine-5: README admits several Up/Down/Left/Right words at one tick on the Settings screen can act on the old row (brief: same-tick words act one after another).
- day-engine-5: The word none does not clear 'Not enough coins' (S06 c6, S21 u2c); the pack attributes this to the runner, not the game.
- day-engine-5: Honey and bread crates are similar browns in the frame.
- day-engine-6: Frames never mark the chosen menu entry or row on any screen (title, slots, settings, morning, upgrades, paused).
- day-engine-6: Spanish upgrade row reads 'Cartel: comprada' (feminine) for a masculine noun; minor wording.
- day-engine-6: Continue in the same run after a given-up day brought back the Stall cursor (1,1) from the abandoned day (a later run gives (0,0)); cursor is not part of the save rules, minor.
- day-engine-7: The frames never mark the chosen menu entry or row (title, slots, settings, morning, upgrades, paused).
- day-engine-7: Stall frame: the cursor cell is filled plain grey, so you can't see the crate's colour under the cursor.
- day-engine-7: Day frame: the till is a plain grey box, so the visitor at the till is not shown by type.
- day-engine-7: On the Season over screen the state reports day 0, coins 0, reputation 0 and slot null.
- day-engine-7: Records read 'Score 0, 1 days, ...' (plural with 1); the brief gives only plural examples.
- day-engine-8: After Title from Paused, the title screen prints cursor {x:1,y:1} (the stall cursor left over); the brief says cursor is (0,0) when no season is being played (S23-a t288).
- day-engine-8: README's frame command contains a literal '...' in --inputs, so it cannot be pasted as written; the headless-run command above it is complete.
- day-engine-8: Frames are very dim (README admits it) and aisle/queue squares carry no letters; types are told apart only by colour.
- day-plain-1: The frame draws the chosen-row marker as '?' and the till occupant as '?4': a glyph the built-in font does not have (cosmetic; still marked).
- day-plain-1: Records read 'Score 0, 1 days, Hard, bankrupt' (no singular); the brief gives only plural examples.
- day-plain-1: The till box in the Day frame shows the visitor number but not its type.
- day-plain-10: The day's random source is seeded as seed*golden + day*C + const and advanced by the same golden step, so seed s+1's stream is seed s's shifted by one draw: visitor lists of adjacent seeds are identical (3/4, 7/8, 10/11) or the same list shifted a minute (1/2).
- day-plain-10: Evening frame: the good lines ('Flowers: sold 0, spoiled 0, left 0') run past the left column into the stall panel and over its 'QUEUE' label.
- day-plain-2: Random source: Rng(seed, salt, day) starts at seed*G + ... and each draw adds G, so seed n+1 is seed n's stream shifted by one; adjacent seeds can give identical visitor lists (day 1: seeds 14/15 and 17/18 among 1..20).
- day-plain-2: Window play mode was never run by the builder (README says so); not judged here.
- day-plain-3: The frame labels 'aisle', 'queue' and 'tills' are written in game.py, not taken from the string tables, so they stay English in Spanish.
- day-plain-3: The README says the tkinter window was never opened or tested; I did not open it either, because no window may be opened on this machine.
- day-plain-4: Records use the plural for one day ('Score 0, 1 days, Hard, bankrupt'); the brief gives only plural examples.
- day-plain-4: The window mode (window.py, tkinter) was not opened by me or, per its README, by the builder.
- day-plain-5: Slot numbering is off by one: state 'slot' is 0..2, a season from Slot 1 is saved into slot 3, from Slot 2 into slot 1; continuing slot 3 copies the season into slot 2, leaving two copies.
- day-plain-5: Because of the slot bug every pack scenario that continues 'slot 1' (S05 c/h, S23 c/c2, S24-continued) stalls on the slots screen.
- day-plain-5: The headless JSON goes to stdout in the console code page (cp1252 when piped), not UTF-8: Spanish accents are not valid UTF-8 bytes.
- day-plain-5: Upgrades frame highlights the row above the chosen one as well (two rows marked).
- day-plain-6: On the Season over screen (no season being played, slot null) the state still carries the ended season's day and coins (e.g. coins -6 after S13) instead of the brief's idle values (day 0, coins 0); the README admits this.
- day-plain-6: The random source is seeded linearly (seed*G + day*C + salt*K, G being splitmix's own increment), so seed s+1's stream is seed s's stream shifted by one draw: adjacent seeds give correlated wholesale prices and visitor lists (seed 1 and 2 share the first two visitors of day 1).
- day-plain-6: The tkinter window was not checked by the builder and was not opened here.
- day-plain-7: The headless JSON is written in the console code page (cp1252 here) when stdout is piped, so Spanish texts with accents are not UTF-8; a UTF-8 reader fails on it (read here by falling back to cp1252).
- day-plain-7: No interactive window: the only play mode is a terminal prompt (README says so).
- day-plain-8: Frame font is upper-case 5x7 without accents, so Spanish frames drop accents (texts in JSON keep them).
- day-plain-8: Visitor types on the day frame are told apart by colour only, with no legend.
- day-plain-9: Spanish frames drop accents ('Dia', 'Records'): the PNG font has no accented glyphs. The JSON texts are correct, and the README says so.
- day-plain-9: The Spanish Stall and Season over texts keep the English key name 'Enter' ('Enter: abrir', 'Enter: título').
- day-plain-9: The tkinter play window was never opened by the builder (README says so). Not opened here either, by this machine's rules.
