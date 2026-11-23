# Measured leverage: the runs

Folder: `lv2r`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the day runs: 1dc2f432d0
- day: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| day-engine-1 | 710,147 | 1.34 | 110,620 | 2,468,678 | 65,004 | 31 | 34 | 18 | done | 30 | 3,896 / 22,797 | 46 / 61 | 2 / 2 | 4 | 2,866 (877 at the start) |
| day-engine-10 | 814,622 | 1.50 | 117,889 | 3,051,894 | 72,418 | 38 | 44 | 12 | done | 28 | 5,527 / 9,382 | 59 / 61 | 2 / 2 | 4 | 2,044 (877 at the start) |
| day-engine-2 | 736,486 | 1.43 | 122,097 | 2,246,643 | 71,843 | 30 | 33 | 11 | done | 15 | 2,262 / 18,333 | 48 / 61 | 2 / 2 | 3 | 2,717 (877 at the start) |
| day-engine-3 | 795,198 | 1.49 | 123,018 | 2,833,481 | 71,619 | 35 | 41 | 13 | done | 26 | 3,796 / 22,731 | 55 / 61 | 2 / 2 | 5 | 2,037 (877 at the start) |
| day-engine-4 | 1,348,585 | 2.34 | 163,715 | 5,991,057 | 108,973 | 58 | 68 | 24 | done | 31 | 4,919 / 12,170 | 49 / 61 | 2 / 2 | 5 | 2,574 (877 at the start) |
| day-engine-5 | 989,923 | 1.73 | 130,296 | 4,495,875 | 75,498 | 49 | 53 | 13 | done | 28 | 9,197 / 26,628 | 60 / 61 | 2 / 2 | 6 | 2,778 (877 at the start) |
| day-engine-6 | 1,739,856 | 2.80 | 179,316 | 9,480,056 | 113,549 | 79 | 94 | 20 | done | 46 | 9,749 / 22,986 | 58 / 61 | 2 / 2 | 4 | 2,770 (877 at the start) |
| day-engine-7 | 823,147 | 1.52 | 116,195 | 3,013,308 | 75,318 | 35 | 41 | 14 | done | 42 | 5,804 / 13,494 | 59 / 61 | 2 / 2 | 4 | 2,829 (877 at the start) |
| day-engine-8 | 997,271 | - | 132,164 | 4,078,431 | 84,849 | - | 45 | 15 | done | 35 | 4,972 / 16,701 | 59 / 61 | 2 / 2 | 3 | 2,526 (877 at the start) |
| day-engine-9 | 916,858 | 1.66 | 128,220 | 3,641,821 | 78,484 | 39 | 46 | 13 | done | 29 | 5,820 / 25,254 | 49 / 61 | 2 / 2 | 3 | 2,607 (877 at the start) |
| day-plain-1 | 278,043 | 0.59 | 48,765 | 380,373 | 35,811 | 11 | 12 | 4 | done | - | - | 53 / 61 | 2 / 2 | 4 | 1,293 |
| day-plain-10 | 279,321 | 0.59 | 47,809 | 405,403 | 35,805 | 11 | 14 | 4 | done | - | - | 61 / 61 | 2 / 2 | 4 | 1,115 |
| day-plain-2 | 370,369 | 0.75 | 58,242 | 765,498 | 44,205 | 17 | 21 | 5 | done | - | - | 61 / 61 | 2 / 2 | 3 | 1,441 |
| day-plain-3 | 334,964 | 0.70 | 56,300 | 555,358 | 41,812 | 14 | 22 | 5 | done | - | - | 59 / 61 | 2 / 2 | 3 | 1,410 |
| day-plain-4 | 356,982 | 0.74 | 57,802 | 653,317 | 43,881 | 15 | 27 | 5 | done | - | - | 61 / 61 | 2 / 2 | 5 | 1,369 |
| day-plain-5 | 332,711 | 0.70 | 55,109 | 528,761 | 42,191 | 13 | 21 | 5 | done | - | - | 61 / 61 | 2 / 2 | 4 | 1,269 |
| day-plain-6 | 322,501 | 0.67 | 54,396 | 517,426 | 40,554 | 13 | 23 | 4 | done | - | - | 61 / 61 | 2 / 2 | 3 | 1,257 |
| day-plain-7 | 336,561 | 0.70 | 55,809 | 578,871 | 41,784 | 14 | 24 | 5 | done | - | - | 60 / 61 | 2 / 2 | 3 | 1,221 |
| day-plain-8 | 350,787 | 0.74 | 58,315 | 502,490 | 45,530 | 12 | 23 | 5 | done | - | - | 61 / 61 | 2 / 2 | 3 | 1,387 |
| day-plain-9 | 317,955 | 0.65 | 51,940 | 643,882 | 37,730 | 16 | 19 | 4 | done | - | - | 60 / 61 | 2 / 2 | 5 | 1,327 |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| day | engine | 10 of 10 | 987,209 | 710,147 to 1,739,856 | 15 | 54.2 / 61 | 18,214 |
| day | plain | 10 of 10 | 328,019 | 278,043 to 370,369 | 5 | 59.8 / 61 | 5,485 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| day | engine | 987,209 | 328,019 | 3.01 | - | 54.2 / 61 | 59.8 / 61 |

