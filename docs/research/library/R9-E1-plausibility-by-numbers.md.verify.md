# Verify: R9-E1-plausibility-by-numbers.md

Re-check done 2026-11-28 against the box copies in raw/round9 (iso20685, bsp22, hakamada12, wuhrer11, asym25, fujiwara10), page-marked text plus page images for S3 Tables 1, 5, 6 and S5 Figure 1. Citations checked against Crossref (S2, S3, S4 journal version, S5, S6) and OpenAlex (S6). Licences checked on the publisher pages named in the file. A regex check confirmed every cited page in the table below.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | S1 title, © ISO 2018, iTeh catalogue URL | iso20685 PDF p.1-2, page footers | VERIFIED; preview PDF link not logged (stated in file) |
| 2 | "True value" = experienced ISO 7250-1 anthropometrist with tape and calliper | S1 3.5, PDF p.8; 4.2, PDF p.11 | VERIFIED |
| 3 | Body cannot be held to machine-tool accuracy | S1 PDF p.11 | VERIFIED |
| 4 | Whole-body scanners may lack resolution for hand; Tables 1-3 by scanner | S1 PDF p.9 | VERIFIED |
| 5 | Table 4 values 5/4/9/4/4/5/1/2/1/2 mm, "maximum mean difference", see 5.4 | S1 PDF p.11 | VERIFIED |
| 6 | Below Table 4 may go into ISO 15535 databases | S1 PDF p.11 | VERIFIED |
| 7 | Clause 5 not in preview | S1 PDF p.11 ends at 5.1 | VERIFIED |
| 8 | S2 citation, CC BY 4.0 | Crossref; PDF p.1 | VERIFIED |
| 9 | 21 participants, 10 M 11 F, Table 1 | S2 PDF p.1, p.4 | VERIFIED |
| 10 | 16 segments, volume × uniform literature density, geometric centre as COM | S2 PDF p.6-7 | VERIFIED |
| 11 | Beam test +4.2% volume, -0.6% length, -0.3% pCOM | S2 PDF p.9 | VERIFIED |
| 12 | CV 1.8% / 1.6%, ICC 0.96 / 0.99 | S2 PDF p.11 | VERIFIED |
| 13 | Mass means and p-values both sexes | S2 PDF p.11 | VERIFIED |
| 14 | Bland-Altman limits and biases | S2 PDF p.11 | VERIFIED |
| 15 | Sign inconsistency (positive = overestimate vs negative bias with higher means) | S2 PDF p.10-11 | VERIFIED as an inconsistency; cause UNVERIFIABLE (stated in file) |
| 16 | 1000 kg/m³ run gives the same conclusions | S2 PDF p.10, p.12 | VERIFIED |
| 17 | Limits: 30-40 min, larger segments most reliable, 0.5 m, smoothing, face occlusion, breasts | S2 PDF p.12, p.18 | FIX applied ("small segments least reliable" reworded to S2's own heading) |
| 18 | S3 citation, pages 64-70, CC BY 4.0 | Crossref; proceedings abstract page | VERIFIED |
| 19 | 6 men, age/height/mass | S3 PDF p.1 | VERIFIED |
| 20 | Scanner spacing, points, time; 14 segments; 72 landmarks | S3 PDF p.2 | VERIFIED |
| 21 | Clauser densities 1.070 ... 1.084 kg/L | S3 Table 1, PDF p.3 (page image) | VERIFIED |
| 22 | Uniform density gives centre of volume | S3 PDF p.3 | VERIFIED |
| 23 | Mass within 1.5% | S3 PDF p.1 | VERIFIED |
| 24 | Trunk lighter, thigh heavier; trunk centre about 6% lower | S3 PDF p.5 | VERIFIED |
| 25 | Reaction board 180 × 91.5 × 2.5 cm on p.4 | S3 | FIX applied: dimensions are on PDF p.1 only; "a scale" (p.1) vs two scales (p.4) noted |
| 26 | Tables 5-6 COG values, p < 0.001 | S3 PDF p.6 (page image) | VERIFIED |
| 27 | "Within 2.9%" is a mean; per-subject 2.1-4.4% | S3 PDF p.1, p.7 vs Table 5 PDF p.6 | VERIFIED (my reading of the table) |
| 28 | Error causes | S3 PDF p.6 | VERIFIED |
| 29 | S4 arXiv and journal citation; arXiv licence non-exclusive | arXiv abs page; Crossref | VERIFIED |
| 30 | Two-standard-deviation remark | S4 PDF p.2 | VERIFIED |
| 31 | Clamp to l SD, Gaussian assumption | S4 PDF p.7 | VERIFIED |
| 32 | Two optimisation steps, smoothness, no prior | S4 PDF p.5-6 | VERIFIED |
| 33 | l = 10 real, 3 synthetic, λ = 0.1 | S4 PDF p.10 | VERIFIED |
| 34 | S_close and S_k, k = 2, 4, 200 samples | S4 PDF p.11 | VERIFIED |
| 35 | Table 2 values; 360 bodies, 34 measurements | S4 PDF p.13-14 | VERIFIED |
| 36 | Feature analysis outside human space S4 / S2; face sensitivity | S4 PDF p.12, p.14 | VERIFIED |
| 37 | Small-training-set experiment | S4 PDF p.16-17 | FIX applied: added l = 3 and the stated failure (one far body predicted too small) |
| 38 | S5 citation, CC BY 4.0 | Crossref; journal abstract page | VERIFIED |
| 39 | 245 people, ages 6-75, 22 pairs, 16 auto + 6 manual, two researchers | S5 PDF p.1, p.4-5 | VERIFIED |
| 40 | No significant mean difference; fluctuating asymmetry | S5 PDF p.7 | VERIFIED |
| 41 | p-values wrist 0.09 ... shoulder slope 0.27; upper more than lower | S5 PDF p.7 | VERIFIED |
| 42 | Direction 8 of 14 upper left | S5 PDF p.7 | FIX applied: added 5 of 8 lower right, and the contrast with earlier work |
| 43 | Figure 1 axis unit not found | S5 PDF p.8 (page image) | UNVERIFIABLE (stated in file) |
| 44 | Cyclist figures (PMC8615123) | search summary only | UNVERIFIED, marked not used |
| 45 | S6 citation; free on J-STAGE; no licence | Crossref; OpenAlex | VERIFIED |
| 46 | 36 subjects, 15 M 21 F, 164.8 cm, foot 24.1 cm | S6 PDF p.2 | VERIFIED |
| 47 | QS 44.5 %FL SD 7.7, range 31.4-57.7; forward 82.4; back 18.6 | S6 PDF p.4 | VERIFIED |
| 48 | 30 and 60 %FL as change points; stable range margins | S6 PDF p.2, p.6 | FIX applied: p.6 wording made exact (back end about 30, margins 10 and 20 %FL) |
| 49 | COP equals COM projection on average in still standing | none | Labelled (mine) |
| 50 | Section 6 check list | none | Labelled (mine) |

Fixes: rows 17, 25, 37, 42, 48.
WEAK: row 15 (cause of the sign mismatch), row 43 (asymmetry magnitudes), row 1 (preview PDF link not logged).
Em-dash count in the file: 0.
