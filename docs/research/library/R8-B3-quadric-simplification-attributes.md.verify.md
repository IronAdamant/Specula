# Second check: R8-B3-quadric-simplification-attributes.md

Re-check, 2026-11-27. Method:
- Citations were checked on Crossref (GH97 pp. 209-216; GH98 doi 10.1109/VISUAL.1998.745312, pp. 263-269; Hoppe doi 10.1109/VISUAL.1999.809869, pages listed as "59-510").
- Claims were re-found by grep with the page helper.
- Formulas were checked on page images (GH98 PDF 4-5; Hoppe PDF 3).

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | GH97 printed = PDF + 208 | Crossref 209-216, 8 PDF pages | VERIFIED |
| 2 | K_p = ppᵀ with a²+b²+c² = 1; sum over incident planes; initial error 0; 10 floats; at most 3-fold counting | garland-quadrics PDF 4 | VERIFIED |
| 3 | Q̄ = Q₁+Q₂; eq. 1 placement; segment then endpoint/midpoint fallback | PDF 3 | VERIFIED |
| 4 | Valid pairs; threshold t; t = 0 gives edge contraction | PDF 3 | VERIFIED |
| 5 | Ellipsoid interpretation; degenerate planes and cylinders | PDF 4 | VERIFIED |
| 6 | Perpendicular boundary planes with large penalty | PDF 4 | VERIFIED |
| 7 | Normal-flip test: penalise or disallow | PDF 5 | VERIFIED. **FIX applied:** the cited prior scheme is [7] Ronfard and Rossignac, not Hoppe |
| 8 | Non-manifold regions "regularly" created; topology not maintained | PDF 1 | VERIFIED |
| 9 | Link condition plus t = 0 recommendation | - | Labelled as mine |
| 10 | GH98 e₁, e₂, A, b, c | gh98 PDF 4 image | VERIFIED |
| 11 | Unit-cube scaling; copying attributes fails (Fig. 10); segregated metric rejected | gh98 PDF 4 | VERIFIED |
| 12 | Coefficient table 10/21/28/28 | gh98 PDF 5 image | VERIFIED |
| 13 | Clamping colours; renormalising normals; equal weights "doubtful" | gh98 PDF 5 | VERIFIED |
| 14 | Seams §6.1; multiple quadrics proposed but not implemented | gh98 PDF 5 | VERIFIED |
| 15 | Area weighting; crease penalty planes | gh98 PDF 2, PDF 3 | VERIFIED |
| 16 | Hoppe: R³ projection; ŝ_j fit with n·g_j = 0 (4×4 system); summed A, b, c | hoppe99 PDF 3 image | VERIFIED |
| 17 | 11+4m vs (4+m)(5+m)/2; Table 1 values | hoppe99 PDF 3 image | VERIFIED |
| 18 | λ_j weights; clamp and re-evaluate; renormalise normals; QP more expensive | hoppe99 PDF 3 image | VERIFIED |
| 19 | Wedges; 3+km never stored | hoppe99 PDF 4 | VERIFIED |
| 20 | Memoryless and volume preservation, "decreasing importance" | hoppe99 PDF 1, 4 | VERIFIED |
| 21 | Undulation failure | hoppe99 PDF 5 | **FIX applied:** stated precisely (planar mandrill; noise × shrinkage × memoryless) |
| 22 | Bone-weight counts 595 and 131 | arithmetic | VERIFIED. (B+5 choose 2) at B = 30 is 35·34/2 = 595; 11 + 120 = 131 |
| 23 | Bone-weight method | - | Labelled as mine and untested |

Corrections: rows 7 and 21.
