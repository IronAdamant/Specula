# Second check: J16-light-levels-and-sky.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its findings; its per-row table stayed with the checker.

### Round 3: J16-light-levels-and-sky.md

**57 rows: 54 VERIFIED, 3 FIX, 0 UNVERIFIABLE.**

FIX:
- Row 1.19 "Sun, measured 128,000 lx": in the physicallybased.info API (CC0-1.0, confirmed from the GitHub repo licence) 128,000 lx is the Sun's **"Space"** variant, i.e. above the atmosphere. The ground variant is **"Midday": 400 / 100,000 / 100,000 lx**, and "Sunset" is 3,000 lx at 2,600 K. Wikipedia "Lux" says the same thing: the unobscured Sun gives up to 100 klx at the surface, and 128,000 lx is the solar illuminance constant. Replace with 100,000 lx (ground, midday) and keep 128,000 only as top-of-atmosphere. It is also not purely an "own meter measurement": the entry's references include a meter measurement plus Wikipedia, Schlyter and the NASA Sun fact sheet.
- Row 3.1 (dynamic range): uses 128,000 lx as the ground maximum. With 100,000 lx it is 1.0 × 10⁹, 29.9 stops (the file's 1.3 × 10⁹, 30.3 stops is for top of atmosphere). Conclusion "about 30 stops" still holds.
- Row D.3 (direct sun disagreement): "128 klx measured (physicallybased.info)" is the Space value; the same database gives 100 klx at midday, so it agrees with "Lux", not against it. Drop it from the disagreement.

Missing disagreement / spreads:
- Sky-only shade: physicallybased.info "Sky / Clear" is **6,000 lx**, described as "Shade illuminated by entire clear blue sky at midday"; Wikipedia ("Orders of magnitude", "Daylight") gives **20,000 lx** for the same scene. The file lists both but does not flag the 3.3× disagreement.
- physicallybased.info variants left out: Sky "Overcast Day" 100 / 2,000 / 2,000 lx; Sky "Moonless Clear Night" 0.002 lx; Moon "Full" 0.32 lx and "Crescent" 0.05 lx.
- Kyba et al. 2017 (primary, opened: GFZ open copy) also says mid-latitude full Moon gives only 0.05-0.2 lx and that illuminance does not reach 0.32 lx except at perigee in the tropics; worth adding as the spread.
- Wikipedia "Lux" gives the magnitude-6 star as 8 nlx in one paragraph and 8.3 nlx in another.

Notes on secondary derivation (J16-2): several luminance rows on Wikipedia are editor-derived, not sourced measurements. "Photographic scene" rows (1.4 mcd/m², 25, 700, 5,000 cd/m²) are computed by Wikipedia from its illuminance table with an 18% grey card or from an exposure-value table; rows tagged with the page's "avg" note (250, 700, 1,000, 2,000, 3,000-5,000, 8,000, 10,000 sky, 130,000 cd/m²) are "a (rounded) geometric mean" of wider ranges. The sRGB 80 cd/m² row has no ref. Hahn 1996 (UPenn) is a dead link (archived). Treat these as order-of-magnitude only. The Daylight AM1.5 109,880 lx row is tagged "circular reference" on Wikipedia.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| 1.19 | Sun measured 128,000 lx | FIX | Space variant; Midday 100,000 | https://api.physicallybased.info/v2/lightsources |
| 3.1 | 1.3 × 10⁹, about 30 stops | FIX | ground: 1.0 × 10⁹, 29.9 stops | (computed) |
| D.3 | Direct sun incl. 128 klx measured | FIX | 128 klx is top-of-atmosphere | as 1.19 |
