# J8: Projectiles and thrown objects

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

Using ρ = 1.21 kg/m³ (OpenStax's example value), C = 0.45 (OpenStax sphere), A = π·(d/2)², g = 9.80665 m/s², mass and diameter at the middle of each governing-body range in J2-2. Drag on real balls varies with speed and spin (NASA: sphere C is 0.07 to 0.5), so treat these as rough defaults.

| Ball | m used (kg) | d used (m) | range it came from | v_T (m/s) |
|---|---|---|---|---|
| Tennis ball (Type 2) | 0.0577 | 0.0670 | mass 56.0–59.4 g, diameter 6.54–6.86 cm | 24.3 |
| Basketball size 7 | 0.6000 | 0.2419 | 580–620 g, circumference 750–770 mm | 21.7 |
| Football (soccer) | 0.4300 | 0.2196 | 410–450 g, circumference 68–70 cm | 20.2 |

Diameters from circumference were computed as C/π.

## Not found

- Spin (Magnus) lift coefficients for balls in an open primary table.
- Arrow, javelin and thrown-rock drag figures.
