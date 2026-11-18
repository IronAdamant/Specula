# Second check: J31-ion-engines-and-reentry-heating.md (2026-11-18)

This is the second checker's summary for this note, as relayed in the checker's findings for round four; its per-row table stayed with the checker. The text below is copied word for word, leaving out only the lines that named the checker's own file locations.

#### Round 4: J31-ion-engines-and-reentry-heating.md

**32 rows checked: 29 VERIFIED, 3 FIX, 0 UNVERIFIABLE.**

I opened all four NTRS PDFs (via the NTRS API) and the JPL press kit. Every number matches:
- the NEXT Table 3 rows (all 25, including the 4 marked "Not a throttle table operating point") and the Table 6 requirements
- the DS1 quote
- the Sutton-Graves K values, errors, ranges and units
- every Stardust, Pathfinder, MSL, Pioneer Venus and Galileo figure in Venkatapathy
- the Hall-thruster and entry statements on Wikipedia
- my recomputed figures: 34-45 mN/kW and 46,440 km/h = 12.9 km/s

FIX:
- **Row 3, NEXT Table 3 "printed p.7":** it is on **printed p.8** (PDF p.12).
- **Row 6, NEXT Table 6 "p.9":** it is on **printed p.10** (PDF p.14).
- **Row 30, Soyuz quote "uncited source":** Wikipedia cites it to a NASA report, *ISS Soyuz Vehicle Descent Module Evaluation of Thermal Protection System (TPS) Penetration Characteristics*, https://ntrs.nasa.gov/citations/20130013840. The row can be relabelled, and it could be upgraded to primary by opening that report.

Minor or upgrade notes:
- **Row 2:** 0.034-0.044 is the "ratio of doubly-to-singly-charge[d] ion current", not a "double-ion fraction". The source says only that thrust is "calculated"; "not from a thrust stand" is an inference, though a reasonable one.
- **Row 32, Venkatapathy licence:** the NTRS record lists it as distribution **PUBLIC**, copyright **GOV_PUBLIC_USE_PERMITTED**. The same holds for the other three NTRS items. So the licence isn't just assumed from NASA authorship.
- **Sutton-Graves:** Table II is on printed p.38. Printed p.16 is the Results and Discussion page, which presents the results as q̇√(R/p_s) against h_s − h_w and points to eqs. (41)-(42) for K. That matches the file.

Spreads to add under Sources disagree:
- Stardust drogue altitude: 32 km (PDF p.14) vs "approximately 30 kilometers (100,000 feet)" (PDF p.29). Both are at about Mach 1.4.
- MSL peak heat flux grew during design, from about 140 W/cm² to over 250 W/cm² at PDR (PDF p.12).
- Apollo CM heat-shield mass: about 3,000 lb in the text vs 1,869 lb in the spec list, on the same Wikipedia page.
- Atmospheric entry: Wikipedia gives radiative heating as roughly v⁸ against convective v³. The file says only "far faster".

FIX and UNVERIFIABLE rows:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 3 | Table 3 on printed p.7 | FIX | printed p.8 | https://ntrs.nasa.gov/citations/20040000790 |
| 6 | Table 6 (p.9): ≥4,050 s, 4,090/4,070/4,060, efficiencies | FIX | values right, page is printed p.10 | https://ntrs.nasa.gov/citations/20040000790 |
| 30 | Soyuz 21-28 mm plus 8 mm, "uncited" | FIX | cited to NTRS 20130013840 | https://en.wikipedia.org/wiki/Soyuz_(spacecraft) |
