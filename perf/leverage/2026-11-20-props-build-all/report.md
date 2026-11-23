# Measured leverage: the runs

Folder: `lev-props3`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the kit runs: f77a13d1
- kit: builder claude-sonnet-5-5, cap 400,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| kit-guided-1 | 158,740 | 0.33 | 40,622 | 473,318 | 12,128 | 18 | 28 | 13 | done | 34 | 1,078 / 59,597 | 9 / 9 | 2 / 2 | 3 | 974 (738 at the start) |
| kit-guided-2 | 151,322 | 0.31 | 37,032 | 483,420 | 11,340 | 19 | 19 | 13 | done | 38 | 1,151 / 62,295 | 9 / 9 | 2 / 2 | 3 | 883 (738 at the start) |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| kit | guided | 2 of 2 | 155,031 | 151,322 to 158,740 | 13 | 9.0 / 9 | 17,226 |

## Against the baseline arm, `none` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| kit | guided | 155,031 | no judged run | - | - | 9.0 / 9 | - |

Judging: 2 judging(s) of 2 run(s), 191,714 weighted tokens in all.

At the runner's list prices: 0.64 USD for the builds, 0.80 USD for the judging shown above (1.44 USD in all).

## What each judge found beyond the list

- kit-guided-1: Every studio render reports a style error, STYLE.TOML: 1 ERRORS: NEUTRAL_SATURATION. The builder disclosed this and did not resolve it.
- kit-guided-1: The heart's bottom point ends in a small bulbous nub, slightly kinked where the two round cones meet, rather than a clean tapered point.
- kit-guided-1: The heart is 9820 triangles against 36 to 576 for the other props, so the four pickups vary widely in mesh density.
- kit-guided-2: By the README's own account, the studio's style check (neutral_saturation) still flags the shared palette as slightly over its chroma limit; this was left unresolved.
- kit-guided-2: The coin's raised rim is only 0.003 high on each face, so it barely shows in the 512 front frame (a faint ring line).
- kit-guided-2: The heart's bottom point is quite blunt and rounded, and it is 0.0496 thick rather than 0.05 (within tolerance).
