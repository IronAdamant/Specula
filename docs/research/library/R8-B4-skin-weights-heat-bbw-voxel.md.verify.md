# Second check: R8-B4-skin-weights-heat-bbw-voxel.md

Re-check, 2026-11-27. Crossref was used for all three DOIs:
- L1: 26(3), article 72.
- L2: 30(4); Crossref shows pages 1-8.
- L3: pp. 173-180.

Claims were re-found by grep in `baran07.flow.txt`, `bbw11.flow.txt` and `maya-bind-skin-options.txt`, with the page helper.

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | Heat analogy; surface-only solve with nearest-bone heat | baran PDF 7 | VERIFIED |
| 2 | Eq. 1, pⁱ, H_jj = c/d², visibility, ties kc/d² | baran PDF 7 | VERIFIED |
| 3 | c ≈ 0.22 vs c = 1 | baran PDF 7 | VERIFIED |
| 4 | Σwⁱ = 1 derivation | baran PDF 7 | VERIFIED |
| 5 | Factor once, back-substitute; TAUCS | baran PDF 7 | VERIFIED |
| 6 | Forcing weight 1 caused occasional artefacts | baran PDF 7 | VERIFIED |
| 7 | Well-defined interior as future work; rubbery dress/armour; hips, shoulder/neck | baran PDF 8 | VERIFIED |
| 8 | Fig. 11 LBS knee/hip | baran PDF 7 | VERIFIED |
| 9 | BBW eqs. 2-6 | bbw PDF 2 | VERIFIED |
| 10 | No local maxima observed, not proven; bounds "likely" help | bbw PDF 3 | VERIFIED |
| 11 | Heat weights not smooth at handles; unbounded biharmonic weights negative or non-local | bbw PDF 4 | VERIFIED |
| 12 | TetGen; Armadillo 43,234 → 46,898; MOSEK; superlinear | bbw PDF 4 | VERIFIED |
| 13 | Separate solves then normalise; 50× for the Gargoyle | bbw PDF 4-5 | VERIFIED |
| 14 | Table 2 BT/h 0.0906-12.0870 s; |Ω| up to 142,073 | bbw PDF 6 | VERIFIED |
| 15 | Limitations: bind time, volume needed, no linear precision; polygon soup OK | bbw PDF 6 | VERIFIED |
| 16 | Biharmonic equation Δ²w = 0 | bbw PDF 3 (line "∆2 wj = 0") | VERIFIED |
| 17 | Shape-aware fall-off | bbw PDF 3 | VERIFIED |
| 18 | Maya: falloff default 0.2; knee and hand voxels connected; heat "often fails" | maya-bind-skin-options.txt | VERIFIED (string match) |
| 19 | L3 abstract claims (non-manifold etc., pose-time weights) | search-page abstract only | WEAK: not verified against the paper |
| 20 | Leak inferences (welded surfaces; volume contact) | - | Labelled as mine |
| 21 | L1's 16 test characters | baran PDF 8 ("16 models") | VERIFIED |

Corrections: none needed. Row 19 is WEAK (L3 not read).
