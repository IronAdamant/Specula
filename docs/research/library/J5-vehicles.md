# J5: Vehicle figures

Fixed after the second check (2026-11-18):
- J5-2: the values and the OpenStax quote are right, but the OpenStax material is CC BY-NC-SA 4.0 (non-commercial), not CC BY. Link only.
- J5-1: added the licence of *Special Report 286* (copyright, facts only) and the age of its tyre data (EPA tests of the early 1980s; Michelin and Goodyear figures from 1994 to 1995), so the figures are not read as today's tyres.
- Added a Sources list. The checker's summary is in `J5-vehicles.md.verify.md`.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Partial: rolling resistance and drag found; suspension figures not found.

## Table J5-1. Tyre rolling resistance coefficient (RRC, read directly)

RRC is rolling resistance force divided by wheel load. Source: Transportation Research Board, *Special Report 286: Tires and Passenger Vehicle Fuel Economy* (2006), Chapter 3, https://www.nationalacademies.org/read/11620/chapter/5. Read free online at the National Academies Press. Licence: © 2006 National Academy of Sciences, free to read online with no open licence; copyright, facts only.

**Age of the data.** The EPA tyre set was tested in the early 1980s (Thompson and Reineman 1981; Egeler 1984), and the Michelin and Goodyear figures were 1994 to 1995 submissions to NHTSA (per the second check of SR 286). They are not figures for current tyres.

| Figure | Value | Source |
|---|---|---|
| Typical range, new US passenger tyres | 0.007 to 0.014 | SR 286 ch. 3: "For most passenger tires sold in the United States, the coefficient of the tire measured when it is new falls between 0.007 and 0.014." https://www.nationalacademies.org/read/11620/chapter/5 |
| Worked example in the report | 1,000 lb load gives 7 to 14 lb rolling resistance per tyre | same |
| EPA test of 36 radial tyre lines (tested early 1980s) | 0.0098 to 0.0138, mean 0.0113 | SR 286 ch. 3, Table 3-2 discussion |
| Michelin, 24 original-equipment tyres for MY1995 vehicles (1994 to 1995 NHTSA submission) | 0.0077 to 0.0114 | SR 286 ch. 3, Table 3-3 discussion |
| Goodyear estimate, new OE tyres (1994 to 1995 NHTSA submission) | 0.0067 to 0.0152 | SR 286 ch. 3 |
| Goodyear estimate, new replacement tyres (1994 to 1995 NHTSA submission) | 0.0073 to 0.0131 | SR 286 ch. 3 |
| Effect on fuel use | a 10% cut in RRC gives about 1 to 2% better fuel economy | SR 286 ch. 3 |
| Share of mechanical energy at the wheels lost to rolling resistance | about one-third | SR 286 ch. 3 |

All RRCs above were measured on new tyres under SAE J1269 unless the report says otherwise (SR 286 ch. 3).

## Table J5-2. Car drag coefficients

See J4-1 (OpenStax Table 6.2, CC BY-NC-SA 4.0, link only): Camry 0.28, Focus 0.32, Civic 0.36, Testarossa 0.37, Ram pickup 0.43, Hummer H2 0.64, bicycle 0.90. OpenStax adds that "at highway speeds, over 50% of the power of a car is used to overcome air drag" (https://openstax.org/books/university-physics-volume-1/pages/6-4-drag-force-and-terminal-speed). Frontal areas for these cars are not given by OpenStax.

## Table J5-3. Tyre grip

See J1-1 (rubber on dry concrete μs 1.0, μk 0.7; wet 0.5–0.7 and 0.3–0.5) and J1-2 for the braking deceleration it implies (computed).

## Not found

- Suspension natural frequency, spring rates and damping ratios for typical cars, in an open primary source. Not added rather than guessed.
- Typical vehicle masses, frontal areas, acceleration figures and power curves from an open government table. Not reached in this pass.
- Government stopping-distance tables (for example state road-authority tables) were not read in this pass.

## Sources

- Transportation Research Board, *Special Report 286: Tires and Passenger Vehicle Fuel Economy* (2006), Chapter 3, National Academies Press, © 2006 National Academy of Sciences (facts only), https://www.nationalacademies.org/read/11620/chapter/5 (accessed 2026-11-17).
- OpenStax, *University Physics Volume 1*, section 6.4 Drag Force and Terminal Speed, CC BY-NC-SA 4.0 (link only), https://openstax.org/books/university-physics-volume-1/pages/6-4-drag-force-and-terminal-speed (accessed 2026-11-17).
