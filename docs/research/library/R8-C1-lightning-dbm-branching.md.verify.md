# Second check: R8-C1-lightning-dbm-branching.md

Each claim was re-read against the raw text (raw/round8/*.flow.txt; sanudo95-npg.ocr.txt for S1) with a page-locating script; formulas were checked against page images where the text layer was garbled (S3 p.3; L3 p.9).

| row | claim | source page | result |
|---|---|---|---|
| 1 | L1, L2, L3, S1, S3, S4 citations (DOIs, volumes, pages) | Crossref API | VERIFIED |
| 2 | S2 venue not printed, not on Crossref | bickel06 PDF 1; Crossref search returned an unrelated paper | VERIFIED (venue left unstated) |
| 3 | S1 licence CC BY-NC-SA 2.5 on article page; PDF says © EGS 1995 | Copernicus page; S1 PDF 1 | VERIFIED |
| 4 | DBM steps: phi = 0 pattern, phi = 1 electrode, Laplace solve, candidates, p_i = phi^eta / sum | L3 PDF 3 Eqs. 1-2; S2 PDF 3 Eq. 1; S1 PDF 2 Eq. 2 | VERIFIED |
| 5 | L3 lightning set-up (seed at top, ground phi = 1) | L3 PDF 3 Fig. 2b and text | VERIFIED |
| 6 | S1 box set-up (top phi = 0, bottom phi = 1, periodic sides, centre start) | S1 PDF 1 Sec. 2 | VERIFIED |
| 7 | Interval-of-[0,1] sampling | S2 PDF 3 | VERIFIED |
| 8 | Fractal dimension approximately 1.7 citing L1 | L3 PDF 2; S2 PDF 3 | VERIFIED |
| 9 | eta = 1 dense; Hastings eta = 4 non-fractal; range about 1-4 | L3 PDF 10 Sec. 8 | VERIFIED |
| 10 | S1 Table 1a/2a values | S1 PDF 4 (OCR; values cross-checked against the text's statement D_p 1.34 at eta 6, D 1.51) | VERIFIED (OCR) |
| 11 | Photo D_p about 1.34; eta about 6; "considerably more deterministic" | S1 PDF 1, PDF 5 | VERIFIED |
| 12 | R_g ~ N^beta, D = 1/beta; small-lattice check | S1 PDF 4 | VERIFIED (OCR shows "1/8" for 1/beta; read as beta from context) |
| 13 | SOR omega about 1.6, about 15 iterations, below 0.001; omega = 1 needs 2-3x | S1 PDF 2 | VERIFIED |
| 14 | 5.5-10 CPU hours, 900-1600 points, eta = 2 | S1 PDF 2 | VERIFIED |
| 15 | Octree 22x at 128^3, 150x at 256^3 | L3 Fig. 12 caption | FIX: page was PDF 8, corrected to PDF 7 |
| 16 | Adaptive solver no better with complex boundaries; L3 stated limits | L3 Sec. 10 | FIX: page was PDF 10, corrected to PDF 11 (two places) |
| 17 | Ascending-order summation rounding bug | S1 PDF 2 | VERIFIED |
| 18 | Jitter fix | L3 PDF 9-10 Sec. 7 | VERIFIED |
| 19 | Poisson dart leader, rho cleared and re-laid; rho = 1000/(4 pi), 512^2, eta = 1 | L3 PDF 4 Sec. 4.2; PDF 10 Sec. 9 | VERIFIED |
| 20 | Repel by phi = 0 obstacle | L3 PDF 10 Sec. 8 | VERIFIED |
| 21 | Glassner timing: 50 m per 1 microsecond, 50 microsecond pause, 30 ms, four flashes | S3 PDF 1, 2, 4 | VERIFIED |
| 22 | 16 degrees: Hill adjacent-segment angle; Glassner "16-degree mean branching angle" | S3 PDF 2-3 | VERIFIED |
| 23 | 16 degrees as branch deviation from parent (L3's account of L2) | L3 PDF 2 | VERIFIED |
| 24 | S4 16 degrees per segment and branch | S4 PDF 2 | VERIFIED |
| 25 | Ribner-Roy: four-stroke average, 1/20 downward bias, 30 degree half-angle | S3 PDF 2 | VERIFIED |
| 26 | Glassner two-pass method; 30 degree cone; average of four | S3 PDF 4 | VERIFIED |
| 27 | Density form a/y^2 + b/y + c + d y + e y^2; Table 1; 7% error; eight slabs | S3 PDF 3 (page image) | VERIFIED |
| 28 | y as a percentage of total stroke length | S3 Fig. 1 caption PDF 3 | WEAK: the caption describes the plot axis; the paper does not restate the unit next to the equation |
| 29 | 3-5 m detail, mean step 3 m, cone radius 25% | S3 PDF 3-4 | VERIFIED |
| 30 | 0.06 degree vs 0.08 degree pixel argument | S3 PDF 3 | VERIFIED |
| 31 | Channel classes (main, secondary longest path, side) | L3 PDF 9 Sec. 6.3.2 | VERIFIED |
| 32 | Wattage 3.9e5 to 1.3e6 W/m; heuristic values; no tone mapping | L3 PDF 9 | VERIFIED |
| 33 | Plasma channel a few cm; thick look from glow saturation | L3 PDF 9 Sec. 6.3; S3 PDF 1 | VERIFIED |
| 34 | S4 dilation of the main channel plus small Gaussian | S4 PDF 6 | VERIFIED |
| 35 | APSF details, kappa 0.025 m, D = 2000 m, subtract minimum, separable | L3 PDF 8-10 | VERIFIED |
| 36 | Eq. 10 missing squares (my observation) | L3 PDF 9 page image | VERIFIED (printed as stated; the inference that it is wrong is mine) |
| 37 | Fig. 15 caption "tapering brightness" | L3 PDF 11 | VERIFIED |
| 38 | S2 ref [20] wrong title | S2 PDF 7; S1 PDF 1; Crossref | VERIFIED |
| 39 | No numeric taper or branch-length ratio in the sources read | searched all five texts | VERIFIED (absence within the sources read) |
