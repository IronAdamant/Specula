# J16: Light levels: illuminance of scenes, luminance of sky, moon, stars and lamps

Fixed after the second check (2026-11-18):
- J16-1: physicallybased.info's 128,000 lx is the Sun above the atmosphere (its "Space" variant), not a measurement on the ground; the ground value is its "Midday" variant, 100,000 lx. Both are now in the table, with the Sunset variant, and the entry's references named.
- J16-3: the dynamic range on the ground is 1.0 × 10⁹, 29.9 stops (1.3 × 10⁹ and 30.3 stops were for the top of the atmosphere); still about 30 stops.
- Sources disagree: the 128 klx dropped from the direct-sun disagreement (it agrees with "Lux"); added the clear-sky shade (6,000 against 20,000 lx), the full Moon's spread from Kyba et al. 2017 (0.05-0.2 lx), and the magnitude-6 star (8 against 8.3 nlx).
- Added: physicallybased.info's other Sky and Moon variants; that many J16-2 luminance rows are Wikipedia editors' derivations (order of magnitude only); that the AM1.5 row is tagged "circular reference".
- A "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic. Most values are from Wikipedia list pages that cite a primary; the primary is named and the entry is marked secondary. Use these as exposure targets for a physically based renderer (lux on surfaces, cd/m² for what the camera sees).

## J16-1. Illuminance on the ground, night to noon (lux)

