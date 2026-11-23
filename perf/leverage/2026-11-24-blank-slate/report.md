# Measured leverage: the runs

Folder: `lv2u`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the day runs: 5c416dc37b
- day: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell
- engine build of the night runs: 5c416dc37b
- night: builder claude-sonnet-5-5, cap 4,000,000 tokens and 240 min; judge claude-opus-5-5; 5 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| day-engine-1 | 1,156,305 | 2.03 | 152,606 | 5,146,528 | 90,184 | 50 | 54 | 16 | done | 31 | 5,353 / 17,976 | 55 / 61 | 2 / 2 | 3 | 3,495 (879 at the start) |
| night-engine-1 | 304,256 | 0.62 | 57,639 | 781,958 | 30,804 | 18 | 22 | 5 | done | 16 | 2,632 / 6,039 | 34 / 34 | 2 / 2 | 4 | 1,459 (879 at the start) |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| day | engine | 1 of 1 | 1,156,305 | 1,156,305 to 1,156,305 | 16 | 55.0 / 61 | 21,024 |
| night | engine | 1 of 1 | 304,256 | 304,256 to 304,256 | 5 | 34.0 / 34 | 8,949 |

## Against the baseline arm, `plain` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| day | engine | 1,156,305 | no judged run | - | - | 55.0 / 61 | - |
| night | engine | 304,256 | no judged run | - | - | 34.0 / 34 | - |

Judging: 2 judging(s) of 2 run(s), 1,216,020 weighted tokens in all.

At the runner's list prices: 2.64 USD for the builds, 4.34 USD for the judging shown above (6.98 USD in all).

## Items that failed

- day-engine-1 **d14**: Cheese with 2 coins: coins and stock unchanged, 'Not enough coins' shown, gone after the next word (put). The 99-unit cap could not be reached with the coins available.
- day-engine-1 **d35**: Seen: v1 at till from 31, ends 35, paid 20, coins/taken/served/rep up, sales entry; bargain pays tenth less in formula check. Not seen: units leaving the batch with fewest days_left (never had two batches of a good on sale).
- day-engine-1 **d37**: No queued wait ever reached patience in my runs, but no walk-out ever happened, so the leaving effects (goods back, rep -2, walked_out +1) were not seen.
- day-engine-1 **d39**: Second till (80 coins) never affordable in my runs; not checked.
- day-engine-1 **d47**: Cool box bought 50->10, upgrades [cool_box], row 'Cool box: owned', again no change; Second till with 50: Not enough coins, no change. Extra crates (crate_limit 12, 'of 12') never affordable; not checked.
- day-engine-1 **d60**: Only the Morning frame was looked at (texts shown, chosen row in bold); title, slots, settings, upgrades, paused, evening and season-over frames not viewed.

## What each judge found beyond the list

- day-engine-1: Enter on a Language/Difficulty/Season row of Settings also changes that setting (README admits it); the brief gives Enter no meaning there.
- day-engine-1: An input at the run's last tick is refused by the play command ('would never be given'), while the brief says a word at tick T acts after T ticks.
- day-engine-1: The setting values in the state show a Left/Right change on the tick of the word while menu_index and length follow a tick later (inconsistent timing inside one state).
- night-engine-1: A headless run of --ticks 0 prints every key as null (screen null, not title): the state is only written on the first tick.
- night-engine-1: While paused, speed reports the night's speed (e.g. fast) rather than a value tied to the pause; harmless but not stated in the README.
- night-engine-1: In the title and ledger frames, long lines ('Night Market', ledger lines) run past the right edge of the text panel.
- night-engine-1: The random stream is not reseeded per night, so a second night in the same run gets different arrivals than a first night with the same seed (documented in README).
