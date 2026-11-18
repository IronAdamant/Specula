# Second check: J29-bullet-drag-bc-and-penetration.md (2026-11-18)

This is the second checker's summary for this note, as relayed in the checker's findings for round four; its per-row table stayed with the checker. The text below is copied word for word, leaving out only the lines that named the checker's own file locations.

#### Round 4: J29-bullet-drag-bc-and-penetration.md

**26 rows checked: 22 VERIFIED, 4 FIX, 0 UNVERIFIABLE.**

What checks out:
- **JBM G1 and G7 drag tables:** all 19 selected Mach rows match mcg1.txt and mcg7.txt exactly (79 and 84 rows), as do the peaks and the G1/G7 ratios.
  - All 19 Mach values are actual table points, so the "*" interpolation note is never used. That's fine.
- **Hornady BCs:** all 8 rows match the live page, as does the G1/G7 BC ratio of 1.98-1.99.
- **Lost River J40 Doppler table:** matches.
- **NIJ 0101.06 test threats:** all match (masses, velocities, ±9.1 m/s, 44 mm), as does the 18-28 m/s conditioned offset.
- **FM 3-06.11:** Table 7-3, Table 7-2, Table 7-5 and the text facts all match, and the cm conversions are correct.
- **Gelatin and FBI:**
  - The gelatin recipe, the INS calibration (183 ± 3 m/s, 8.3-9.5 cm) and the 4 °C "citation needed" tag match.
  - The FBI quotes match the transcription word for word, as do the author (Special Agent Urey W. Patrick) and the date (July 14, 1989).

FIX:
- **Row 5, Hornady "measured by Doppler radar":** the Hornady BC page doesn't say this. I checked both a curl fetch and a rendered fetch, and the word "Doppler" isn't there. The page says only that the values are "corrected back to … ICAO Standard Atmosphere (Sea Level, 59 degrees F, 29.92 inHg, 0% Humidity)". Remove the Doppler claim or cite a Hornady page that states it.
- **Row 9, "across the page BC falls between 1.6% (375 cal 390 gr) and 8.6%":** recomputing over every bullet on the page (G1, Mach 2.25 to 1.75), the smallest drop is **0.68% for the 30 cal 250 gr A-Tip Match** (0.878 to 0.872). The 375 cal 390 gr drops 1.62%, and the 7 mm 175 gr ELD-X drops 1.60%. The 8.6% for the 6.5 mm 147 gr is correct.
- **Row 14, Roma Plastilina backing in "section 7":** it is in **section 4.2.5.3, "Backing Material"** (under 4.2.5, Armor Backing Material). Section 7 is the test methods.
- **Row 18, FM Table 7-6, ".50 ball vs 10 in reinforced concrete: 50 rounds for a 7 in hole":** the host's HTML table stacks the cells:
  - thickness 10 and 18 in
  - hole 12, 24 and 7 in
  - rounds 50, 100 and 140

  Read in that order, 10 in gives a 12 in hole for 50 rounds (and 24 in for 100), and 18 in gives 7 in for 140. Pairing 50 rounds with a 7 in hole isn't supported. Open the official PDF (the file already says it didn't) before using this cell.

Missing context:
- J40: the maker assigned a BC of **1.062 lb/in²** (746.7 kg/m²), which is in the same paragraph as the Doppler table. It's a useful spread against the 1.032-1.068 measured.
- Hornady lists the Mach 2.0 speed as both 2,233 and 2,232 fps on the same page.
- The page's markup also has a second, hidden row for the 338 cal 300 gr A-Tip with different Mach 2.0 and 1.75 values (0.859/0.818 vs 0.860/0.850). The file doesn't use that row.
- NIJ conditioned velocities: IIA 355 and 325, II 379 and 408, IIIA 430 and 408 m/s. The file gives only one example.
- Gelatin: the 10% recipe sentence carries no reference. The INS citation supports only the BB calibration.

FIX and UNVERIFIABLE rows:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 5 | Hornady BCs Doppler-measured | FIX | not on the page | https://www.hornady.com/bc |
| 9 | BC drop 1.6%-8.6% across the page | FIX | minimum is 0.68% (30 cal 250 gr A-Tip) | https://www.hornady.com/bc |
| 14 | Roma clay in section 7 | FIX | §4.2.5.3 | https://www.ojp.gov/pdffiles1/nij/223054.pdf |
| 18 | Table 7-6 RC 10 in: 50 rounds for a 7 in hole | FIX | cell order doesn't support it | https://www.globalsecurity.org/military/library/policy/army/fm/3-06-11/ch7.htm |
