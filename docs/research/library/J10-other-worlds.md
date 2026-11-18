# J10: Other worlds (gravity, air, temperature)

Second check (2026-11-18): clean (36 of 36 rows verified; Titan's gravity recomputed as 1.3543 m/s²). Small additions, each from the same NSSDC fact sheets as the checker named them:
- Venus: scale height 15.9 km; air 96.5% CO₂, 3.5% N₂.
- Earth: diurnal temperature range 283 to 293 K; surface winds 0 to 100 m/s.
- Mars: surface pressure at the Viking 1 site 6.9 to 9 mb.
- Earth's escape speed is 11.186 km/s on the Earth sheet itself (11.19 in the Mars sheet's comparison column, as the table cites).
- Added a Sources list. The checker's summary is in `J10-other-worlds.md.verify.md`.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Sources: NASA NSSDC planetary fact sheets by David R. Williams (US government work) and NASA Science's Titan facts page.

## Table J10-1. Surface conditions (read directly unless marked)

| Body | Surface gravity (m/s²) | Escape speed (km/s) | Surface pressure | Surface air density (kg/m³) | Temperature | Source |
|---|---|---|---|---|---|---|
| Earth | 9.820 mean; 9.780 equator; 9.832 pole; standard 9.80665 | 11.19 (Mars sheet comparison column; 11.186 on the Earth sheet) | 1,014 mb | 1.217 | 288 K mean; diurnal range 283 to 293 K | https://nssdc.gsfc.nasa.gov/planetary/factsheet/earthfact.html |
| Moon | 1.62 | 2.38 | about 3×10⁻¹⁵ bar at night (effectively none) | none to speak of | 95 to 390 K | https://nssdc.gsfc.nasa.gov/planetary/factsheet/moonfact.html |
| Mars | 3.73 | 5.03 | 6.36 mb mean (4.0 to 8.7 mb range); 6.9 to 9 mb at the Viking 1 site | about 0.016 ± 0.006 | about 214 K mean | https://nssdc.gsfc.nasa.gov/planetary/factsheet/marsfact.html |
| Venus | 8.87 | 10.36 | 92 bar | about 65 | 737 K mean | https://nssdc.gsfc.nasa.gov/planetary/factsheet/venusfact.html |
| Titan | 1.354 (computed by me, see below) | not recorded | about 60% greater than Earth's | not given | −179 °C | mass, radius: https://nssdc.gsfc.nasa.gov/planetary/factsheet/saturniansatfact.html; pressure, temperature, air: https://science.nasa.gov/saturn/moons/titan/facts/ |

The Earth sheet calls 9.80665 the "standard acceleration … (CODATA 2018) … (exact)" (per the second check).

Atmospheric scale height: Earth 8.5 km, Mars 11.0 km, Venus 15.9 km (NSSDC fact sheets, as above).

Mars diurnal range 184 to 242 K at the Viking 1 site; air 95.1% CO₂. Venus air 96.5% CO₂, 3.5% N₂. Winds near the surface: Mars 2 to 7 m/s in summer, 5 to 10 m/s in autumn, 17 to 30 m/s in dust storms (Viking landers); Venus 0.3 to 1.0 m/s; Earth 0 to 100 m/s (NSSDC fact sheets, as above).

Titan air: about 95% nitrogen and 5% methane; the atmosphere extends about 600 km up, ten times higher than Earth's; NASA compares the surface pressure to being about 15 m under the sea on Earth (NASA Titan facts, as above).

**Titan gravity (computed by me):** g = GM/r² with G = 6.674×10⁻¹¹ m³ kg⁻¹ s⁻², M = 1,345.5×10²⁰ kg and r = 2,575 km (NSSDC Saturnian satellite fact sheet: mass 1,345.5 ×10²⁰ kg, radius 2,575 km, density 1,880 kg/m³). Result 1.354 m/s².

## Not found

- Titan surface air density as a number on a NASA page (the Huygens probe value was not read). The second check did not find it on the two NASA pages either.

## Sources

All US government works (NASA); the numbers are free to use. NASA's image and media guidelines apply to pictures, not to these numbers.

- NASA NSSDC, Earth Fact Sheet (D. R. Williams), https://nssdc.gsfc.nasa.gov/planetary/factsheet/earthfact.html (accessed 2026-11-17).
- NASA NSSDC, Moon Fact Sheet, https://nssdc.gsfc.nasa.gov/planetary/factsheet/moonfact.html (accessed 2026-11-17).
- NASA NSSDC, Mars Fact Sheet, https://nssdc.gsfc.nasa.gov/planetary/factsheet/marsfact.html (accessed 2026-11-17).
- NASA NSSDC, Venus Fact Sheet, https://nssdc.gsfc.nasa.gov/planetary/factsheet/venusfact.html (accessed 2026-11-17).
- NASA NSSDC, Saturnian Satellite Fact Sheet, https://nssdc.gsfc.nasa.gov/planetary/factsheet/saturniansatfact.html (accessed 2026-11-17).
- NASA Science, Titan facts, https://science.nasa.gov/saturn/moons/titan/facts/ (accessed 2026-11-17).
