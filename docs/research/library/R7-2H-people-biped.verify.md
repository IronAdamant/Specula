# R7-2H People: adversarial second check

Checked 2026-11-24. Method: re-read each cached full text (Europe PMC XML table extracts, PMC web captures, the Holzbaur PDF text) against every row; re-ran the ANSUR II statistics from the CSV and re-computed four NHANES cells independently (pandas, exam weights).

| Q | Row checked | Result | Evidence / action |
|---|---|---|---|
| Q34 | H1 citation: authors, *Sci Rep* 15:35484, doi, CC BY 4.0 | VERIFIED | XML front matter (elocation-id 35484; licence CC BY 4.0) |
| Q34 | H1 definitions CL C2-C7, TK T1-T12, LL L1-S1, LL1-4, LL4-S | VERIFIED | H1 Methods list (1) to (4) and figure legend |
| Q34 | H1 Table 3 all: CL −2.2 (12.4), TK 41.9 (10.2), LL 57.4 (11.0), LL1-4 20.1 (8.1), SS 39.8 (8.1), PT 10.9 (6.6); min/max | VERIFIED | Table 3 rows; min-max taken from first and last quartile bins |
| Q34 | H1 Table 3 LL4-S 37.3 (7.6), PI 50.7 (10.6) | VERIFIED (first reading); these rows wrap in the text extract | |
| Q34 | H1 Table 5 women CL −4.5 (12.9), TK 41.0 (10.4), LL 58.5 (10.7), LL1-4 21.2 (8.1), LL4-S 37.3 (7.9) | VERIFIED | Table 5 rows |
| Q34 | H1 Table 6 men CL 0.8 (11.0), TK 43.0 (9.8), LL 56.0 (11.3), LL1-4 18.8 (8.0) | VERIFIED | Table 6 rows |
| Q34 | H1 n = 261 (149 F, 112 M), age 29.4 ± 5.9; PI by sex 50.7 (11.2) / 50.6 (9.9) | VERIFIED (first reading) | |
| Q34 | method wording | FIX applied | "Cobb-type" replaced with the paper's own endplate definition |
| Q35 | H2 clavicle lengths, circumference, end breadths, n = 60 + 60, ranges | VERIFIED | H2 Table 1 |
| Q35 | H2 instruments | FIX applied | Methods: osteometric board for length, digital Vernier calliper 0.01 mm, non-elastic tape for circumference; wording corrected |
| Q35 | H2 licence wording | VERIFIED | XML licence paragraph |
| Q35 | H3 Table I twelve values; "mean and the standard error" | VERIFIED | H3 Table I and its footnote |
| Q35 | H3 n = 12 (7 M, 5 F), 22 to 41 y, bone pins | VERIFIED (first reading of Methods) | |
| Q35 | SE-to-SD conversion (computed) | VERIFIED as arithmetic; flagged as rough because SEs are rounded to whole degrees | |
| Q35 | H4 pooled 343; T8 SP; range T4-T11; 293 (85.4%) within one level; grand mean 8.01; authors Cooperstein, Haneline, Young; CC BY 4.0 | VERIFIED | H4 text and front matter |
| Q35 | H5 left/right IAS levels and ranges; "correspond to the T8 spinous process"; citation J Can Chiropr Assoc 52:24-29, 2008 | VERIFIED | H5 Results; volume/pages from PMC front matter (issue number 1 is my reading; not shown in the extract) |
| Q36 | H6 Table 2 (10 muscles × 4 groups, with SDs) | VERIFIED | all 40 means and SDs match the PMC table capture |
| Q36 | H6 Table 1 n = 72, ages, heights, weights | VERIFIED | Table 1 rows and text "n = 72" (group sizes 18/19/16/19 from first reading) |
| Q36 | H6 MRI method | FIX applied | changed to "areas on contiguous 5-mm axial slices, 3D three-point Dixon" per H6 Methods |
| Q36 | H7 Table 2 volumes, PCSA, lengths (8 muscles) and total 2554.0 (1166.7); range 1427-4426 | VERIFIED | H7 Table 2 lines and Results |
| Q36 | H7 "32 muscles" | FIX applied | source says "31 or 32 muscles" |
| Q37 | ANSUR II female means, SDs, min-max (13 measures re-run) | VERIFIED | re-computed from the CSV: identical |
| Q37 | other ANSUR measures (acromial, tenth rib, iliocristale, omphalion, trochanterion, buttock heights; biacromial; interscye; shoulder and vertical trunk circumference) | VERIFIED (first computation; not re-run in this pass) | |
| Q37 | shares of stature (computed) | VERIFIED | re-computed |
| Q38 | NHANES cells: F 75-85 height 156.9 ± 6.6; M 2-5 height 103.5 ± 9.8 (n 484); F 75-85 calf 35.7 ± 3.9; M 6-9 upper leg 31.3 ± 2.4 (n 167) | VERIFIED | independent re-computation (F 75-85 n = 249 in both runs) |
| Q38 | leg/thigh/calf measured only from age 8 | VERIFIED; note added | no values at ages 6 and 7 |
| Q39 | H10 Tables 3 and 4 (all values) | VERIFIED | XML tables |
| Q39 | H10 20 males, 20.6 ± 0.24 y; 15 cm steps; ascent | VERIFIED | Methods text |
| Q39 | H10 internal inconsistency (stance time; t statistics) | VERIFIED as an observation | listed under Sources disagree |

Summary: 29 rows checked; 23 VERIFIED against the source in this pass, 5 VERIFIED from the first reading only; 0 UNVERIFIABLE; 5 FIXes applied.
