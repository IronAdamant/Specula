# Measured leverage: the runs

Folder: `lev-props2`. Every run is here, failed ones included.

## Settings these runs used

- engine build of the kit runs: 76b6e777
- kit: builder claude-sonnet-5-5, cap 400,000 tokens and 60 min; judge claude-opus-5-5; 2 run(s) a cell

Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. "Weighted" is one figure for them: input x 1, cache_write x 1.25, cache_read x 0.1, output x 5 (settings.json's `token_weights`: relative prices, change them for yours).

## Runs

| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| kit-found-1 | 219,039 | 0.42 | 47,227 | 873,046 | 14,543 | 29 | 34 | 2 | done | 45 | 1,770 / 94,127 | 9 / 9 | 2 / 2 | 4 | 1,057 (736 at the start) |
| kit-found-2 | 360,234 | 0.65 | 73,327 | 1,810,922 | 17,500 | 34 | 46 | 3 | done | 66 | 2,401 / 160,525 | 9 / 9 | 2 / 2 | 5 | 995 (736 at the start) |
| kit-guided-1 | 173,926 | 0.34 | 36,868 | 585,071 | 13,869 | 22 | 28 | 2 | done | 40 | 1,109 / 76,927 | 9 / 9 | 2 / 2 | 4 | 980 (736 at the start) |
| kit-guided-2 | 158,904 | 0.32 | 37,775 | 510,601 | 12,127 | 20 | 28 | 2 | done | 39 | 1,264 / 72,021 | 9 / 9 | 2 / 2 | 4 | 984 (736 at the start) |

## By job and arm (the mean of each cell's runs that were judged)

| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |
|---|---|---|---|---|---|---|---|
| kit | found | 2 of 2 | 289,636 | 219,039 to 360,234 | 3 | 9.0 / 9 | 32,182 |
| kit | guided | 2 of 2 | 166,415 | 158,904 to 173,926 | 2 | 9.0 / 9 | 18,491 |

## Against the baseline arm, `none` (each cell's mean over the mean of the baseline's judged runs of the same job)

| Job | Arm | Weighted tokens | The baseline's | Times the baseline's | Requests, times the baseline's | Checks passed | The baseline's |
|---|---|---|---|---|---|---|---|
| kit | found | 289,637 | no judged run | - | - | 9.0 / 9 | - |
| kit | guided | 166,415 | no judged run | - | - | 9.0 / 9 | - |

Judging: 4 judging(s) of 4 run(s), 433,085 weighted tokens in all.

At the runner's list prices: 1.74 USD for the builds, 1.77 USD for the judging shown above (3.51 USD in all).

## What each judge found beyond the list

- kit-found-1: Because of the front-only camera, the coin's frame cannot show its raised rim (a 0.004 m step): it reads as a plain disc. The README admits this.
- kit-found-1: The heart's tip is a little blunt, and the heart has 8654 triangles where the other props have 36-576.
- kit-found-1: The frames command leaves a run-* folder of engine captures in work/captures every time it runs.
- kit-found-1: The README says the gear's tip radius is 0.15, but the recipe uses 0.15033. This is harmless, because the bounds still come out at 0.300.
- kit-found-2: Heart sides (seen from the side, judge-scratch/heart_side.png) have a ragged, jagged seam from the sculpt grid, and a faint centre crease shows in the front frame. It has 5588 triangles, which is heavy for a pickup.
- kit-found-2: style.json says every prop has 'a small soft edge so it catches light', but the gear, star and coin have hard, unbevelled edges. The heart alone is soft.
- kit-found-2: The gear renders dark grey in its front frame (steel metal in front light), so it reads duller than the other three.
- kit-found-2: The coin's rim is only 3 mm proud and cannot be seen in the README's front frame. Only a view at an angle shows it.
- kit-found-2: The frame and build commands hard-code the examen path and the run folder of this machine (EXAMEN/KIT_RUN can override them).
- kit-guided-1: The engine's own style check (studio_check / sheet caption) reports a neutral_saturation error for every prop. The builder knew about this and did not fix it.
- kit-guided-1: The heart has a visible vertical crease down its middle where the two round cones were joined. Its height is 0.274 m.
- kit-guided-1: The coin's raised rim stands only 0.005 m above each face, so it barely shows in the flat front frame (just a thin ring line).
- kit-guided-1: The gear and star bottoms sit a little above y = 0 (by up to 2e-5 m) because the outline is sampled.
- kit-guided-2: The engine's own style check fails heart, star and coin (neutral_saturation: 100% of each surface is a saturated colour). Only the gear passes. The README says this is deliberate.
- kit-guided-2: In the colour front frame the coin's rim is only a faint line. It is clearly visible only in clay passes or at an angle.
- kit-guided-2: The heart has a visible vertical crease down its middle where the two sculpted lobes are smoothly unioned.
- kit-guided-2: The star is 0.285 tall against 0.3 wide. The builder read '0.3 across' as its width.
