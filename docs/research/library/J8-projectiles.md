# J8: Projectiles and thrown objects

Second check (2026-11-18): clean (14 of 14 rows verified; J8-1 recomputed). Small additions:
- The OpenStax-derived inputs (ρ 1.21, C 0.45) carry OpenStax's CC BY-NC-SA 4.0 licence (non-commercial, link only), not CC BY.
- A note on where the 0.45 sphere value holds (a smooth sphere below the drag crisis).
- Added a Sources list. The checker's summary is in `J8-projectiles.md.verify.md`.

Researched 2026-11-17 (all URLs accessed 2026-11-17). This file gathers what a thrown or kicked object needs; the numbers live in the files named.

| Need | Value or formula | Where |
|---|---|---|
| Gravity, standard | g = 9.80665 m/s² (standard; NASA Earth fact sheet notes this value) | J10-1 |
| Gravity on Earth's surface | 9.780 (equator) to 9.832 m/s² (pole), mean 9.820 | J10-1 |
| Drag force | F = ½ ρ C A v², A = projected frontal area | J4 |
| Sphere drag coefficient | 0.45 (OpenStax) or 0.07 to 0.5 by Reynolds number (NASA) | J4-1, J4-2 |
| Air density by altitude | NASA curve fits, about 1.23 kg/m³ at sea level | J4-4 |
| Ball masses and sizes (tennis, basketball, football, table tennis) | governing-body rules | J2-2 |
| Bounce (restitution) of those balls | drop tests, e computed | J2-1 |
| Terminal speed | v_T = √(2mg / (ρ C A)) | J4 |

## Table J8-1. Terminal speed of balls (computed by me)

Using ρ = 1.21 kg/m³ (OpenStax's example value), C = 0.45 (OpenStax sphere), A = π·(d/2)², g = 9.80665 m/s², mass and diameter at the middle of each governing-body range in J2-2. Drag on real balls varies with speed and spin (NASA: sphere C is 0.07 to 0.5), so treat these as rough defaults. The 0.45 sphere value is for a smooth sphere in the subcritical range: a real football near 20 m/s is close to the drag crisis, and a tennis ball's felt raises C (second check). The OpenStax inputs (ρ 1.21, C 0.45) are from a CC BY-NC-SA 4.0 book (non-commercial: link only; see J4).

| Ball | m used (kg) | d used (m) | range it came from | v_T (m/s) |
|---|---|---|---|---|
| Tennis ball (Type 2) | 0.0577 | 0.0670 | mass 56.0–59.4 g, diameter 6.54–6.86 cm | 24.3 |
| Basketball size 7 | 0.6000 | 0.2419 | 580–620 g, circumference 750–770 mm | 21.7 |
| Football (soccer) | 0.4300 | 0.2196 | 410–450 g, circumference 68–70 cm | 20.2 |

Diameters from circumference were computed as C/π.

## Not found

- Spin (Magnus) lift coefficients for balls in an open primary table.
- Arrow, javelin and thrown-rock drag figures.

## Sources

This file cites no source of its own: every value is taken from J2, J4 and J10, where each is sourced. Through them it rests on:

- OpenStax, *University Physics Volume 1*, section 6.4 Drag Force and Terminal Speed, CC BY-NC-SA 4.0 (link only), https://openstax.org/books/university-physics-volume-1/pages/6-4-drag-force-and-terminal-speed (accessed 2026-11-17).
- NASA Glenn Research Center, "Shape Effects on Drag", https://www1.grc.nasa.gov/beginners-guide-to-aeronautics/shape-effects-on-drag/ and "Earth Atmosphere Model, Metric Units", https://www.grc.nasa.gov/WWW/K-12/airplane/atmosmet.html (accessed 2026-11-17).
- NASA NSSDC, Earth Fact Sheet, https://nssdc.gsfc.nasa.gov/planetary/factsheet/earthfact.html (accessed 2026-11-17).
- The governing-body ball rules listed in J2's Sources (ITF, FIBA, IFAB, ITTF), accessed 2026-11-17.
