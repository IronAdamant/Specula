# Second check: J22-ground-vehicles.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its handoff of findings; the checker's full per-row table stayed with the checker.

### Round 3: J22-ground-vehicles.md

**64 rows checked: 61 VERIFIED, 3 FIX, 0 UNVERIFIABLE.** All eight NHTSA vehicle rows match the Auburn copy of SAE 1999-01-1336 digit for digit, including the Expedition GVWR row. Every "computed by me" value recomputes.

FIX:
- Row 39: the *Racecar Engineering* 2020-11 column is by **Claude Rouelle** (OptimumG), as its byline says, not "C. Haney". The values (1.1-1.8 Hz, 1.5-3.0 Hz, 3.0-7.0 Hz) are right.
- Row 52: the trail and head-angle ranges come from "a small survey by **Whitt and Wilson**" (*Bicycling Science*, 2nd ed., 1982, p. 188), not "Zinn and others". The values are right.
- Row 63 (Sources disagree): the Braking distance article does **not** give 1 s as a population average. It says a keen, alert driver may react "well below 1 second". It quotes NHTSA as using **1.5 s for the average reaction time**, and its figure legend uses 3/4 s. Rewrite the row as 0.67 s (Highway Code) against 1.5 s (NHTSA, reconstruction baseline, Taoka), with alert drivers under 1 s and a 2.5 s slow driver (AASHTO).

Missing spreads and context:
- Row 10: the paper also gives the facility's inertia error bounds (1% for pitch and yaw, 2% for roll). The file only gives the ±0.5% CG height bound.
- Row 48: the same paragraph gives the result of the 60/120 cm geometry, a **maximum deceleration of about 0.5 g** for front braking on an upright bike. The file omits the number. It also says a recumbent could reach 0.8 g if μ = 0.8.
- Row 61: Ground pressure also lists a mountain bike at 245 kPa and "a galloping horse will exert up to 3.5 MPa". Several rows do carry their own citations (Thompson, *Sustainable Landscape Construction*, p. 50 for the human; mathscinotes for the M1), so "rows not individually cited" is only partly true.
- Rows 55-60: the infoboxes give ranges where the file gives lower bounds or a single value. Golf Mk8 is 1,255-1,465 kg. Model 3 is 3,552-4,048 lb. The Xcelsior 40 ft is 26,000-30,500 lb and the 60 ft is 39,000-45,500 lb. The M1 runs from 60 ST (M1) to 73.6 ST (M1A2 SEPv3). Leopard 2 runs from 55.2 t (2A4) to 66.5 t (2A7V).
- Row 64: I recomputed the listed SSFs. They match the **mean** of front and rear track (Tercel 1.389, S10 1.145, Caravan 1.243). That confirms the file's "presumably". The note can be firmed up.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 39 | Racecar column 1.1-1.8, 1.5-3.0, 3.0-7.0 Hz, "C. Haney" | FIX | author Claude Rouelle | https://optimumg.com/wp-content/uploads/2021/10/racecar-2020_11.pdf |
| 52 | Trail/head-angle ranges "citing Zinn and others" | FIX | Whitt & Wilson 1982 p. 188 | same |
| 63 | Disagreement: 0.67 s vs 1.5 s vs "1 s population average" | FIX | 1 s claim not in source; NHTSA 1.5 s average | Braking distance page |