Judging: 22 judging(s) of 20 run(s), 16,641,629 weighted tokens in all (2 of them earlier judgings of runs judged again).

At the runner's list prices: 22.64 USD for the builds, 54.02 USD for the judging shown above, and -0.00 USD for the earlier judgings of the runs judged again (76.65 USD in all).

## Judged more than once

The figures above are the last judging. Before it:

- day-engine-2: no verdict. Judged again because: the first judge reached its 30 minutes with no verdict
- day-engine-4: no verdict. Judged again because: the first judge reached its 30 minutes with no verdict

## Items that failed

- day-engine-1 **d03**: Title texts/menu_index 0/Up stop at 0/Down stop at 4 OK. Enter on Quit at tick 5 with --ticks 12: game stops answering (up ignored) but the run goes on and prints tick 12, not the state where it ended; with Continue, Quit at 7 printed tick 20.
- day-engine-1 **d14**: Not enough coins shows, changes nothing, gone after next word (seen). The 99-unit cap was not exercised.
- day-engine-1 **d20**: Single-cell moves and Up raising y seen; stopping at x 0/5 and y 0/1 was not exercised.
- day-engine-1 **d22**: put placed crates and Crates N of 8 followed; 9th put at 8 crates refused; take removed crates. Put on an occupied cell and take on an empty cell not exercised.
- day-engine-1 **d23**: price_up to 5 seen working; the bounds 1 and 3x reference were not exercised.
- day-engine-1 **d32**: Cook took 2 apples at 4 at its column, budget -8; at price 5 only tourists bought. Prices 6 and 7 not run (my runs failed while the engine was busy).
- day-engine-1 **d37**: No queued wait reached patience in ~1900 checked minutes, but no walk-out ever happened in my runs, so its effects were not seen.
- day-engine-1 **d42**: Bread bought that morning spoiled 6 that evening; honey never lost days. Fish over two days not exercised.
- day-engine-1 **d43**: Heat day without Cool box: apples 5->3. The Cool box case on a heat day was not exercised.
- day-engine-1 **d44**: Normal 20+2*3=26, Easy 15+2*7=29 and 15+2*8=31 seen. Hard rent not reached in my runs.
- day-engine-1 **d47**: Buying Sign 133->103 'Sign: owned'; rebuying Second till changed nothing; Extra crates -> crate_limit 12, 'Crates: 5 of 12'. 'Not enough coins' on Upgrades not exercised.
- day-engine-1 **d50**: Records 'Score 65, 3 days, Easy', 'Score 0, 2 days, Easy, bankrupt' in order; 'No records yet' fresh. Five-record cap, equal-score order and display in a later run not exercised.
- day-engine-1 **d53**: Sold out earned when crated apples/flowers sold out (seed 1). The negative case (good without a crate) was not set up.
- day-engine-1 **d59**: day.png: grid and text panel, one unlabelled red square below the grid; no visible aisle cells, queue or tills. Front row drawn farthest from the aisle.
- day-engine-1 **d60**: title.png shows texts with '>' on chosen entry; stall/day panels show texts. Slots, settings, morning, upgrades, paused, evening and season-over frames were not viewed.
- day-engine-10 **d58**: Stall frame shows grid, cursor at its cell and the text panel, but bread (133,133,77) and cheese (133,133,63) crates are drawn in nearly the same colour: the six goods are not told apart.
- day-engine-10 **d60**: Title, slots, settings, morning, upgrades, paused, season over frames show texts with '>' on the chosen row, but the Evening frame (two runs) shows only the raw '{work.display}' and the engine warns nothing keeps work.display.
- day-engine-2 **d03**: Texts, menu_index 0 and Up/Down stops are right, but Quit (enter on index 4 at tick 5) does not end the run: --ticks 20 printed tick 20 with screen title, not the state as it stood at the quit (README admits the run goes on counting ticks).
- day-engine-2 **d14**: With 14 coins Enter on Fish (36) changed nothing and showed 'Not enough coins' until the next word (accept). The 99-unit cap could not be reached in a run.
- day-engine-2 **d17**: Continue in a later headless run gave day 2 wholesale 3/4/5/7/4/5 and order 4 Cheese for 66, but the season was saved with 2/2/7/6/3/5 and 6 Flowers for 63 (same within one run). Cause: the saved season seed is a 64-bit number kept as a JSON float (-7135811184969857000), so precision is lost.
- day-engine-2 **d32**: Cooks took 2 apples at 4 at their column's minute with budget 8 lower; the price-5 cook and the 6/7 tourist cases were not run.
- day-engine-2 **d35**: Ends = start + 2 + goods, paid totals with bargain tenth off, served/taken/sales all matched; that units leave the batch with the fewest days_left first was not observed (all units sold).
- day-engine-2 **d37**: No queued wait ever reached patience, but no visitor walked out in any run (queue at most 1), so the walk-out effects were not seen.
- day-engine-2 **d40**: Sign was bought on one run but no back-row crate had stock on that day, so the effect was not seen.
- day-engine-2 **d42**: Bread bought that Morning spoiled that evening (spoiled 6, line 'spoiled 6'); honey never spoiled; fish spoiling on the next day's evening was not observed (only seen on a heat day).
- day-engine-2 **d44**: Rent: 28 at Normal with 4 crates, Rent: 31 at Easy with 8 crates; Hard (30) not observed.
- day-engine-2 **d50**: Records showed 'Score 261, 3 days, Easy' and 'Score 0, 2 days, Easy, bankrupt'; the five-cap, ordering among several and display in a later run were not checked.
- day-engine-2 **d53**: Sold out earned when crated apples/bread/flowers sold out; the negative case (good without crate) was not isolated.
- day-engine-2 **d54**: After Continue in a later run the day's draws differ from the unbroken game (wholesale and order differed, see d17), so the day cannot be the same.
- day-engine-2 **d60**: Frames on the Evening (both the README's own command and mine) show only the placeholder '{work.view_a}' and the engine warns 'nothing keeps work.view_a'; the evening's texts are not drawn.
- day-engine-3 **d03**: Fresh title texts are right (no Continue), menu_index 0, Up at 0 and Down at 4 stop. But Quit (down x4, enter at tick 4) does not end the run: with --ticks 50 the state prints tick 50, and --at 5,6,20 shows the run going on. Words stop acting, but the run keeps ticking.
- day-engine-3 **d14**: With 2 coins, Enter on Bread changed nothing and added 'Not enough coins', which went away at the next word. I could not reach the 99-unit cap: the most I could buy was 48 apples with 151 coins.
- day-engine-3 **d37**: No queued visitor ever waited at or over its patience across ~2400 simulated minutes. But the queue never passed 2, walked_out stayed 0 in every run, and the walk-out path (goods back, rep -2, walked_out +1) was never seen.
- day-engine-3 **d48**: In-run: coins -6 -> 'Bankrupt', slots [null,null,null], Season over 'Bankrupt','Score: 0', record bankrupt true. But the kept slot was never cleared: settings.json still held slot1, and the next run showed Continue and 'Slot 1: Day 2 of 7, 9 coins'.
- day-engine-3 **d49**: In-run, the 3-day Easy season was right: 'Season over', 'Score: 486' (386+100), 'Days: 3', 'Best: 486', and no Continue. A later run on the same place showed Continue again, with 'Slot 1: Day 3 of 3, 201 coins' still in the kept file.
- day-engine-3 **d60**: Title, slots, settings, morning, upgrades, paused and season_over frames show their texts with '> ' on the chosen row. The evening frame (three different runs, by finish and by rush) shows only the literal '{work.disp}' and none of the Evening texts.
- day-engine-4 **d14**: Not enough coins: cheese at 6 with 2 coins changed neither coins nor stock, showed 'Not enough coins', and the line went away on the next word. The 99-unit cap was not exercised.
- day-engine-4 **d20**: Up lowers y: from (0,0) Up stayed at y 0 and Down went to y 1, and from (1,1) Up went to (1,0). The brief says Up raises y. Left/Right stepped and stopped at 0 and 5 correctly.
- day-engine-4 **d22**: put/take on empty and occupied cells behaved right and Crates: N of 8 followed, but a put with 8 crates already placed was never tried (could not check).
- day-engine-4 **d23**: Honey price went 9->11->1 and stopped at 1, with the Price text following. The upper stop (3x reference) was not exercised.
- day-engine-4 **d32**: The specific cook/tourist price setups (4/5 and 6/7) were not built; only the general percentage rule was checked.
- day-engine-4 **d37**: No queued wait reached patience, but no walk-out happened in any run (max queue 1), so its effects are unseen.
- day-engine-4 **d38**: Evening came at clock 480, but no visitor was left on the aisle, in the queue or at a till at the end of any run.
- day-engine-4 **d39**: With Second till, tills had 2 entries, but two services never ran at once in the run.
- day-engine-4 **d43**: With Cool box, fish on a heat day went 2->1. The heat case without Cool box was not observed.
- day-engine-4 **d50**: Records showed 'Score 65, 3 days, Easy' and 'Score 0, 2 days, Easy, bankrupt' in a later run, and 'No records yet' when empty. The cap of 5 and the tie order were not exercised.
- day-engine-4 **d59**: Day frame showed crates, a till box and the time/speed/coins/taken/served/rep texts, but no visitor was on the aisle at that moment, so type drawing is unseen. The front row is drawn away from the aisle.
- day-engine-4 **d60**: Frames of title/slots/settings/morning/upgrades/paused/evening/season over were not taken (judge budget).
- day-engine-5 **d60**: Title, slots, settings, morning, upgrades, paused, season over frames show texts with '>' on chosen entry; but both Evening frames show only '{work.disp0}' in the main panel plus the last few lines (Reputation, Tomorrow, Stamp): Evening, day, Taken..Coins are not drawn (engine also warned 'nothing keeps work.disp0').
- day-engine-6 **d14**: Too few coins: honey lot with 20 coins left coins and stock unchanged, 'Not enough coins' shown and gone after next word (up). The 99-unit cap could not be reached in my runs (stock_cap 99 is in rules.json, not seen acting).
- day-engine-6 **d37**: No queue ever formed in any day I played (6 days, 30 seeds scanned walked_out 0), so patience, goods returned, rep -2 and walked_out +1 were never seen.
- day-engine-6 **d39**: With Second till tills has two entries in the day, but no queue ever formed so two services at once / till 1 first were never seen.
- day-engine-7 **d37**: Over 8 played days (busy stalls, rep up to 100) the queue never exceeded 1 and no visitor walked out, so no queued wait reached patience and the walk-out effects (goods back, reputation -2, walked_out +1) could not be seen. Code at lib.rs:824 looks right but the item needs observed behaviour.
- day-engine-7 **d60**: Title, slots, settings, morning, upgrades, paused and season over frames show their texts with '>' on the chosen row, but the Evening frame shows only '{work.tx0}' and the two 'Stamp:' lines (no Evening, Taken, goods, Rent, Coins...); the run also warns that nothing keeps work.tx0.
- day-engine-8 **d14**: Not enough coins: coins 2, Enter on fish changed nothing and showed 'Not enough coins' until the next word (up). The 99-unit cap was never reached in play (needs over 96 units of one good), so not seen.
- day-engine-8 **d37**: No queued wait ever reached patience in ~2400 checked minutes, but no visitor ever walked out in any run (queue at most 2), so the leaving rule (goods back, rep -2, walked_out +1) was never seen.
- day-engine-9 **d03**: Title texts/menu_index/Up-Down limits correct, but Quit at tick 5 of a 30-tick run printed tick 30 (the run went on; later inputs ignored), so the state is not printed as it stood when Quit ended the run (README admits this).
- day-engine-9 **d14**: Not enough coins: coins/stock unchanged and the line shown until the next word (seen). The 99-unit cap could not be reached in a run I could set up.
- day-engine-9 **d37**: No queued visitor waited to patience in matched days, but no walk-out ever happened in any run I made (12 seeds tried), so the leaving path was not seen.
- day-engine-9 **d39**: Not checked: never had 80 coins at an Upgrades screen in my runs.
- day-engine-9 **d41**: Failed order seen (rep -5, 'Order failed'); order not taken changed nothing; a delivered order was not reached.
- day-engine-9 **d42**: Bread bought that morning spoiled that evening (spoiled 6, line shown); a 2-day-life good (flowers) had days_left 1 after first evening; fish and honey not checked directly.
- day-engine-9 **d43**: Not checked: no heat day reached with stock.
- day-engine-9 **d47**: Sign bought: coins 55->25, upgrades [sign], 'Sign: owned', second Enter no change. Not-enough-coins text on Upgrades and Extra crates (crate_limit 12) not captured.
- day-engine-9 **d50**: Record lines 'Score 0, 2 days, Easy, bankrupt' shown in a later run; cap of 5 and ordering/tie-break not checked (one record per place).
- day-engine-9 **d53**: Sold out earned when crated apples sold out; the negative case (good with stock but no crate) not cleanly tested.
- day-engine-9 **d59**: Day frame viewed shows stall rows, aisle, queue, till, time, speed, coins, taken, served, reputation as text, but no visitor was on the aisle in my frame, so type distinction not seen.
- day-engine-9 **d60**: Frames of title, slots, settings, morning, upgrades, paused, evening, season over not taken; chosen-entry marking not seen.
- day-plain-1 **d06**: Spanish texts checked on title, both slots headings, settings (changes at once), records, stamps, morning, stall, day, paused, evening, season over: all Spanish. But the Upgrades screen can never be reached (Enter on Evening goes straight to the next Morning), so its Spanish could not be shown on that screen.
- day-plain-1 **d18**: Paused texts [Paused, Resume, Title]; Resume/Escape restore morning (menu kept), stall cursor kept, day clock stood (10 at pause, 12 after 200 paused ticks + 10). Across runs Title leaves the slot as saved. But in the same run, buy apples -> Escape -> Title -> Continue -> slot 1 opens with coins 50 and the 6 apples bought that Morning still in stock: the saved slot shares the live season's stock.
- day-plain-1 **d39**: Second till cannot be bought: Upgrades screen is unreachable. With a hand-written kept slot holding second_till, tills had 2 entries and 2 served at once, but not through the game.
- day-plain-1 **d40**: Sign cannot be bought (Upgrades unreachable); not observable in play.
- day-plain-1 **d43**: Heat day without Cool box: apples days_left 5->3 (correct). Cool box cannot be bought (Upgrades unreachable); only a hand-written slot with cool_box showed 5->4.
- day-plain-1 **d46**: Enter on the Evening of a continuing season goes straight to 'Morning, Day 2 of 7'; the Upgrades screen never opens (game.py on_evening calls start_morning).
- day-plain-1 **d47**: No upgrade can be bought: Upgrades screen unreachable.
- day-plain-1 **d60**: Settings, slots, morning, paused, evening, season over frames show their texts with the chosen row in yellow; the Upgrades screen cannot be reached, so its frame cannot be made.
- day-plain-3 **d26**: Lists are well formed (ids 1.., minutes increasing in 0..479, wants distinct and 2/1/2/3) and same seed repeats, but another seed does not give another list: day-1 visitor lists of seeds 5, 6 and 9 are identical, so are 3 and 4, and 7 and 8 (seeds shift the same splitmix stream).
- day-plain-3 **d60**: Title, slots, settings, morning, upgrades, paused, season over frames show texts with the chosen row highlighted, but the Evening frame with 17 texts (order delivered, no stamp) clips the last line 'Tomorrow: Sun' off the bottom of the 480px image (render.py only tightens spacing above 17 lines).
- day-plain-7 **d60**: Title, settings, morning, upgrades, paused and season-over frames show their texts with the chosen row marked, but render.py cuts every line at 30 characters, so the Evening frame shows 'APPLES: SOLD 12, SPOILED 0, LE' and the 'left N' part of every good line is missing.
- day-plain-9 **d60**: Title, slots, settings, morning, upgrades frames mark the chosen row with '>' and a bar; evening and season over show their texts. But Paused from the Stall (or Day) shows Paused/Resume/Title with no row marked, even with menu_index 1 (frame.py draws paused-over-stall without the menu marker).

## What each judge found beyond the list

- day-engine-1: Quit in a headless run does not end it: the game stops reacting but ticks keep counting to --ticks.
- day-engine-1: Records text 'Score 0, 1 days, Normal, bankrupt' (plural for one day).
- day-engine-1: Frame draws the stall front row (y 0) farther from the aisle/till area than the back row.
- day-engine-1: Day frame does not draw the aisle, queue or tills legibly; only an unlabelled square.
- day-engine-10: Several words at the same tick: only one acts ('1:down,1:down' gives menu_index 1, not 2), against the brief's headless spec (README admits it).
- day-engine-10: The Evening screen is drawn as the literal placeholder '{work.display}' in frames; the engine prints a warning to that effect on runs that end on the Evening.
- day-engine-10: Bread and cheese crates are almost the same colour in frames.
- day-engine-10: A save whose batches are not sorted is used in stored order (remove does not sort); not reachable through play, only by a hand-edited save.
- day-engine-2: The season seed is saved as a JSON float and loses precision, so a season continued in a later run redraws its saved day differently.
- day-engine-2: Quit does not end a headless run; the printed tick is the full --ticks count.
- day-engine-2: Evening frames render '{work.view_a}' instead of the screen.
- day-engine-3: Several words at the same tick act only once (0:down,0:down,0:down gave menu_index 1); the brief requires each to act in order. README admits this.
- day-engine-3: A season that ends (bankrupt or finished) is cleared from the slot in memory, but the kept slot keeps the last Morning save. A later run offers Continue and replays the ended season.
- day-engine-3: The Evening frame renders the placeholder '{work.disp}' instead of the evening text.
- day-engine-3: Quit freezes input but does not end the headless run; tick keeps counting to --ticks.
- day-engine-3: A 'none' word does not clear 'Not enough coins' unless it releases a held word (README admits).
- day-engine-4: Stall: Up moves the cursor to lower y and Down to higher y, the reverse of the brief.
- day-engine-4: Stall/Day frame: the front row (y 0) is drawn on top, away from the aisle strip, which is next to the back row.
- day-engine-4: After Quit the printed tick is the script's full tick count, not the tick at which the run ended.
- day-engine-4: Enter on Language/Difficulty/Season in Settings steps the setting (README admits it).
- day-engine-4: Words given at the same tick: all but the last are lost (README admits it; brief requires them in order).
- day-engine-5: Several input words at the same tick: only one acts (1:down,1:down,1:down gives menu_index 1); the brief requires each to act in order. README admits it.
- day-engine-5: Quit at tick 9 reports tick 10 (one extra tick counted/run before the run ends).
- day-engine-5: State printed at tick 0 shows default settings/slots before the kept place is loaded (es/easy place reads en/normal at tick 0, es/easy from tick 1).
- day-engine-5: Day frame draws the aisle below the back row of the stall, i.e. beside the back row rather than in front of the front row.
- day-engine-5: Evening frame: main text panel shows the raw placeholder '{work.disp0}'.
- day-engine-5: game.txt comment says names are in assets/strings/<language>.json; the tables actually used are modules/work/strings/*.json (README is right).
- day-engine-6: Frame: the front row (y 0) is drawn at the top, farthest from the aisle strip, while the text grid in the same frame prints the back row first; the aisle is drawn next to the back row.
- day-engine-6: Bread and cheese crate colours are very close in the frames.
- day-engine-6: Records read 'Score 0, 1 days, ...' (plural with 1).
- day-engine-6: Quit freezes the run but the printed tick stops at the Quit tick + 1 rather than the requested tick count (as README admits).
- day-engine-7: The Evening's frame is broken: it draws '{work.tx0}' and the stamp lines only, and every run that reaches the Evening prints the engine warning 'a screen shows {work.tx0}: nothing keeps work.tx0'.
- day-engine-7: Kept settings and slots load at tick 1: a 0-tick headless run on a place with Spanish/Hard/3 days kept prints en/normal/7 and the default title.
- day-engine-7: Several words at one tick ('T:a+b') act in a fixed order rather than the order given, and a repeated word acts once ('0:down+down+down+down+enter' opened Records, not Settings); the brief asks for words at the same tick to act one after another in the order given.
- day-engine-7: Frames are a small text panel in the top-left corner (crates as two-letter codes, visitors as letter+id); nothing is drawn as pictures.
- day-engine-8: Records line says '1 days' for a one-day season.
- day-engine-8: Each headless run prints on stderr: 'a screen shows {work.shown}: nothing keeps work.shown' (the first frame before the state is set).
- day-engine-8: Quit given at tick 8 ends the run with tick 9 printed, one tick after the word.
- day-engine-9: After Quit the run keeps counting ticks: report tick is the full --ticks, not the tick Quit was given.
- day-engine-9: Spanish plural not handled: 'Puntos 0, 1 días'.
- day-engine-9: The frame is a small text panel in the top-left corner; no drawn grid or visitors.
- day-plain-1: Enter on the Evening skips the Upgrades screen entirely; no upgrade can ever be bought in play.
- day-plain-1: In one run, a slot's saved season shares the live stock/prices/stall objects, so giving the day up and continuing in the same run keeps purchases made after the Morning save (coins revert, stock does not).
- day-plain-1: Headless stdout is written in the console code page (cp1252 here), not UTF-8, when Spanish accented text is printed through a pipe.
- day-plain-1: In every frame the 'AISLE' label overlaps the bottom of the stall grid.
- day-plain-10: Headless stdout is written in the console locale encoding (cp1252 on this Windows machine), not UTF-8, so Spanish texts come out as cp1252 bytes when piped.
- day-plain-10: Frame labels STALL, AISLE, QUEUE, TILLS, FREE, END are hard-coded English and stay English when the language is Spanish.
- day-plain-10: Frame font draws Spanish accented letters without their marks (stated in README).
- day-plain-10: Spanish uses Pausa both for the Paused screen and for the Hold speed.
- day-plain-2: The headless run prints its JSON with ensure_ascii=False in the console code page (cp1252 here), not UTF-8: a reader decoding UTF-8 fails on Spanish texts (I had to decode as cp1252), and a character outside the code page would crash the print.
- day-plain-2: The frame font is ASCII only: Spanish accents are dropped in the PNG (README says so).
- day-plain-2: The window mode (play) was never run by the builder and was not run here (it opens a window).
- day-plain-3: The seed enters the splitmix64 state linearly (seed*GOLDEN + ...), so nearby seeds give the same random stream shifted by a step: different seeds often produce identical visitor lists (5, 6, 9; 3, 4; 7, 8).
- day-plain-3: Evening frame with exactly 17 lines (e.g. an order delivered, no stamp) draws the last line 'Tomorrow: ...' half outside the 480px image.
- day-plain-3: Frame font has no accents: Spanish text is drawn without accents in the PNG (README admits it).
- day-plain-4: The headless run prints the JSON with ensure_ascii=False through the console code page, so on this machine Spanish texts (Español, Día, Mañana) come out cp1252, not UTF-8, when stdout is piped; a UTF-8 reader fails to decode it.
- day-plain-4: The frame font has capitals only, so every text is drawn upper case (MORNING, APPLES) rather than as the strings read.
- day-plain-4: Spanish uses Mañana both for the Morning heading and the Tomorrow: line.
- day-plain-4: Adding a seventh good needs code changes (the draws and next_good use 6), as the README admits; upgrade and stamp effects are code, only their numbers are data.
- day-plain-4: Window mode (--play, tkinter) was not run by the builder or by me.
- day-plain-5: Day frame: crate labels in the stall cells overlap their price digits (e.g. 'Ho' over '9'), still readable.
- day-plain-5: Spanish frames draw accented letters without accents (README admits it); the JSON texts carry the accents.
- day-plain-5: Spanish 'Enter: abrir' / 'Enter: titulo' keep the key name 'Enter' (a key label, not left-over English text).
- day-plain-5: The play window (tkinter) was not run by the builder and not by me.
- day-plain-6: Spanish strings drop every accent and the n tilde (Manana, Dia, Espanol, Reputacion, Titulo); the Records heading and menu entry keep the English spelling 'Records' instead of a Spanish form such as 'Récords' or 'Registros'.
- day-plain-6: A visitor whose basket was empty takes from the front crate and then, in the same minute, also from the back crate of that column; the brief's 'if its basket already holds something' could also be read as 'before looking', which would forbid that.
- day-plain-6: Per the README a kept file that cannot be read is silently treated as the default (e.g. a corrupt records.json loses the records without warning).
- day-plain-7: The headless run prints its JSON in the console code page (cp1252 here), not UTF-8, when piped. With Spanish texts the output is not valid UTF-8 (byte 0xf1 for n-tilde), so a UTF-8 reader fails until it decodes as cp1252.
- day-plain-7: Frame text is limited to 30 characters per line, and accents are dropped (the README says so). Longer lines such as the Evening good lines and the Spanish evening lines are cut off in the PNG.
- day-plain-7: With no --store, the kept files go to work/save, inside the game folder.
- day-plain-8: Stall texts draw the grid with the first letter of each good's name: Fish and Flowers are both F in English; in Spanish Manzanas/Miel are both M and Pan/Pescado both P.
- day-plain-8: Frame labels AISLE, QUEUE, TILLS, FRONT ROW IS BOTTOM and 'til N' are hard-coded English and are not translated in Spanish.
- day-plain-8: The frame font has no '#' glyph, so tills show '?19' for visitor 19; accents are dropped in frames (README says so).
- day-plain-9: The state JSON is printed with ensure_ascii=False in the console's code page (cp1252 here), not UTF-8, so Spanish texts are not valid UTF-8 for a reader expecting it.
- day-plain-9: README says the default store is ./save, but the code defaults to a save folder beside daymarket.py (inside the game folder).
- day-plain-9: Paused over the Stall or Day: the frame does not mark Resume/Title as chosen.
- day-plain-9: Visitor labels in the day frame show only the last digit of the id (26 drawn as 6).
- day-plain-9: Spanish keeps the key name 'Enter' in 'Enter: abrir' and 'Enter: título'.
