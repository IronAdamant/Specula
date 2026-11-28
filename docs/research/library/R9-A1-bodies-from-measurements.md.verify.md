# Second check: R9-A1-bodies-from-measurements.md

Independent re-check, 2026-11-28.
- **Page cites.** Each was re-found by grep in freshly page-marked text (`=== PDF p.N ===`, one pdftotext call per page) of allen03.pdf, seo03thesis.pdf, scape05.pdf, smpl15.pdf and pishchulin17.pdf.
- **Bibliography.** Citations were re-checked against Crossref (volume, issue, pages).
- **Plots and figures.** Plot readings were re-checked on page images (SMPL PDF p.8 and p.10; Pishchulin PDF p.17 and p.20; Seo PDF p.90).
- **Licences.** The Seo licence was re-checked in OpenAlex and on the PDF cover. The arXiv licence was checked on the abs page.

| row | claim | source location | result |
|---|---|---|---|
| 1 | Allen TOG 22(3) 587-594; SMPL TOG 34(6); SCAPE TOG 24(3) 408-416; Pishchulin PR 67 276-286; Seo I3D 19-26 | Crossref API | VERIFIED |
| 2 | Seo I3D paper has no free copy | OpenAlex is_oa false; Semantic Scholar CLOSED; ACM bot page; CiteSeerX fail | VERIFIED as a search result; absence can't be proven exhaustively: WEAK |
| 3 | Seo thesis licence: CC BY (OpenAlex) against "protected by copyright" (cover) | OpenAlex record; seo03thesis PDF p.1 | VERIFIED (both stated as disagreeing) |
| 4 | 125 male + 125 female CAESAR scans | allen03 p.2 | VERIFIED |
| 5 | Compatible points: normals within 90°, 10 cm | allen03 p.3 | VERIFIED |
| 6 | L-BFGS-B, two resolutions, four weight stages | allen03 p.4 | VERIFIED |
| 7 | Hole weight 0, confidence clamp, ears/fingers marked | allen03 p.4-5 | VERIFIED |
| 8 | PCA separately by sex | allen03 p.6 | VERIFIED |
| 9 | Linear feature map, pseudoinverse, cube root of weight | allen03 p.6 | VERIFIED |
| 10 | Marker-only fit with 74 markers | allen03 p.6-7 | VERIFIED |
| 11 | Local minima (left arm onto right); zero-scale trivial minimum | allen03 p.4 | VERIFIED |
| 12 | Pose leaked into PCA; lips misaligned in under 5%; single Gaussian | allen03 p.7 | VERIFIED |
| 13 | Skeleton transfer imprecise because of body thickness | allen03 p.7 | VERIFIED |
| 14 | Allen gives no % variance or mm error | full text grep for "%" and "mm" | VERIFIED (only the 10 cm and 5% numbers appear) |
| 15 | 120 scans, 60 per sex, Tecmath, 75,000 triangles | seo03thesis p.57 | VERIFIED |
| 16 | Eight measurements: 5 girths + 3 lengths; list in Fig. 5-2 | seo03thesis p.74 | VERIFIED |
| 17 | 33 joints x 6 DoF, 841 vertices; 30 PCs | seo03thesis p.73, p.77 | VERIFIED |
| 18 | No linear term in displacement interpolators; absolute joint parameters | seo03thesis p.77 | VERIFIED |
| 19 | Normalisation via PCA without truncation; stature dominates; neck-hip correlation | seo03thesis p.78 | VERIFIED |
| 20 | Valid measurement volume; clamp to nearest point or reuse last valid input | seo03thesis p.79-80 | VERIFIED |
| 21 | 270 interpolators; n^3 LU | seo03thesis p.88 (draft said p.89) | **FIX** applied |
| 22 | Cross-validation 0.980 / 0.623 cm, and per-girth values | seo03thesis p.90 (table image checked; text extraction scrambles the columns) | VERIFIED |
| 23 | Limits: sensitivity to input (p.79); out-of-range demos (p.83); skinning dependence (p.120); 10-20 rule (p.119) | seo03thesis | VERIFIED |
| 24 | SCAPE: 37 people at p.3 against 45 instances at p.5 | scape05 p.3, p.5 | VERIFIED (difference stated as unexplained) |
| 25 | 18 parts reduced to 16; regression on two nearest joints; overfitting on 3-DoF joints | scape05 p.3-4 | VERIFIED |
| 26 | PC list: weight and height, gender, abdominal fat/chest muscle, chest against hips bulk | scape05 p.5 | VERIFIED |
| 27 | SCAPE limits F3-1 to F3-5; armpit artefact | scape05 p.7-8, p.4 | VERIFIED |
| 28 | SMPL 6890 vertices, 23 joints, 72 pose numbers, 207 pose blend shapes, 4 influences | smpl15 p.4-5 | VERIFIED |
| 29 | Pose normalisation before PCA; vertex PCA argued over triangle PCA | smpl15 p.8 | VERIFIED |
| 30 | Data: 1700 M / 2100 F CAESAR; 1786 registrations of 40 people | smpl15 p.6 | VERIFIED |
| 31 | Fig. 9: about 0.9 by 3-5 coefficients | smpl15 p.8 image | VERIFIED as a plot reading (labelled read off plot) |
| 32 | PC1 size/stature, PC2 heavy to slim | smpl15 p.8 Fig. 8 image | WEAK: my reading of a figure; the paper does not name the components (said so in the file) |
| 33 | Fig. 11: about 6.5-7 mm falling to about 3.2 mm at 300; Fig. 12 about 4.6-7 mm | smpl15 p.10 image | VERIFIED as a plot reading |
| 34 | SMPL limits (shape-independent pose correctives, no dynamics, sparse-variant seams) | smpl15 p.10, p.12-14 | VERIFIED |
| 35 | S-SCAPE: 15 bones, 30 pose parameters; 10,000 samples; 1,000 samples enough | pishchulin17 p.7, p.16, p.18 | VERIFIED |
| 36 | Pishchulin generalisation about 15-20 mm at 1 PC to about 4-5 mm at 30 | pishchulin17 p.17 image | WEAK: small plot with error bars; order of magnitude reliable |
| 37 | Pishchulin component I is size, II is slim to heavy | pishchulin17 p.20 image | WEAK: figure reading (labelled) |
| 38 | "Which numbers matter" ordering | synthesis | Labelled (mine); rests on rows 26, 32 and 37 |

Fixes: row 21 (p.89 corrected to p.88).
WEAK: rows 2, 32, 36 and 37 (search-based absence; figure and plot readings), all labelled in the file.
