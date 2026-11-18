# Second check: J28-engine-torque-curves-and-gears.md (2026-11-18)

This is the second checker's summary for this note, as relayed in the checker's findings for round four; its per-row table stayed with the checker. The text below is copied word for word, leaving out only the lines that named the checker's own file locations.

#### Round 4: J28-engine-torque-curves-and-gears.md

**27 rows checked: 25 VERIFIED, 2 FIX, 0 UNVERIFIABLE.**

I downloaded all three EPA zips, the SAE PDF and the ORNL PDF, and recomputed every table from the raw xlsx sheets with my own script:
- **Toyota "High Load Initial":** all 14 speed groups match exactly, with the same n, low and high values, and power. The 37 points match the CSV.
- **Ford "High Load Initial":** all 8 points match, with the same power.
- **BMW "Engine Mapping":** the 250 rpm nearest bins (M_TRQENG vs Speed) match all 15 rows, including the point counts, the bin-centre power and the 190.4 kW peak at 3750.
- **CSVs:** all three match the xlsx point for point (37, 8 and 205 points). Every power value equals T x rpm / 9549.3.
- **Quotes and ratings:**
  - The method quotes, the Table 1 ratings of all three engines and the BMW BMEP-step sentence all match the package .docx files word for word.
  - The SAE Table 2 and Table 6 gear ratios and torque capacities match.
  - The ORNL author list and the "60 kW for 18 seconds … 150°C" quote match.
- **"Computed by me" items:** the overall ratios (14.71, 1.886, 7.80), the 4.6% gap and the 95 Nm at 60 kW are all correct.

FIX:
- **Row 17, ORNL Table 2.1 page:** the file says printed p. 5. The table is on **printed p. 6** (PDF p. 15). The p. 5 footer sits just above the table in the text layer. The values are correct: 60 kW, 207 Nm, 13,500 rpm and 42 kW generator for the 2010 Prius; LS 600h 300 Nm, 10,230 rpm; Camry 270 Nm, 14,000 rpm; 2004 Prius 400 Nm, 6,000 rpm, 50 kW @ 1200-1540 rpm.
- **Row 27, the CSV citation:** the licence note says the raw point lists "are re-exported in J28-data/ **with the citation**". None of the three CSVs has a citation line; each has only the header `speed_rpm,torque_Nm,power_kW_computed_by_me`. Add the EPA "SUGGESTED CITATION" text, as a comment row or a README in J28-data/.

Missing context:
- Fig. 3.11's torque axis **stops at 200 Nm**, and the yellow peak line runs along that top edge from 0 to about 2,200 rpm. The "200 vs 207 Nm" item under Sources disagree may be just the axis limit. Say so, or treat the published 207 Nm as the plateau.
- In the Table 2.1 row, the LS 600h and Camry motor powers (165 kW @ 5250 rpm and 105 kW @ 4500 rpm) are both marked "(disputed)" in the source. The file leaves the powers out, which is fine, but if they're added later, keep the "(disputed)" flag.
- Ford steady-state bins: the file lists 10 of the 14 nearest-250 bins. It skips 2250 (218), 2750 (208), 3250 (169) and 3750 (156 Nm). The skipped 3250 and 3750 dips show the steady-state map isn't monotonic. That's fine, since the file already says it is "not a WOT curve".
- BMW "Single Variable Geometry Turbocharger (VGT), Intercooled" is confirmed in Table 1.

FIX and UNVERIFIABLE rows:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 17 | Table 2.1 on printed p. 5 | FIX | printed p. 6 / PDF p. 15 | https://info.ornl.gov/sites/publications/files/Pub26762.pdf |
| 27 | CSVs "with the citation" | FIX | no citation in the CSVs | (local files) |
