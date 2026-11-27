# Second check: R8-B2-meshing-dual-contouring-mc33.md

Re-check, 2026-11-27. Each claim was re-found by grep in `*.flow.txt` / `*.pg.txt`, and its PDF page was recomputed with the page-marker helper. DOIs and pages come from the search records and the PDFs' own headers.

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | DC printed = PDF + 338 | dc02 footers 339...346 on PDF 1...8 | VERIFIED |
| 2 | QEF eq. 1 | dc02 PDF 3 = p. 341 | **FIX applied** (the draft said PDF 2 = p. 340) |
| 3 | Two-step DC algorithm | dc02 PDF 3 | VERIFIED |
| 4 | Normal-equation instability, 256³, bᵀb ≈ 10⁶; QR form | dc02 PDF 3-4 | VERIFIED |
| 5 | SVD truncation < 0.1; minimiser outside the cube | dc02 PDF 4 = p. 342 | VERIFIED |
| 6 | Minimal-edge rule; always closed; even count; 6 or more polygons possible | dc02 PDF 6 = p. 344 | **FIX applied** (the draft said pp. 343-344) |
| 7 | Prior octree method "occasionally yields contours with cracks" | dc02 PDF 6 = p. 344 | **FIX applied** (the draft said p. 343) |
| 8 | DC non-manifold for ambiguous configurations; 3-check safe-collapse test | dc02 PDF 7 = p. 345 | VERIFIED |
| 9 | MDC: DC non-manifold "for all of the ambiguous sign configurations"; Nielson's Dual MC; always manifold | mdc07 PDF 2 | VERIFIED |
| 10 | Proposition 1 (χ = 1; 0 or 2 face crossings); sufficient, not necessary; genus; Fig. 5 | mdc07 PDF 5 | VERIFIED |
| 11 | Clustering 10.89 s vs 10.99 s (Table I) | mdc07 PDF 7-8 (table spans the break) | VERIFIED (largest model row) |
| 12 | MDC may contain intersecting polygons; "all subject to such geometric errors" | mdc07 PDF 8 | VERIFIED |
| 13 | MC33: holes (config 6 vs complement of 3); trilinear eq. 1; 15 → 33 | mc33 PDF 3 (OCR) | VERIFIED |
| 14 | Face test AC > BD; denominator positive | mc33 PDF 4 (OCR) | VERIFIED |
| 15 | Internal ambiguity: configuration 4; hyperbola projection; slice quadratic with three steps | mc33 PDF 4-5 (OCR) | VERIFIED. OCR of the t_max formula is noisy; −b/(2a) is the standard vertex of the parabola and is consistent with the text: WEAK on exact notation |
| 16 | Lewiner: "not complete"; 730 subcases; manifold claim | lewiner PDF 2-3 | **FIX applied** (page cites) |
| 17 | Cases 3, 4, 6, 7, 10, 12, 13 test table | lewiner PDF 5 | VERIFIED |
| 18 | Custódio issues I-IV with pages | custodio PDF 5-7 | VERIFIED |
| 19 | Etiene et al. found Lewiner's implementation incorrect | custodio PDF 1 | VERIFIED |
| 20 | Citations (TOG 21(3) 339-346; TVCG 13(3) 610-619; JGT 8(2) 1-15; C&G 37(7) 840-850) | search records; dc02 footers | VERIFIED (all four checked on Crossref) |
| 21 | Engine comparison paragraph | - | Labelled as mine |

Corrections: rows 2, 6, 7 and 16.
