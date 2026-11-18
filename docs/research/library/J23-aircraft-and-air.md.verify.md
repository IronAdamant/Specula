# Second check: J23-aircraft-and-air.md (2026-11-18)

This is the second checker's summary for this file, as relayed in its handoff of findings; the checker's full per-row table stayed with the checker.

### Round 3: J23-aircraft-and-air.md

**54 rows checked: 51 VERIFIED, 2 FIX, 1 UNVERIFIABLE.** I recomputed the whole ISA table, the turn, CLmax, rotor and wind-law arithmetic. All of it matches except one density rounding. Every one of the nine NACA 0012 rows matches the TMR Ladson file digit for digit.

FIX:
- Row 4 (ISA at 2,000 m): ρ is **1.006** kg/m³, not 1.007. The file's own formula gives 1.00649.
- Row 35 (F-16C): the infobox span is **32 ft 8 in**, not 32 ft.

UNVERIFIABLE:
- Row 44: figure of merit "typically 0.7 to 0.8" (Leishman, ch. 2). The book wasn't opened. The arithmetic using 0.75 is correct.

Missing spreads and context:
- Rotor table: the file flags only the R22's disc-loading mismatch, but all three infoboxes are inconsistent with their own MTOW and rotor area. The Bell 206 infobox gives 4 lb/sq ft (19.5 kg/m²), against 3.67 lb/sq ft (17.9 kg/m²) from 3,200 lb on 873 sq ft. The UH-60 infobox gives **7.19 lb/sq ft (35.1 kg/m²)**, against 9.73 lb/sq ft (47.5 kg/m²) from 22,000 lb on 2,260 sq ft, which is a 35% gap. The infobox values look like they're based on lower weights, so add both to Sources disagree.
- Spitfire Vb: the infobox also gives a gross weight of 6,622 lb, separate from the 6,700 lb MTOW. The F-16 infobox gives a 26,500 lb gross and a 50,000 ft ceiling, and adds that thrust/weight is 1.24 at loaded weight with 50% fuel. The 737-800 span without winglets is 34.32 m.
- Lift-to-drag: the same article's prose gives Concorde "about 7 at Mach 2" and the 747 "about 17 at about Mach 0.85", against 7.5 and 17.7 in its list. That's a small internal spread.
- Sources disagree, row 1: I recomputed 0.4135 kg/m³ at 10 km geometric (9,984.3 m geopotential, giving 0.41351). That confirms the file's figure and its 16 m offset (10,015.8 m geometric is 10 km geopotential). I didn't find the printed table value in the NTRS full-text (OCR) file.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 4 | 2,000 m: 2.0 °C, 79,495 Pa, 1.007 | FIX | ρ is 1.006 | (computed) |
| 35 | F-16C row | FIX | span 32 ft 8 in | https://en.wikipedia.org/wiki/General_Dynamics_F-16_Fighting_Falcon |
| 44 | FM 0.7-0.8 (Leishman ch. 2) | UNVERIFIABLE | book not opened | n/a |
