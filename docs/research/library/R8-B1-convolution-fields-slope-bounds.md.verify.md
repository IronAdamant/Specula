# Second check: R8-B1-convolution-fields-slope-bounds.md

Re-check, 2026-11-27. Method:
- Citations were checked against Crossref (all four DOIs).
- Text claims were re-found by grep in the page-marked text (`*.flow.txt`, with `=== PDF p.N ===` markers).
- Formulas were checked on page images: SCALIS PDF 4, 7, 8 and 9; Hart PDF 46.
- Derived constants were recomputed in Python.

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | BS91 citation 25(4) 251-256, doi 10.1145/122718.122757 | Crossref | VERIFIED (Crossref lists the proceedings, pp. 251-256) |
| 2 | Sherstyuk VC 15(4) 171-182; thesis embargoed and "In Copyright" | Crossref; figshare API (is_embargoed True) | VERIFIED |
| 3 | Angles TOG 36(6) Art. 181 | Crossref (pages 1-13); PDF footer | VERIFIED |
| 4 | Hart VC 12(10) 527-545 | Crossref | VERIFIED. The copy read is a course-notes version (footers 5-xx), as the file says |
| 5 | Distance vs convolution; creases; summation bulges; halves-of-segment bulge | bs91 PDF 2 | VERIFIED |
| 6 | Superposition h★(S1+S2) | bs91 PDF 2 | VERIFIED |
| 7 | Separable evaluation via images; cubic-spline approximation; no closed form for a segment | bs91 PDF 3-4 | VERIFIED |
| 8 | Weight w(s): carrot taper; negative weights between fingers; variable kernel width | bs91 PDF 4 | VERIFIED |
| 9 | "<1% energy above half a unit", four samples per unit | bs91 PDF 3-4 | VERIFIED |
| 10 | Cauchy, Inverse and Compact Polynomial kernel formulas and properties | scalis PDF 4 image | VERIFIED |
| 11 | Jin eq. 6 (height) vs Hornus eq. 7 (width); Hornus closed form only for Inverse degree 2 | scalis PDF 4 image | VERIFIED |
| 12 | Three faults (radius control, blurring, scale-dependent blending) | scalis PDF 4-5 | VERIFIED |
| 13 | Eqs. 8-9 scale invariance; subdivision independence | scalis PDF 6 | VERIFIED |
| 14 | Eq. 10 and its continuity classes; eq. 11 with τ² | scalis PDF 7 image | VERIFIED |
| 15 | Linear-radius segment; two antiderivative forms; Tables 4-5 | scalis PDF 7 image | VERIFIED |
| 16 | Leibniz gradients "never done before"; Inverse equals the standard formulation with weight (Δτ t+τ₀)^{i−1}; compact-support quadratic in t | scalis PDF 8 image | VERIFIED |
| 17 | Eq. 12 N_k(c); L = 2τ√(σ²−1) | scalis PDF 8 image | VERIFIED |
| 18 | §5.2 end and maximum thinning; 1-ring phantom length | scalis PDF 9 image | VERIFIED |
| 19 | Loss of superposition at Y-junctions; dependence on kernel and σ | scalis PDF 12 (line "Loss of superposition") | VERIFIED |
| 20 | Gradient-angle operator, eqs. 1-2; bulge, premature bulge, tunnel | angles PDF 2 | VERIFIED |
| 21 | Iso 0.5; linear fall-off R₁ → 1/2, R₁+R₂ → 0; any monotone C⁰ fall-off | angles PDF 5 | VERIFIED |
| 22 | Limitations: similar fall-offs, gradient discontinuities, non-associativity | angles PDF 11 | VERIFIED |
| 23 | 1/(2R₂) slope | - | Labelled as my inference; arithmetic correct |
| 24 | Hart: Lipschitz definition and Theorem 1 | hart PDF 9 | VERIFIED |
| 25 | C_R cubic; Lip = 3/(2R) at R/2 | hart PDF 45-46 image | VERIFIED. C′(R/2) = 6/4R − 6/2R = −3/(2R) recomputed |
| 26 | Theorem 5 misprint | hart PDF 46 image | VERIFIED that the printed form differs from the proof. The correction is mine and labelled |
| 27 | Kernel slope maxima table | Python | VERIFIED (0.6495, 0.8587, 1.0352, 1.3293; 2, 1.5396, 1.7173) |
| 28 | SCALIS raytrace paper not read | rt.log / file size | VERIFIED (download incomplete) |
| 29 | Licence and legality of the Hart copy | ResearchGate cover page on a course site | WEAK: legality unclear, as the file says |

Corrections: none needed after the check. Row 29 stays WEAK.
