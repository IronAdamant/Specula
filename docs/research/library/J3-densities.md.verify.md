# Second check: J3-densities.md (2026-11-18)

This is the checker's summary for this file as relayed in its handoff; the checker's per-row table stayed with the checker.

### Round 2: J3-densities.md

**Rows: 35 total (the L5 source row, which carries 2 FIXes, plus 18 solids, 7 liquids, 9 gases and 1 gap row was not counted): 34 VERIFIED, 1 FIX row with 2 fixes, 0 UNVERIFIABLE.**

- **FIX (licence, L5).** The live OpenStax page for §14.1 states "Creative Commons Attribution-NonCommercial-ShareAlike License" (CC BY-NC-SA 4.0), not CC BY 4.0. Flag ⚠NC. The densities themselves are facts.
- **FIX (conditions, L5).** Table 14.1's headers give **solids (0.0 °C)** and **liquids (0.0 °C)**, and gases at 0.0 °C, 101.3 kPa. The file says "Solids and liquids as printed", which loses the stated 0 °C condition. Correct it to "all at 0 °C; gases also at 101.3 kPa".
- All 18 solids, 7 liquids and 9 gases match Table 14.1 "Densities of Some Common Substances" exactly. The table prints them as e.g. 2.70 × 10³, and the file's plain-number conversions are right. The file's "Ethanol" is the book's "Ethyl alcohol", 806, and "Aluminium" is the book's "Aluminum".
- The table gives no spread; it's a single textbook value per substance. That isn't a defect, but these are defaults and not measurements.
- Dead links: none.

FIX and UNVERIFIABLE rows from the table:

| Row | Claim | Status | Note | URL checked (2026-11-17) |
|---|---|---|---|---|
| L5 | Source Table 14.1, CC BY 4.0; solids and liquids "as printed" | FIX | Licence is CC BY-NC-SA 4.0; all at 0 °C | https://openstax.org/books/university-physics-volume-1/pages/14-1-fluids-density-and-pressure |
