# Verification of A3-girths-depths.md

Checked 2026-11-16 (AEDT, UTC+11) by Library Verify's checker. The researcher's file was not edited.

**File checked:** mtime 2026-11-16 10:30:00 +1100, 248 lines, 41708 bytes, recorded before checking. Re-measured when this report was written: 248 lines, 41708 bytes (unchanged = yes).

## Summary

**Rows: 149 total: 130 VERIFIED, 19 FIX, 0 UNVERIFIABLE.**

- **Worst problem: all 19 ANSUR page references in Table A3-1 are one page too high** (index offset, same as A1). E.g. chest circumference is printed pp. 91–92 (file 92–93); waist circumference (omphalion) pp. 217–218 (file 218–219). All values match.
- A3-2 (38 rows) and both correlation matrices recomputed exactly from the CSVs. BMI F 25.5 ± 3.5, M 27.7 ± 4.0 ✓; r(mass, stature) 0.53 F / 0.47 M ✓.
- A3-3 (64 NHANES rows) and A3-4 (10 rows) reproduced with WTMEC2YR weights; r(weight, height) 0.43 M / 0.27 F ✓.
- Blocked link: Fryar 2021 https://www.cdc.gov/nchs/data/series/sr_03/sr03-046-508.pdf returns 403 (Akamai) to curl; the **Wayback copy (2026-11-09) downloaded and is the correct report**, and its public-domain statement is quoted correctly. The girth tables the researcher could not read are present in that archived copy (not used for any number here).
- Structure check: rows have URLs; access date only in header. Not found / Sources disagree present.

### Grok live-search pass

