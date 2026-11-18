# Second check: H10-joint-curves-walk-run-and-arm-swing.md (2026-11-18)

This is the second checker's summary for this note, as relayed in the checker's findings for round four; its per-row table stayed with the checker. The text below is copied word for word, leaving out only the lines that named the checker's own file locations.

#### Round 4: H9-joint-curves-walk-run-and-arm-swing.md

**26 rows checked: 23 VERIFIED, 3 FIX, 0 UNVERIFIABLE.**

I downloaded both CC0 data files from Zenodo myself:
- Nuckols_PLoS_UpDown_Summary_Repo.mat, read with SciPy
- final_for_dryad.xls, read with pandas

Then I recomputed everything:
- **Nuckols:** all 42 table rows (walk and run, hip, knee, ankle, plus the computed ankle column) match the Mn arrays to 0.1°. So do the ranges, the peaks and their % of stride, and the moment extremes. The four H9-data CSVs (30 columns each, 101 points, 0 to 100%) match the .mat Mn and SD arrays **exactly** at 3 decimals, with a max difference of 0.
- **Killeen:** every mean, SD and age-group mean in the table reproduces from Condition 0 with STARTS=1 (n = 85, ages 21-79, groups 31/25/29).
- **Paper methods:** the Nuckols methods and the 1.02 and 3.66 W/kg powers are confirmed in the Europe PMC full text.

FIX:
- **Row 10, running "stance knee flexion peak about 41° at 10%":** the stance peak is **43.4° at 14%** of stride. 41.3° is just the value at 10%.
- **Row 16, Killeen speed "each person's own speed (from a 25-foot walk test)":** the treadmill was set to **50% of maximal overground speed**, taken from the mean of two timed 25-foot walks done at maximum speed. It was not a self-selected speed. The "self-selected speed" wording under Sources disagree needs the same correction.
- **Row 17, "Plug-in Gait full-body markers":** the paper used Plug-in-Gait for the **upper body** and a **modified Cleveland** marker set for the lower body.

Spreads and notes:
- **Row 18 cohort:** the paper's final analysis has **83** people (31 aged 18-39, 23 aged 40-59, 29 aged 60-80, per the Results and Table 1). The Dryad sheet filtered by STARTS=1 gives 85 (31/25/29). The file's means are correctly computed from the sheet, but it should say they don't match the paper's n=83. The README's age bands (20-39 and so on) also differ from the paper's 18-39.
- **Killeen README:** it says "Please use sheet 3", but the .xls has only one sheet (Sheet1, 363 rows = 121 trials × 3 conditions).
- **Wrist path "per stride":** the README defines it only as "3d mm travelled". The paper defines the arm-swing cycle by max protraction and retraction of the wrist joint centre. "Per stride" is a fair reading, but it isn't stated in the README.
- **"101 points":** this comes from the data file, which has 101 points. The paper itself doesn't state the point count.
- **Grade labels:** the .mat grade labels are percent grades (walk up15 = 8.53°, run up10 = 5.71°). The paper confirms this mapping, but the CSV headers don't say it. It's worth a note.
- **Missing URL:** the Killeen paper has no URL in the source list. Europe PMC has it as PMC5319362 under CC BY 4.0.

FIX and UNVERIFIABLE rows:

| Row | Claim | Status | Note | URL checked (2026-11-18) |
|---|---|---|---|---|
| 10 | Run stance knee peak about 41° at 10% | FIX | 43.4° at 14% | https://zenodo.org/records/3986714 |
| 16 | Own speed from a 25-foot walk test | FIX | 50% of max overground T25FW speed | https://europepmc.org/article/PMC/PMC5319362 |
| 17 | Plug-in Gait full-body | FIX | PiG upper body plus modified Cleveland | https://europepmc.org/article/PMC/PMC5319362 |
