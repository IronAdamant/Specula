# R7-5 verification (adversarial re-check)

Checked 2026-11-24 against the files in `sources/`. Method: every number in R7-5 was searched for in the saved source text (grep for the exact string), or recomputed from the spreadsheet / docx with a script. Where a number came from a figure or a scanned page, the page image was opened and read.

## Results by source

| Source | What was checked | Result |
|---|---|---|
| X1 Biancardi 2011 | 4.55 Hz, 0.275 m/s, f = 14.4 s + 0.14, d = -0.531 s + 0.666, 18.5 ± 3.7 g, 11 cm/s cut-off (Froude 0.08), Table 2 duty factors, 6.14 ± 0.71 cm | all found in the saved text |
| X2 Bender 2011 | Froude 0.4 / 0.5; "above 7 Hz (equivalent to 15-20 cm/s)"; preferred speeds | found; the text says the tripod gait is "above 7 Hz", so "6 to 7 Hz" in R7-5 is the transition zone from the abstract, not a measured point |
| X3 McElroy 2008 | Table 1, all eight species rows quoted | all match; footnote says means ± s.e.m. (R7-5 corrected to s.e.m.) |
| X4 Gardiner 2011 | Table 1 values; Desmodus 6.51 x BW, 2.38 m/s | found |
| X5 old JEB abstracts | Full & Tu 27 Hz, 0.44-1.5 m/s; Fieler & Jayne ranges | abstract text only; body text NOT checked (scans) |
| X6 Peng 2025 | RMSE 3.57-4.18°, 124 angles, height 16.45 m; searched for "30.7" and "54.1" | RMSE etc. found; 30.7 and 54.1 NOT present: the brief's beech means are not from this paper |
| X7 Kuuluvainen 1988 | Table 4 whorl means 57.0, 74.2, 89.6, 93.5, 81.9; VAR 629.5; mean 77.3 | found in the OCR text; OCR was compared with the page image for Table 4. SD values are computed (square root of the printed variance) |
| X8 Volf 2023 | group means; ratios 1.092 and 1.187 | found; an earlier draft line giving a hare femur range was removed because the scraped columns were mixed up |
| X9 Schopf 2010 | measures 1, 2, 16, 17, 20 for all seven skulls | found (comma decimals, e.g. "105,20", "62,70") |
| X10 Chapman 2016 data | adult means, SD, ranges, n | recomputed from PhotoDRYAD.xlsx sheet "Body paramters": identical to R7-5. Sex coding is INFERRED (no codebook in the file) |
| X11 Di Santo data | per-species table | generated directly from MegakineMorpho.xlsx by script (work/alt/tables) |
| X12 Böhmer 2019 | Table S3 group counts (Aves 103 spp., median 13, 10-23; Palaeognathae 16, 14-18; Galloanseriformes 16, 14-23; Australaves 11, 10-13); Table S4 logs | found; back-transform is computed |
| X13 Humphries 2020 | Table S5, all rows quoted; Table S3 stance and swing | all match the docx tables exactly |
| X15 Baliga 2019 | goshawk humerus 99.19, radius-ulna 121.4, carpometacarpus 51.91 mm | match Data S1 |
| X17 Magro 2026 | all six means and SD | found |
| X18 Castillo-Figueroa 2020 | Desmodus, Molossus, Myotis, Saccopteryx rows; Artibeus 68.38 | found |
| X19 Herrel 2002 | Callisaurus, Petrosaurus, Sator rows | found in the text, but the text layer splits digits ("1 1.13" = 11.13; "4 2.08" = 42.08). I re-joined them by matching the column layout. Treat as WEAK until read off the PDF page |
| X20 Dempster 1955 | 56.5, 15.7, 43.6, 50.6 and the Table 10 percentages | found in the archive.org text |
| X22 Perovich 1996 | 0.05, 0.5-0.7, 0.75-0.85, 0.05-0.9 | found (p. 3 "Bulk albedo" text) |
| X24 Meijaard 2007 (Russian translation) | w 1.02, c 0.08, π/10, all masses and inertias, v_w 4.29238253634111, v_c 6.02426201538837, v_d 0.68428307889246 | found in the saved text (Table 1 lines 173-226, Table 2 line 699) |
| X23 SPM 1984 | eq. 3-33, 3-34, 3-33a | the OCR text garbles the exponents and "5.112"; I read all three equations off the page-image of p. 3-44 instead. Confirmed there |

## Weak spots and disagreements (flagged)
1. **Herrel 2002 OCR**: digit splitting in the text layer. Values look right by column logic and sum checks (e.g. Callisaurus femur + tibia + metatarsus + toe = 76.27 mm, the printed hind limb length), but read off the PDF before relying on them.
2. **Böhmer log base**: the paper's methods say natural log; the numbers only give plausible bird sizes as log10 of metres (pigeon femur 56 mm with log10 against 286 mm with ln, and ostrich 316 mm against 606 mm; real pigeon femora are about 50 mm). log10 used.
3. **Castillo Artibeus lituratus**: forearm and digit III both 68.38 mm, almost surely a misprint. Excluded.
4. **Chapman sex coding**: inferred, not stated in the data file.
5. **Di Santo**: the paper's own text values (eel 0.58 BL) are for a speed subset; R7-5 gives all-clip means. Both are real; they measure slightly different things.
6. **Peng 2025** does not hold the 30.7° and 54.1° beech means. These stay unverified (wrong attribution in the brief).
7. **Substitutes** (X8 for A37, X11 for A8, X12 for A6's figshare, X21, X22, X23) measure the same quantity but are not the cited source. They are labelled "Substitute" in R7-5.
8. **Humphries dog lengths** are marker-to-marker on skin, not bone lengths.

## Verdict
All quoted numbers were found in the sources or recomputed from the data files, except the Herrel table (WEAK, OCR) and the three SPM equations (checked on the page image, not the text). One wrong line (the Volf hare range) and one wrong label (McElroy SE vs s.e.m.) were fixed. One wrong claim about the Chapman paper's sex counts was replaced with the evidence actually in the paper.