Ran 2026-11-16 ~10:47–10:53 AEDT: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` (live web search on), single 10-question prompt; waited until the process exited (exit 0). A Grok answer counted only where it gave a URL that I then opened myself; no number was taken from Grok alone.

For A3: no A3-specific question was put to Grok (all A3 numbers were settled by direct recomputation). No conflicts.

## Row table

L = line number in the researcher's file. Status: VERIFIED = re-read or recomputed from the source and matches; FIX = correction needed (corrected value given); UNVERIFIABLE = could not be re-read (reason given).

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| L15 | Chest Circumference (24) ; cm ; 94.69 ; 8.27 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 91–92, not pp. 92–93 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L16 | Waist Circumference (Omphalion) (87) ; cm ; 86.09 ; 9.99 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 217–218, not pp. 218–219 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L17 | Buttock Circumference (16) ; cm ; 102.12 ; 7.59 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 75–76, not pp. 76–77 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L18 | Thigh Circumference (78) ; cm ; 61.61 ; 5.58 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 199–200, not pp. 200–201 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L19 | Lower Thigh Circumference (60) ; cm ; 40.07 ; 3.52 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 163–164, not pp. 164–165 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L20 | Calf Circumference (21) ; cm ; 37.33 ; 2.85 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 85–86, not pp. 86–87 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L21 | Biceps Circumference, Flexed (9) ; cm ; 30.56 ; 3.08 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 61–62, not pp. 62–63 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L22 | Forearm Circumference, Flexed (38) ; cm ; 26.41 ; 1.85 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 119–120, not pp. 120–121 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L23 | Neck Circumference (62) ; cm ; 32.98 ; 1.92 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 167–168, not pp. 168–169 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L24 | Neck Circumference, Base (63) ; cm ; 37.12 ; 1.97 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 169–170, not pp. 170–171 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L25 | Wrist Circumference (92) ; cm ; 15.48 ; 0.78 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 227–228, not pp. 228–229 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L26 | Ankle Circumference (4) ; cm ; 21.57 ; 1.49 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 51–52, not pp. 52–53 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L27 | Chest Depth (25) ; cm ; 24.74 ; 2.73 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 93–94, not pp. 94–95 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L28 | Waist Depth (88) ; cm ; 21.30 ; 3.14 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 219–220, not pp. 220–221 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L29 | Buttock Depth (17) ; cm ; 23.29 ; 2.43 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 77–78, not pp. 78–79 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L30 | Chest Breadth (23) ; cm ; 26.93 ; 1.86 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 89–90, not pp. 90–91 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L31 | Waist Breadth (86) ; cm ; 29.99 ; 3.30 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 215–216, not pp. 216–217 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L32 | Hip Breadth (50) ; cm ; 35.38 ; 2.67 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 143–144, not pp. 144–145 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L33 | Weight (91) ; kg ; 67.76 ; 10.98 | FIX | Mean, SD, P5, P95 all match the report. Page FIX: printed pp. 225–226, not pp. 226–227 (the report's index runs one page ahead of the printed page numbers). | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) |
| L43 | chestcircumference ; F ; 946.9 ; 82.7 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L44 | chestcircumference ; M ; 1058.7 ; 87.4 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L45 | waistcircumference ; F ; 860.9 ; 99.9 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L46 | waistcircumference ; M ; 940.6 ; 111.7 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L47 | buttockcircumference ; F ; 1021.2 ; 75.9 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L48 | buttockcircumference ; M ; 1019.5 ; 76.7 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L49 | thighcircumference ; F ; 616.1 ; 55.8 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L50 | thighcircumference ; M ; 625.1 ; 58.5 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L51 | lowerthighcircumference ; F ; 400.7 ; 35.2 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L52 | lowerthighcircumference ; M ; 409.3 ; 31.9 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L53 | calfcircumference ; F ; 373.3 ; 28.5 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L54 | calfcircumference ; M ; 392.3 ; 29.7 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L55 | bicepscircumferenceflexed ; F ; 305.6 ; 30.8 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L56 | bicepscircumferenceflexed ; M ; 358.1 ; 34.6 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L57 | forearmcircumferenceflexed ; F ; 264.1 ; 18.5 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L58 | forearmcircumferenceflexed ; M ; 310.1 ; 22.0 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L59 | neckcircumference ; F ; 329.8 ; 19.2 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L60 | neckcircumference ; M ; 397.6 ; 25.8 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L61 | neckcircumferencebase ; F ; 371.2 ; 19.7 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L62 | neckcircumferencebase ; M ; 434.6 ; 25.7 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L63 | wristcircumference ; F ; 154.8 ; 7.8 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L64 | wristcircumference ; M ; 175.9 ; 9.0 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L65 | anklecircumference ; F ; 215.7 ; 14.9 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L66 | anklecircumference ; M ; 229.3 ; 14.6 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L67 | shouldercircumference ; F ; 1028.2 ; 52.9 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L68 | shouldercircumference ; M ; 1178.6 ; 63.6 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L69 | chestdepth ; F ; 247.4 ; 27.3 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L70 | chestdepth ; M ; 253.8 ; 26.2 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L71 | waistdepth ; F ; 213.0 ; 31.4 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L72 | waistdepth ; M ; 237.8 ; 34.7 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L73 | buttockdepth ; F ; 232.9 ; 24.3 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L74 | buttockdepth ; M ; 245.8 ; 26.5 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L75 | chestbreadth ; F ; 269.3 ; 18.6 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L76 | chestbreadth ; M ; 289.4 ; 18.3 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L77 | waistbreadth ; F ; 299.9 ; 33.0 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L78 | waistbreadth ; M ; 326.4 ; 34.7 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L79 | hipbreadth ; F ; 353.8 ; 26.7 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L80 | hipbreadth ; M ; 345.7 ; 24.2 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L91 | chest ; 1.00 ; 0.89 ; 0.84 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L92 | waist ; 0.89 ; 1.00 ; 0.88 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L93 | buttock ; 0.84 ; 0.88 ; 1.00 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L94 | thigh ; 0.80 ; 0.83 ; 0.94 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L95 | calf ; 0.73 ; 0.71 ; 0.79 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L96 | biceps ; 0.79 ; 0.68 ; 0.74 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L97 | neck ; 0.79 ; 0.74 ; 0.71 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L98 | mass ; 0.91 ; 0.89 ; 0.93 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L99 | stature ; 0.25 ; 0.21 ; 0.34 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L104 | chest ; 1.00 ; 0.84 ; 0.74 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L105 | waist ; 0.84 ; 1.00 ; 0.80 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L106 | buttock ; 0.74 ; 0.80 ; 1.00 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L107 | thigh ; 0.69 ; 0.76 ; 0.94 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L108 | calf ; 0.56 ; 0.56 ; 0.74 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L109 | biceps ; 0.75 ; 0.76 ; 0.79 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L110 | neck ; 0.73 ; 0.70 ; 0.64 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L111 | mass ; 0.83 ; 0.84 ; 0.91 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L112 | stature ; 0.26 ; 0.23 ; 0.38 | VERIFIED | Recomputed from CSVs; matches. | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv |
| L120 | Standing height (cm) ; M ; 20-39 ; 731 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L121 | Weight (kg) ; M ; 20-39 ; 729 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L122 | BMI (kg/m²) ; M ; 20-39 ; 729 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L123 | Waist circumference (cm) ; M ; 20-39 ; 709 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L124 | Hip circumference (cm) ; M ; 20-39 ; 710 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L125 | Mid-upper-arm circumference (cm) ; M ; 20-39 ; 711 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L126 | Upper arm length (cm) ; M ; 20-39 ; 711 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L127 | Upper leg length (cm) ; M ; 20-39 ; 701 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L128 | Standing height (cm) ; M ; 40-59 ; 771 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L129 | Weight (kg) ; M ; 40-59 ; 773 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L130 | BMI (kg/m²) ; M ; 40-59 ; 770 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L131 | Waist circumference (cm) ; M ; 40-59 ; 747 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L132 | Hip circumference (cm) ; M ; 40-59 ; 747 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L133 | Mid-upper-arm circumference (cm) ; M ; 40-59 ; 752 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L134 | Upper arm length (cm) ; M ; 40-59 ; 752 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L135 | Upper leg length (cm) ; M ; 40-59 ; 744 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L136 | Standing height (cm) ; M ; 60-80 ; 998 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L137 | Weight (kg) ; M ; 60-80 ; 999 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L138 | BMI (kg/m²) ; M ; 60-80 ; 994 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L139 | Waist circumference (cm) ; M ; 60-80 ; 953 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L140 | Hip circumference (cm) ; M ; 60-80 ; 953 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L141 | Mid-upper-arm circumference (cm) ; M ; 60-80 ; 968 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L142 | Upper arm length (cm) ; M ; 60-80 ; 969 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L143 | Upper leg length (cm) ; M ; 60-80 ; 947 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L144 | Standing height (cm) ; M ; 20-80 ; 2500 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L145 | Weight (kg) ; M ; 20-80 ; 2501 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L146 | BMI (kg/m²) ; M ; 20-80 ; 2493 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L147 | Waist circumference (cm) ; M ; 20-80 ; 2409 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L148 | Hip circumference (cm) ; M ; 20-80 ; 2410 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L149 | Mid-upper-arm circumference (cm) ; M ; 20-80 ; 2431 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L150 | Upper arm length (cm) ; M ; 20-80 ; 2432 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L151 | Upper leg length (cm) ; M ; 20-80 ; 2392 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L152 | Standing height (cm) ; F ; 20-39 ; 845 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L153 | Weight (kg) ; F ; 20-39 ; 846 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L154 | BMI (kg/m²) ; F ; 20-39 ; 845 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L155 | Waist circumference (cm) ; F ; 20-39 ; 798 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L156 | Hip circumference (cm) ; F ; 20-39 ; 802 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L157 | Mid-upper-arm circumference (cm) ; F ; 20-39 ; 809 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L158 | Upper arm length (cm) ; F ; 20-39 ; 809 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L159 | Upper leg length (cm) ; F ; 20-39 ; 803 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L160 | Standing height (cm) ; F ; 40-59 ; 869 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L161 | Weight (kg) ; F ; 40-59 ; 868 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L162 | BMI (kg/m²) ; F ; 40-59 ; 868 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L163 | Waist circumference (cm) ; F ; 40-59 ; 834 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L164 | Hip circumference (cm) ; F ; 40-59 ; 839 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L165 | Mid-upper-arm circumference (cm) ; F ; 40-59 ; 845 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L166 | Upper arm length (cm) ; F ; 40-59 ; 845 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L167 | Upper leg length (cm) ; F ; 40-59 ; 827 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L168 | Standing height (cm) ; F ; 60-80 ; 971 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L169 | Weight (kg) ; F ; 60-80 ; 970 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L170 | BMI (kg/m²) ; F ; 60-80 ; 969 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L171 | Waist circumference (cm) ; F ; 60-80 ; 896 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L172 | Hip circumference (cm) ; F ; 60-80 ; 898 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L173 | Mid-upper-arm circumference (cm) ; F ; 60-80 ; 913 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L174 | Upper arm length (cm) ; F ; 60-80 ; 914 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L175 | Upper leg length (cm) ; F ; 60-80 ; 894 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L176 | Standing height (cm) ; F ; 20-80 ; 2685 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L177 | Weight (kg) ; F ; 20-80 ; 2684 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L178 | BMI (kg/m²) ; F ; 20-80 ; 2682 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L179 | Waist circumference (cm) ; F ; 20-80 ; 2528 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L180 | Hip circumference (cm) ; F ; 20-80 ; 2539 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L181 | Mid-upper-arm circumference (cm) ; F ; 20-80 ; 2567 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L182 | Upper arm length (cm) ; F ; 20-80 ; 2568 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L183 | Upper leg length (cm) ; F ; 20-80 ; 2524 | VERIFIED | Reproduced from NHANES 2017–18, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L193 | BMXWAIST ; M ; 2401 ; 0.91 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L194 | BMXHIP ; M ; 2402 ; 0.94 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L195 | BMXARMC ; M ; 2420 ; 0.88 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L196 | BMXARML ; M ; 2421 ; 0.49 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L197 | BMXLEG ; M ; 2385 ; 0.27 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L198 | BMXWAIST ; F ; 2527 ; 0.91 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L199 | BMXHIP ; F ; 2538 ; 0.95 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L200 | BMXARMC ; F ; 2566 ; 0.91 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L201 | BMXARML ; F ; 2567 ; 0.53 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |
| L202 | BMXLEG ; F ; 2523 ; 0.10 | VERIFIED | Reproduced, weighted; matches. | https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2017/DataFiles/BMX_J.xpt |

## Source table

| Key | URL | Resolves (y/n) | Licence statement correct (y/n) | Note |
|---|---|---|---|---|
| Gordon 2014 (ANSUR II report) | https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf | y (206) | y | "Approved for public release; distribution is unlimited" confirmed on the report. |
| ANSUR II public CSVs | https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv ; …MALE_Public.csv | y (206) | y | Downloaded to /workspace/tmp, recomputed, then deleted. CSVs = unweighted working sample, F 1986 / M 4082, ages 17–58. |
| OpenLab host page | https://www.openlab.psu.edu/ansur2/ | y (200) | n/a |  |
| NHANES 2017–18 BMX_J / DEMO_J | wwwn.cdc.gov links in file | y | y | US Gov; data-use terms stated correctly. |
| Fryar et al. 2021, Vital Health Stat 3(46) | https://www.cdc.gov/nchs/data/series/sr_03/sr03-046-508.pdf | n live (403 Akamai); y via http://web.archive.org/web/20261109140447/https://www.cdc.gov/nchs/data/series/sr_03/sr03-046-508.pdf | y | "All material appearing in this report is in the public domain…" confirmed in the archived PDF. |