| Scene | Illuminance | Source |
|---|---|---|
| Overcast moonless night (starlight) | 0.0001 lx (100 µlx) | Wikipedia "Orders of magnitude (illuminance)", https://en.wikipedia.org/wiki/Orders_of_magnitude_(illuminance), and "Lux", https://en.wikipedia.org/wiki/Lux, both citing Schlyter, *Radiometry and photometry in astronomy* FAQ, http://stjarnhimlen.se/comp/radfaq.html (secondary; the FAQ itself refused the box) |
| Venus at its brightest | 140 µlx | Orders of magnitude (illuminance) |
| Clear moonless sky, starlight without airglow | 200 µlx | same, citing Schlyter |
| Clear moonless sky with airglow | 0.002 lx | same, citing Schlyter |
| Quarter Moon, clear night | 0.01 lx | "Lux" and "Orders of magnitude" |
| Full Moon, clear night | 0.05-0.3 lx ("Lux"); 0.25 lx (Orders of magnitude, citing Schlyter) | Lux cites Kyba, Mohar and Posch 2017, "How bright is moonlight?", *Astronomy & Geophysics* 58(1) 1.31-1.32, doi:10.1093/astrogeo/atx025; Wikipedia "Moonlight" gives the typical full Moon as 0.05-0.1 lx from the same paper; Kyba et al. 2017 itself (opened by the second checker, GFZ open copy) says a mid-latitude full Moon gives only 0.05-0.2 lx, and that illuminance does not reach 0.32 lx except at perigee in the tropics |
| Full Moon at perigee, overhead in the tropics | up to 0.32 lx | Wikipedia "Moonlight", https://en.wikipedia.org/wiki/Moonlight, citing Kyba et al. 2017 |
| Dark limit of civil twilight, clear sky | 3.4 lx | "Lux" (secondary) |
| Darkest storm clouds at sunset or sunrise | under 1 lx | Orders of magnitude (illuminance) |
| Fully overcast, sunset or sunrise | 40 lx | same |
| Very dark overcast day | 100 lx | "Lux" |
| Darkest storm clouds, midday | under 200 lx | Orders of magnitude; Wikipedia "Daylight" table |
| Clear sunrise or sunset (ambient) | 400 lx | "Lux"; "Daylight" |
| Overcast day | 1,000 lx ("Lux"); 1,000-2,000 lx midday ("Daylight"); 10-25 klx midday (Orders of magnitude) | see Sources disagree |
| Shade lit by the whole clear blue sky, midday | 20,000 lx | Orders of magnitude; "Daylight" |
| Full daylight, not direct sun | 10,000-25,000 lx | "Lux" |
| Direct sunlight | 32,000-100,000 lx ("Lux"); 111,000 lx "bright sunlight", 120,000 lx "brightest sunlight" ("Daylight", https://en.wikipedia.org/wiki/Daylight) | secondary |
| AM1.5 global solar spectrum (1,000.4 W/m²) | 109,880 lx | "Daylight" (the row is tagged "circular reference" on Wikipedia) |
| Sun on the ground, midday | 100,000 lx (the database's "Midday" variant: range 400 to 100,000, default 100,000) | physicallybased.info API v2 light sources, CC0 1.0, https://api.physicallybased.info/v2/lightsources; the Sun entry's references include an illuminance-meter measurement, Wikipedia, Schlyter and the NASA Sun fact sheet |
| Sun above the atmosphere (top of atmosphere, not on the ground) | 128,000 lx (the database's "Space" variant; the solar illuminance constant) | same; Wikipedia "Lux" also gives 128,000 lx as the solar illuminance constant and up to 100 klx at the surface |
| Sun at sunset | 3,000 lx at 2,600 K (the database's "Sunset" variant) | same |
| Sky only (no sun), measured: "Shade illuminated by entire clear blue sky at midday" | 6,000 lx (the database's Sky "Clear" variant; against 20,000 lx on Wikipedia for the same scene, see Sources disagree) | same |
| Sky, overcast day | 2,000 lx (range 100 to 2,000) (the database's Sky "Overcast Day" variant) | same |
| Sky, moonless clear night | 0.002 lx (the database's Sky "Moonless Clear Night" variant) | same |
| Moon | full 0.32 lx; crescent 0.05 lx (the database's Moon "Full" and "Crescent" variants) | same |

Single stars (Wikipedia "Lux", citing Schlyter sections 7 and 14): a magnitude-0 star gives 2.54 µlx above the atmosphere and about 82% of that, 2.08 µlx, at the ground on a clear night; a magnitude-6 star (barely visible) gives 8 nlx (8.3 nlx in another paragraph of the same page; see Sources disagree). Each magnitude step is a factor of 100^(1/5) ≈ 2.512 (definition of the magnitude scale).

Indoor reference points from "Lux": living room 50 lx (Australia, 1998); office hallway 80 lx; railway platform 150 lx; office 320-500 lx.

## J16-2. Luminance of things you look at (cd/m²)

Wikipedia "Orders of magnitude (luminance)" section of the same page (citing Hahn 1996, Univ. of Pennsylvania "Photometric Units", and others; secondary):

Second check (2026-11-18): several rows here are derived by Wikipedia's editors, not sourced measurements. The "Photographic scene" rows (1.4 mcd/m², 25, 700, 5,000 cd/m²) are computed by Wikipedia from its illuminance table with an 18% grey card, or from an exposure-value table; rows tagged with the page's "avg" note (250, 700, 1,000, 2,000, 3,000-5,000, 8,000, 10,000 sky, 130,000 cd/m²) are "a (rounded) geometric mean" of wider ranges; the sRGB 80 cd/m² row has no reference; Hahn 1996 is a dead link (archived). Treat this table as order-of-magnitude only.

| Thing | Luminance |
|---|---|
| Absolute threshold of vision | 1 µcd/m² |
| Cloudy night sky without moon | 100 µcd/m² |
| "Darkest sky" | 400 µcd/m² |
| Night sky | 1 mcd/m² |
| Photographic scene lit by full moon | 1.4 mcd/m² |
| Scotopic to mesopic threshold | about 5 mcd/m² |
| Floodlit buildings | 2 cd/m² |
| Mesopic to photopic threshold | about 5 cd/m² |
| Photographic scene at sunrise or sunset | 25 cd/m² |
| Cinema screen (SMPTE) | 55 cd/m² |
| sRGB reference monitor white | 80 cd/m² |
| Typical LCD peak | 250 cd/m² |
| Photographic scene, overcast day | 700 cd/m² |
| Cloudy sky at noon | 1,000 cd/m² |
| Average cloudy sky | 2,000 cd/m² |
| Moon surface | 2,500 cd/m² |
| Clear sky | 3,000-5,000 cd/m²; average clear sky 8,000; clear sky at noon 10,000 |
| Photographic scene in full sun | 5,000 cd/m² |
| White sunlit cloud | 10,000 cd/m² |
| Fluorescent lamp | 12,000 cd/m² |
| Low-pressure sodium lamp | 75,000 cd/m² |
| Frosted incandescent bulb | 130,000 cd/m² |
| Solar disk at the horizon | 600,000 cd/m² |
| Clear incandescent filament | 7 × 10⁶ cd/m² |
| Possible retinal damage | 10⁸ cd/m² |
| Solar disk at noon | 1.6 × 10⁹ cd/m² |

Night sky in astronomers' units: natural zenith brightness about 21.9 mag/arcsec² in the V band (airglow plus zodiacal light plus starlight), up to about 21.0 in extreme natural cases (Wikipedia "Sky brightness", https://en.wikipedia.org/wiki/Sky_brightness, secondary).

## J16-3. Dynamic range a renderer must span (computed by me)

- Ground illuminance: 0.0001 lx (overcast moonless) to 100,000 lx (sun on the ground at midday): a ratio of about 1.0 × 10⁹, 29.9 stops (log2). Taking the Sun above the atmosphere (128,000 lx) instead gives 1.3 × 10⁹, 30.3 stops. Either way, about 30 stops. (Corrected after the second check: this line first used 128,000 lx as the ground maximum.)
- Full Moon (0.25 lx) to direct sun (100,000 lx): about 400,000 times, about 18.6 stops.
- Visible luminance: 1 µcd/m² threshold to 1.6 × 10⁹ cd/m² sun disk: 15 orders of magnitude.

## J16-4. Inverse-square falloff (definition, plus computed example)

Illuminance from a small source of luminous intensity I (cd) at distance d (m), surface facing it: E = I / d², tilted surfaces multiply by cos θ (Wikipedia "Lux", relationship section; this is the SI definition of the lux as lm/m²). Example (computed by me): a source of 1 cd (roughly candle-like) gives 1 lx at 1 m, 0.25 lx (full-Moon level) at 2 m, 0.01 lx (quarter-Moon level) at 10 m.

## Sources disagree

- Overcast day: 1,000 lx ("Lux"), 1,000-2,000 lx midday ("Daylight"), 10-25 klx midday (Orders of magnitude). Overcast varies hugely with cloud depth; use 1-25 klx as the range and 100 lx for "very dark".
- Full Moon: 0.25 lx (Schlyter via Orders of magnitude) against 0.05-0.3 lx (Kyba 2017 via "Lux", typical 0.05-0.1).
- Direct sun: 100 klx upper ("Lux"; and physicallybased.info's Midday variant), 120 klx ("Daylight"). Depends on sun height and haze. (physicallybased.info's 128 klx is the Sun above the atmosphere, so it agrees with "Lux" and is not part of this disagreement.)
- Clear sky luminance: 3,000-5,000 cd/m² and 10,000 cd/m² at noon on the same page.
- Shade lit by the whole clear sky at midday: 6,000 lx (physicallybased.info, Sky "Clear", "Shade illuminated by entire clear blue sky at midday") against 20,000 lx (Wikipedia "Orders of magnitude (illuminance)" and "Daylight") for the same scene, a factor of about 3.3.
- Full Moon spread: Kyba et al. 2017 give 0.05-0.2 lx at mid-latitudes and 0.32 lx only at perigee in the tropics; physicallybased.info's Moon "Full" variant is 0.32 lx, the top of that spread.
- Magnitude-6 star: Wikipedia "Lux" gives 8 nlx in one paragraph and 8.3 nlx in another.

## Not found

- An open measured table of illuminance inside forests, caves, or under water by depth (water attenuation is in round-one G3).
- Firelight illuminance at given distances from a campfire.

## Sources

Wikipedia text is CC BY-SA 4.0; physicallybased.info is CC0 1.0 (the licence of its GitHub repository).

- Wikipedia, "Orders of magnitude (illuminance)" (and its luminance section). https://en.wikipedia.org/wiki/Orders_of_magnitude_(illuminance) . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Lux". https://en.wikipedia.org/wiki/Lux . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Daylight". https://en.wikipedia.org/wiki/Daylight . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Moonlight". https://en.wikipedia.org/wiki/Moonlight . CC BY-SA 4.0. Accessed 2026-11-17.
- Wikipedia, "Sky brightness". https://en.wikipedia.org/wiki/Sky_brightness . CC BY-SA 4.0. Accessed 2026-11-17.
- Schlyter, *Radiometry and photometry in astronomy* FAQ (cited by Wikipedia; it refused the fetch). http://stjarnhimlen.se/comp/radfaq.html .
- Kyba, Mohar and Posch (2017), "How bright is moonlight?", *Astronomy & Geophysics* 58(1) 1.31-1.32, doi:10.1093/astrogeo/atx025. https://doi.org/10.1093/astrogeo/atx025 . Read through Wikipedia by this file; opened by the second checker (GFZ open copy).
- Hahn (1996), Univ. of Pennsylvania, "Photometric Units" (cited by Wikipedia; dead link, archived). No link given.
- physicallybased.info API v2, light sources. https://api.physicallybased.info/v2/lightsources . CC0 1.0. Accessed 2026-11-17.
- Round-one G3 of this library (water attenuation), named under "Not found".
