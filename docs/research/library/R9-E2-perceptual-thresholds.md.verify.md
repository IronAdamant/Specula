# Verify: R9-E2-perceptual-thresholds.md

Re-check done 2026-11-28 against the box copies harrison04.pdf (page-marked text), thaler18.pdf (page-marked text) and green08.webfetch.txt (WebFetch text, cited by section and table). Citations checked with Crossref (S1 10.1145/1186562.1015761; S2 10.1016/j.chb.2008.02.019; S3 10.3389/fict.2018.00018, with CC BY 4.0 in Crossref). The S3 landing page returned HTTP 200; the S2 author URL returned HTTP 500 from the box. Every cited page below was matched by a string check.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | S1 citation, pp. 569-573 | Crossref | VERIFIED |
| 2 | S1 author copy at UBC, © ACM, no open licence | URL used for download; no licence in PDF | VERIFIED (copyright assumed from venue; no notice in the text) |
| 3 | Stimulus: two links about 5°, 0.3-0.7 rev/s, 2AFC | S1 PDF p.1-2 | VERIFIED |
| 4 | Thresholds are Weber fractions; lower = more sensitive | S1 PDF p.1 footnote 1, p.2 | VERIFIED |
| 5 | Static-line Weber fraction 2.9% | S1 PDF p.2 | VERIFIED |
| 6 | Figure 2 values 1A-4D with standard errors | S1 PDF p.3 | VERIFIED |
| 7 | Speed values 9.2 / 10.5 / 10.7% | S1 PDF p.2 | VERIFIED |
| 8 | Replication 8.0% against 18.4%, n = 46; three observers the other way | S1 PDF p.2 footnote 3 | VERIFIED |
| 9 | 2.7% close to static lines | S1 PDF p.2 | VERIFIED |
| 10 | Seven guidelines | S1 PDF p.3 | VERIFIED |
| 11 | Rendered character may hide changes (not measured) | S1 PDF p.3 | VERIFIED |
| 12 | 75% against 80% criterion mismatch | S1 PDF p.2 | VERIFIED as stated in the paper |
| 13 | Change over time, not between figures | none | Labelled (mine) |
| 14 | S2 citation, © Elsevier | Crossref; text header | VERIFIED |
| 15 | S2 author copy now HTTP 500 | curl header check 2026-11-28 | VERIFIED |
| 16 | 208 participants, 126 / 81, age 31.9, 17-79 | S2 §2.1 | FIX applied: noted the counts sum to 207 and n = 194 for cheek width |
| 17 | 11 figures; four ratios defined; ±10% in 1% frames, 21 frames | S2 §2.2, §3.1 | VERIFIED |
| 18 | Tasks; tolerance = mean range; sensitivity = SD of best point | S2 §2.3, §3.1 | VERIFIED |
| 19 | Table 2 tolerance values for human figures | S2 Table 2 | VERIFIED |
| 20 | Robot ranges 0.026-0.058 / 0.035-0.047 / 0.032-0.059 / 0.114-0.168 | S2 Table 2 (recomputed) | VERIFIED |
| 21 | Human stricter than robot; realistic stricter than 2D | S2 §3.1 | VERIFIED |
| 22 | Per-cent conversions 4%, 6%, 7% | S2 Tables 1-2 | Labelled (mine); arithmetic rechecked |
| 23 | H1 not supported; H2 supported except eye separation; H3 supported | S2 §4.1-4.2 | VERIFIED |
| 24 | Best points: narrower cheeks and jaws, longer faces; eyes differ human/robot; max shift 6 frames 3D female jaw | S2 §4.3, Table 9 | VERIFIED |
| 25 | Female participants more tolerant, p < 0.01, small effects | S2 §3.6, §4.5 | VERIFIED |
| 26 | Creepiness highest at neutral humanlike rating | S2 §4.4, Fig. 6 caption | VERIFIED |
| 27 | Text 0.047 against Table 2 0.095 (3D female jaw) | S2 §3.1 vs Table 2 | VERIFIED as a mismatch; which is right is UNVERIFIABLE |
| 28 | S3 citation, CC BY 4.0 | Crossref; S3 PDF p.14 | VERIFIED |
| 29 | Registered to a statistical model from 2,094 female bodies; average body from CAESAR | S3 PDF p.3-4 | VERIFIED |
| 30 | 26 participants, 13 + 13, ages 20-37 | S3 Table 1, PDF p.3 | VERIFIED |
| 31 | Nine avatars 0, ±5 ... ±20% BMI; height, arm length, inseam fixed; Weber reasoning | S3 PDF p.4 | VERIFIED |
| 32 | 180 trials; 0.05% adjustment steps | S3 PDF p.4-5 | VERIFIED |
| 33 | 1AFC means −4.42 / −1.54 / −3.17 / −2.78% and significance | S3 PDF p.7 | VERIFIED |
| 34 | Adjustment means −3.09 / +0.36 / −2.16 / −0.57% | S3 PDF p.7 | VERIFIED |
| 35 | Weibull fits, R² 0.996, 13 of 208 unfit, β 6.92 vs 13.91, p = 0.02 | S3 PDF p.6, p.8 | VERIFIED |
| 36 | Ideal weight −8.27% women, −1.62% men | S3 PDF p.8 | VERIFIED |
| 37 | Uncanniness percentages, four conditions | S3 PDF p.11 | VERIFIED |
| 38 | No per-cent threshold derivable from text | S3 PDF p.6-8 | VERIFIED (location parameters not listed in text) |
| 39 | "Small sample of young adults" | none | FIX applied: relabelled as mine, with ages from Table 1 |
| 40 | Piryankova 2014 and the step-length paper | search summaries only | UNVERIFIED, marked not read and not used |
| 41 | Section 5 tolerances | none | Labelled (mine) |

Fixes: rows 16, 39.
WEAK: row 2 (copyright of the author copy assumed), row 15 (S2 copy no longer reachable; read via WebFetch), row 27 (which value is right).
Em-dash count in the file: 0.
