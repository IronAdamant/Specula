# Second check: R8-A7-local-feature-size-least-edge.md

Independent re-check, 2026-11-27. Ruppert claims were re-found in the OCR text and the key formulas re-read on fresh 100-dpi page images (PDF 15, 17). Shewchuk delnotes claims were re-found by grep, with pages derived from the running heads (printed = PDF - 4). The NASA URL was re-downloaded and compared by MD5. The S3 citation was checked on Crossref.

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | L1 provenance: NASA RNR-94-002, URL, "to appear in J. Algorithms" | re-download, MD5 29cae6a5... identical | **FIX applied**: the URL was missing in the draft; added |
| 2 | Definition 1 (lfs) and "incident" via Fig. 5, D3 | L1 PDF 11-12 | VERIFIED |
| 3 | lfs continuous; slope ≤ 1 | L1 PDF 11 | VERIFIED |
| 4 | Lemma 1: lfs(q) ≤ lfs(p) + dist(p, q) | L1 PDF 11 (OCR shows "<"; the inequality direction and meaning are confirmed by Shewchuk Lemma 37) | VERIFIED |
| 5 | Shewchuk Lemma 37 and its proof; directional derivatives in [-1, 1] | delnotes PDF 92-93 (printed 88-89) | VERIFIED |
| 6 | lfs_min footnote: equals min over input vertices; not a domain lower bound (midpoint lfs_min/2) | delnotes PDF 96 (printed 92) | VERIFIED |
| 7 | Boxed conditions; α < arcsin(1/(2√2)) ≈ 20.7°; C_T, C_S formulas; α = 10° gives 2.8 and 5 | L1 PDF 13-16; PDF 15 image | VERIFIED. Recomputed: C_T(10°) = 2.648, C_S(10°) = 4.744; Ruppert rounds up to 2.8 and 5, and these satisfy all three conditions (2.736 ≤ 2.8; 4.96 ≤ 5) |
| 8 | Input angles ≥ 90° assumption | L1 PDF 14 and 23 | VERIFIED |
| 9 | Theorem 1 nearest neighbour ≥ lfs(p)/(C_S + 1) | L1 PDF 16 | VERIFIED |
| 10 | Theorem 2 and C₁ ≥ 4(2C_S + 3)²/π | L1 PDF 16-17 (image of PDF 17) | **FIX applied**: draft wording was vague ("given as at least"); now states the region B and the disjoint-disk argument as on the page |
| 11 | "halts for any α < 20°", practice ≈ 30° | L1 PDF 11 | VERIFIED; the internal 20° vs 20.7° disagreement is reported in the file |
| 12 | Theorem 39 (B ≥ √2, ≥ 60°, no edge shorter than lfs_min) | delnotes PDF 96-97 (printed 92-93) | VERIFIED |
| 13 | Lemma 41 (strict B > √2, > 60°) and Theorem 42 (≥ lfs(v)/(D_S + 1)) | delnotes PDF 98-99 (printed 94-95) | VERIFIED |
| 14 | 15°: B ≈ 1.93, D_T ≈ 5.66, D_S ≈ 9.01; "about ten times / seven times" | delnotes PDF 100 (printed 96) | VERIFIED |
| 15 | Lake Superior terminates at 34.2°, not at ≥ 34.3° | delnotes PDF 100, Fig. 6.15 | VERIFIED |
| 16 | Shewchuk's 60° vs Ruppert's 90° remark | delnotes PDF 93 (printed 89) | VERIFIED (from the drafting notes; the Theorem 39 text on PDF 96 also states the 60° condition) |
| 17 | sin θ = 1/(2B); √2 ↔ 20.7° | computed; delnotes printed 93 | VERIFIED |
| 18 | Triangle: 33.8° / 33.9°; Mitchell (2 cos θ)^(180°/θ) = 27 at 30°; 0.6°; 5°; hull edges | triangle96 PDF 5-7 | VERIFIED (re-found in text; 2 cos 30° = √3, √3^6 = 27, recomputed) |
| 19 | Corner-lopping, shield circles, angles > 2α divided into α..2α | L1 PDF 24-26 | VERIFIED |
| 20 | Concentric shells D·2^i, D = 0.01, rounded log₂; basic algorithm loops endlessly | L1 PDF 40 | **FIX applied**: the draft omitted that it is a heuristic and the midpoint-shell rule; wording expanded from the page |
| 21 | Fig. 13 "arms" / Euclidean lfs suboptimal for interior-only meshes | L1 PDF 25 | VERIFIED |
| 22 | Medial-axis sampling criterion; medial axis touches the surface at sharp corners | abe-crust PDF 3 | VERIFIED. S3 itself says "distance to the medial axis"; the phrase "local feature size" for it is not in S3 (the file says "the same idea is measured as") |
| 23 | S3 citation, pages 415-421, DOI | Crossref | VERIFIED |
| 24 | Fact A: each input edge ≥ lfs at its end points | - | Author's derivation; checked against Definition 1 (q is a vertex not incident to vertex p) |
| 25 | Facts B-E and the procedure | - | Author's derivation, labelled. The vertex speed 1/sin(α/2) matches R8-A3 |
| 26 | J. Algorithms page numbers | - | UNVERIFIABLE (journal version not read) |

Corrections: rows 1, 10, 20.
