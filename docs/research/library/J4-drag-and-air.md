# J4: Drag, reference area and air density

Researched 2026-11-17 (all URLs accessed 2026-11-17).

Drag force F_D = ½ ρ C A v² (OpenStax UP1 section 6.4, https://openstax.org/books/university-physics-volume-1/pages/6-4-drag-force-and-terminal-speed). **Reference area A is the projected frontal area**, the area of the object facing the flow (OpenStax: "A is the area of the object facing the fluid"; NASA Glenn: drag coefficients are quoted against the frontal area, https://www1.grc.nasa.gov/beginners-guide-to-aeronautics/shape-effects-on-drag/). Terminal speed v_T = √(2mg / (ρ C A)).

## Table J4-1. Drag coefficients, OpenStax (read directly)

Source: OpenStax UP1 Table 6.2 "Typical Values of Drag Coefficient C", CC BY 4.0, https://openstax.org/books/university-physics-volume-1/pages/6-4-drag-force-and-terminal-speed.

| Object | C |
|---|---|
| Airfoil | 0.05 |
| Toyota Camry | 0.28 |
| Ford Focus | 0.32 |
| Honda Civic | 0.36 |
| Ferrari Testarossa | 0.37 |
| Dodge Ram pickup | 0.43 |
| Sphere | 0.45 |
| Hummer H2 SUV | 0.64 |
| Skydiver (feet first) | 0.70 |
| Bicycle | 0.90 |
| Skydiver (horizontal) | 1.0 |
| Circular flat plate | 1.12 |

## Table J4-2. Drag coefficients by shape, NASA Glenn (read directly)

Source: NASA Glenn Research Center, Beginner's Guide to Aeronautics, "Shape Effects on Drag", https://www1.grc.nasa.gov/beginners-guide-to-aeronautics/shape-effects-on-drag/. US government work.

| Shape | C |
|---|---|
| Flat plate | 1.28 |
| Wedge prism, point downstream | 1.14 |
| Sphere | 0.07 to 0.5 (depends on Reynolds number) |
| Bullet | 0.295 |
| Airfoil | 0.045 |
| Model rocket | 0.75 |

## Table J4-3. Worked human examples with area (read directly)

| Case | m | A | C | ρ | v_T | Source |
|---|---|---|---|---|---|---|
| Skydiver, head first | 75 kg | 0.18 m² | 0.70 | 1.21 kg/m³ | 98 m/s (350 km/h) | OpenStax UP1 section 6.4, worked example in the text, https://openstax.org/books/university-physics-volume-1/pages/6-4-drag-force-and-terminal-speed |
| Skydiver, spread-eagle | 85 kg | 0.70 m² | 1.0 | 1.21 kg/m³ | 44 m/s | same section, worked example |

## Table J4-4. Air temperature, pressure and density with altitude (formula read directly; values computed by me)

Source: NASA Glenn, "Earth Atmosphere Model, Metric Units", https://www.grc.nasa.gov/WWW/K-12/airplane/atmosmet.html. Curve fits, h in metres, T in °C, p in kPa:

- Troposphere (0 to 11,000 m): T = 15.04 − 0.00649·h; p = 101.29·[(T + 273.1)/288.08]^5.256
- Lower stratosphere (11,000 to 25,000 m): T = −56.46; p = 22.65·exp(1.73 − 0.000157·h)
- Upper stratosphere (above 25,000 m): T = −131.21 + 0.00299·h; p = 2.488·[(T + 273.1)/216.6]^−11.388
- Density: ρ = p / [0.2869·(T + 273.1)] (kg/m³)

| h (m) | T (°C) | p (kPa) | ρ (kg/m³) |
|---|---|---|---|
| 0 | 15.04 | 101.40 | 1.227 |
| 1,000 | 8.55 | 89.96 | 1.113 |
| 2,000 | 2.06 | 79.58 | 1.008 |
| 5,000 | −17.41 | 54.11 | 0.738 |
| 8,848 | −42.38 | 31.53 | 0.476 |

Computed by me from the NASA fits; rounded. NASA's Earth fact sheet gives surface density 1.217 kg/m³ (see J10), close to the 1.227 here; OpenStax uses 1.21 kg/m³ in its examples and 1.29 kg/m³ at 0 °C (J3).

## Sources disagree

- Sphere: OpenStax gives a single 0.45; NASA gives 0.07 to 0.5 depending on Reynolds number. Use 0.45 for everyday balls in flight only as a default.
- Flat plate: OpenStax 1.12 (circular), NASA 1.28 (flat plate, shape not further specified).
- OpenStax is inconsistent about the skydiver: Table 6.2 labels C = 0.70 "feet first", while the worked example uses 0.70 for "head first" and later calls the same case "feet first".

## Not found

- Drag area for animals, or for a running human (as opposed to a skydiver). Not found in an open primary source in this pass.
