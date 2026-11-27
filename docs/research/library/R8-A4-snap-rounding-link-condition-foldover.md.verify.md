# Second check: R8-A4-snap-rounding-link-condition-foldover.md

Independent re-check, 2026-11-27. Every page cite was re-found by searching the `.pg.txt` page-marked text of the raw PDF for the claim's key terms, separately from the drafting pass. The page offsets were re-derived from the printed running heads (Dey: journal = PDF + 22; Floater: journal = PDF + 684; dBHO: printed = PDF - 1; Garland thesis: printed = PDF - 10; Packer: printed = PDF - 2). Hobby's formulas were checked against 100-dpi page images because the OCR loses symbols.

| # | Claim | Source file, page | Status |
|---|---|---|---|
| 1 | SR definition: hot pixels, chains through the centres of the hot pixels in the order met | dbho-snap PDF 3 (p. 2); isr-trier p. 2; packer-msc PDF 12 (p. 10) | VERIFIED |
| 2 | Guibas-Marimont's three properties, including "no segment ever passes completely over a vertex" | dbho-snap PDF 3, lines "Geometric similarity" / "passes completely over a vertex" | VERIFIED (second-hand; L1 not read, as the file says) |
| 3 | ISR restatement "no vertex ever crosses through a curve" | isr-trier PDF 2 | VERIFIED |
| 4 | Hobby tolerance square R half-open; bend a segment through each rounded point whose square it meets | hobby99 OCR p. 3 + page image | VERIFIED |
| 5 | Naive re-intersection iteration has no guarantee on the count | hobby99 p. 2 | VERIFIED |
| 6 | Hobby Thm 4.1 (fully intersected preserved), Lemma 4.2 (direction (1, ±1) by slope sign), Lemma 4.3 | hobby99 PDF 13-14 | VERIFIED |
| 7 | No interleaving; weaker than Greene-Yao; output may meet in several disjoint pieces | hobby99 PDF 15 | VERIFIED |
| 8 | Orientation reading of SR (signs may go to zero, never flip) | - | Author's inference, labelled as such in the file. Not in any source |
| 9 | Vertex-edge distance about 1/√((2^b - 1)² + 1) ≈ 2^-b; construction by moving t's ends | isr-trier PDF 3 | VERIFIED |
| 10 | "Extra topology info makes output more cumbersome" | isr-trier PDF 4 | **FIX applied**: drafted as part of p. 3 with an unsupported sentence about floating-point failure; now cited to p. 4 and the float remark is labelled as my note |
| 11 | Orange (degree-2) pixels; Theorem 3.1 with non-redundancy; triangle q1 p q2 argument | dbho-snap PDF 8-9 (p. 7-8) | VERIFIED |
| 12 | Σ|S_h| can be Θ(n³), Goodrich et al. Θ(n³ log n) | dbho-snap PDF 3-4 (Fig. 1) | VERIFIED |
| 13 | Rounded segments meet and diverge many times; ISR "only defined algorithmically", poor geometric similarity | dbho-snap PDF 9 (p. 8) | VERIFIED |
| 14 | ISR drift "can be very large" | dbho-snap PDF 4 (p. 3) | VERIFIED |
| 15 | dBHO Theorem 2.1, O((n + I) log n) | dbho-snap PDF 7 (p. 6) | VERIFIED |
| 16 | Reroute procedure; no new hot pixels | isr-trier PDF 4-5 | VERIFIED |
| 17 | Lemma 3.1, Corollary 3.2, Lemma 3.3 (i) and (ii), Lemma 3.4 | isr-trier PDF 6-7 | VERIFIED |
| 18 | ISR Theorem 4.2 time bound O(n log n + I + L^(2/3) N^(2/3+ε) + L); N ≤ 2n + I | isr-trier PDF 9 | VERIFIED |
| 19 | Theorem 4.3: Θ(n²) pixels per chain, Θ(n³) incidences, Θ(n²) arrangement size | isr-trier PDF 9 | VERIFIED |
| 20 | Packer Theorem 3.8: ISR drift Θ(n²), each hot pixel adds ≤ √2/2 | packer-msc PDF 23 (p. 21) | VERIFIED |
| 21 | Dey: closure / star / link definitions; link is a complex, star generally not | dey-pim PDF 3 (p. 25) | VERIFIED |
| 22 | Order, Order Bound, Bd_j, M_j, shark fin | dey-pim PDF 5-6 (p. 27-28) | VERIFIED |
| 23 | Contraction, unfolding, local, relaxed | dey-pim PDF 7 (p. 29) | VERIFIED |
| 24 | Link Conditions with ω; Fig. 2 examples {x, y} and {x, y, z, xz} | dey-pim PDF 8 (p. 30) | VERIFIED |
| 25 | Table 1 redundancy; 1-complex rule (common neighbour, or both degree ≠ 2) | dey-pim PDF 9 (p. 31) | VERIFIED |
| 26 | Bordered-surface reading: one condition (i = 0, with ω coned to the boundary) remains | dey-pim PDF 9 text: i = j = d - 1 subsumed by d - 2 | **FIX applied**: the draft said "my paraphrase, check against p. 31"; checked, and now stated as checked |
| 27 | Theorems A (p. 32), B (p. 33), B′ (p. 36), C (p. 37, one direction), C′ (p. 40) | dey-pim PDF 10, 11, 14, 15, 18 | VERIFIED |
| 28 | B′ proof: an extra edge leaves xy in one triangle; an extra vertex puts cx in four triangles | dey-pim PDF 14 | VERIFIED |
| 29 | Open problem for d ≥ 4; tetrahedron is the only irreducible S²; S³ has infinitely many (knots) | dey-pim PDF 18-19 (p. 40-41) | VERIFIED |
| 30 | Hoppe legal edge collapse (3 rules), legal swap, "proof in [6]" | hoppe-meshopt PDF 5 | VERIFIED |
| 31 | Garland fold-over definition, Fig. 2.12 | garland-thesis PDF 34-35 (p. 24-25) | VERIFIED |
| 32 | Normal-flip threshold, and its weakness (needs a threshold) | garland-thesis PDF 66 (p. 56); garland-quadrics PDF 5 | VERIFIED |
| 33 | Perpendicular-plane test, Fig. 3.15 | garland-thesis PDF 67 (p. 57) | VERIFIED |
| 34 | Penalise (progress) versus discard (may stall; reconsidered only when the neighbourhood changes) | garland-thesis PDF 66 | VERIFIED |
| 35 | Clustering (Rossignac-Borrel, Low-Tan) compatible with quadrics | garland-thesis §3.8, PDF 67 (p. 57) | VERIFIED |
| 36 | GH97 valid pairs (edge, or distance < t); t = 0 is edge contraction; aggregation | garland-quadrics PDF 3; thesis §3.9, PDF 68 | VERIFIED (thesis cite added) |
| 37 | Floater Theorem 4.1 page | floater03 PDF 7 = p. 691 | **FIX applied**: draft said p. 688 = PDF 4 |
| 38 | Floater §5 global from local | floater03 PDF 10-11 (p. 694-695) | VERIFIED; wording tightened to "under Theorem 4.1's hypotheses" |
| 39 | Planar fold-over sufficiency (winding-number argument) and the welding recipe | - | Author's derivation and synthesis, labelled. Boundary-vertex case explicitly left open |
| 40 | Guibas-Marimont 1998, Hershberger 2013, Goodrich et al. 1997 content | - | UNVERIFIABLE (not obtained), as the file says |
| 41 | Trier copy legality of ISR | - | WEAK: legality unclear; content cross-checked against Packer's thesis (S2b) |

Corrections made: rows 10, 26, 37 (and cite added at 36). No other errors found.
