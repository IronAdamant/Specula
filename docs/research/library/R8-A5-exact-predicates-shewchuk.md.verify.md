# Second check: R8-A5-exact-predicates-shewchuk.md

Independent re-check, 2026-11-27. Every constant was re-read from a freshly rendered 110-dpi page image of `shewchuk-robust.pdf` (PDF pages 7, 22, 32, 43-45, 48-49), not from the drafting notes. Decimal values were recomputed with Python `fractions` (exact rationals) and round-tripped through float to test exact representability. Text claims were re-found by grep in the page-marked text. S1 page offset re-derived from the running heads: printed = PDF - 4.

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | ε = ½ ulp(1) = 2^-p; 2^-53 double, 2^-24 single | S1 printed 28 / PDF 32 (image) | VERIFIED |
| 2 | Any +, -, × expression can be made adaptive | S1 printed 28 / PDF 32 (image) | VERIFIED |
| 3 | Radix 2, exact rounding required | S1 PDF 5 and 6 (text lines 20, 121, 127) | VERIFIED |
| 4 | Overflow/underflow not addressed; no overflow/underflow for exponents in [-142, 201] with IEEE double | S1 printed 3 / PDF 7 (image) | VERIFIED |
| 5 | FAST-EXPANSION-SUM needs round-to-even; LINEAR-EXPANSION-SUM makes no tie assumption | S1 PDF 13-14 | **FIX applied**: page range narrowed from "printed 9-13 = PDF 13-17" to printed 9-10 = PDF 13-14 |
| 6 | Theorem 17 (Dekker split): p ≥ 3, p/2 ≤ s ≤ p - 1, multiplier 2^s + 1 | S1 printed 18 / PDF 22 (image) | VERIFIED. 2^27 + 1 = 134,217,729 recomputed |
| 7 | Stage scheme A / B / B′ / C / D; B exact if leaf subtractions exact; C's O(ε²) argument | S1 printed 39 / PDF 43 (image, Fig. 21) | VERIFIED |
| 8 | Derivation: t5 = x5 ± (3ε + 3ε² + ε³)\|x5\|; (3ε + 6ε² + 8ε³); (3ε + 12ε² + 24ε³); rounded up to (3ε + 16ε²) | S1 printed 40-41 / PDF 44-45 (images) | VERIFIED |
| 9 | Table 1 ORIENT2D: A (3ε + 16ε²), B′ (2ε + 12ε²), C (3ε + 8ε²)\|B′\| ⊕ (9ε² + 64ε³), all ⊗ (\|x5\| ⊕ \|x6\|) | S1 printed 41 / PDF 45 (image) | VERIFIED |
| 10 | x5, x6 definitions | S1 printed 40 / PDF 44 (image) | VERIFIED |
| 11 | Bounds zero for points on a horizontal or vertical line | S1 printed 41 / PDF 45 | VERIFIED |
| 12 | Table 3 ORIENT3D: 7ε + 56ε², 3ε + 28ε², (3ε + 8ε²)\|B′\| ⊕ (26ε² + 288ε³); α_a, α_b, α_c | S1 printed 44 / PDF 48 (image) | VERIFIED, each α term compared symbol by symbol |
| 13 | Table 5 INCIRCLE: 10ε + 96ε², 4ε + 48ε², (3ε + 8ε²)\|B′\| ⊕ (44ε² + 576ε³); α with squared lifted terms ("squares are approximate") | S1 printed 45 / PDF 49 (image) | VERIFIED |
| 14 | INSPHERE: no table; D from scratch | S1 printed 44 / PDF 48 (image) | VERIFIED |
| 15 | Decimal values and exact representability of all 10 coefficients | computed | VERIFIED (re-run; all True) |
| 16 | Caveats: compiler removing b_virtual; x87 / 80486 / Pentium extended registers; `volatile` workaround | S1 printed 47 / PDF 51 | **FIX applied**: the draft presented `volatile` and FPU precision mode as equal workarounds. The PDF 52 image shows `volatile` can still fail by double rounding and the preferred fix is internal double-precision rounding; wording corrected. Added Table 9's non-terminating approximate run (PDF 52, image) as a failure case |
| 17 | FMA contraction also breaks it | - | Author's addition, labelled in the file |
| 18 | Exact predicates not a panacea; constructors harder | S1 PDF 5; S2 PDF 5 | **FIX applied**: drafted as "S1 §4.1 and S2 §1-2". §4.1 is a robustness survey, so the cite was moved to S1 p. 1 and S2 p. 1 |
| 19 | Translated expressions (2), (4) preferred | S2 printed 8 / PDF 12 | VERIFIED |
| 20 | Delaunay failure from one wrong INCIRCLE, Fig. 1 | S2 PDF 7-8 | VERIFIED (re-found in text) |
| 21 | Tolerancing inconsistent; translation of triangles exact if within factor of two | S2 PDF 10-11 | VERIFIED |
| 22 | RFC 3514: IEEE 754-2008 match with roundTiesToEven, no flush-to-zero; ops list includes mul_add, sqrt; no alt rounding modes | rfc3514.txt lines 212-219 | VERIFIED |
| 23 | RFC 3514 exceptions: i586 x87 precision; 32-bit ARM NEON FTZ | rfc3514.txt lines 318-321 | VERIFIED |
| 24 | "Rust will not fuse a*b+c" | - | WEAK: inferred from "exactly match IEEE"; RFC 3514 does not use the word "contraction". Labelled as my reading in the file |
| 25 | DCG journal page numbers for S1 content | - | UNVERIFIABLE: tech-report pagination only, stated in the file |

Corrections: rows 5, 16 and 18. No constant needed correcting.
