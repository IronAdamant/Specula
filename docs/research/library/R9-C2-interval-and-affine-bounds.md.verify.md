# Verify: R9-C2-interval-and-affine-bounds.md

Re-check done 2026-11-28. Every page citation was re-searched by script in the page-marked text of S2 (hickey01), S3 (stolfi97) and S4b (aaslides). URLs were re-fetched and compared byte for byte. Crossref was checked for S2 and S4.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | S2 J ACM 48(5):1038-1068 | Crossref (title given there as "Interval arithmetic") | VERIFIED |
| 2 | S2 URL and size | curl 200, 250,734 bytes = local | VERIFIED |
| 3 | S3 URL | curl 200; gunzip output is identical (cmp) to the PS read | VERIFIED |
| 4 | S4 Numer. Algorithms 37:147-158 | Crossref (issue 1-4) | VERIFIED |
| 5 | S4b URL | curl 200, 402,956 bytes = local | VERIFIED |
| 6 | Fundamental invariant | S3 p.12; S2 "Fundamental Theorem" p.2 and p.4, Moore p.4 | VERIFIED |
| 7 | Guarantee holds only if the exact algorithm is correct | S3 p.26 | VERIFIED |
| 8 | Negation exact; empty operand gives an empty result for addition | S3 p.28 | VERIFIED |
| 9 | Subtraction; scaling with zero handled | S3 p.29-30 | VERIFIED |
| 10 | Nine sign cases for multiplication | S3 p.31; S2 p.6 | VERIFIED |
| 11 | Reciprocal straddling zero gives the whole line; signed zero | S3 p.32-33 | VERIFIED |
| 12 | Division routine; [3,3]/[3,3] exact | S3 p.33-34 | VERIFIED |
| 13 | Soft domain policy, √[−3,4] = [0,2]; early detection versus super-soft | S3 p.26, p.34-35 | VERIFIED |
| 14 | Exp overflow becomes +∞ | S3 p.35-36 | VERIFIED |
| 15 | Cos by scaling with 1/π, even and odd integers; π held as an interval of consecutive floats | S3 p.36 | VERIFIED |
| 16 | Floor computed in floating point to avoid integer overflow | S3 p.37 | VERIFIED |
| 17 | "Spans two or more integers gives [−1,1]" | implied by S3 p.36, not stated | FIX applied (reworded as implied) |
| 18 | Other functions with an error bound ε | S3 p.38 | VERIFIED |
| 19 | Integer power rule; sine from cosine | (mine) | Labelled (mine) |
| 20 | S2 two-ray example | S2 p.8: the set is strictly below 0, plus 1 and above | FIX applied ("at or below 0" corrected to "strictly below 0") |
| 21 | Theorem 7 (Ratz); three representations | S2 p.8 | VERIFIED; FIX applied (added the [0,0] gives empty case) |
| 22 | S2 allows unbounded intervals and zero-containing denominators | S2 p.1-2 | VERIFIED |
| 23 | S3 forbids NaN endpoints; NaN must be trapped | S3 p.25 | VERIFIED |
| 24 | Outward rounding rule | S3 p.25 | VERIFIED |
| 25 | Mixed directions; mode switching costs about a dozen instructions; negation trick | S3 p.18-19 | VERIFIED |
| 26 | Overflow gives infinity, still valid | S3 p.25 (§2.3.2) | VERIFIED |
| 27 | IEEE signed infinities and zeros make optimal implementation efficient; pipeline stalls from tests | S2 p.1-2 | VERIFIED |
| 28 | Dependency examples x−x, x(10−x) [16,36] versus [24,25] | S3 p.42-43 | VERIFIED |
| 29 | Error explosion; Fig. 2.2 divergence | S3 p.43 | VERIFIED |
| 30 | Splitting helps little; single occurrence gives the exact range; macro operations | S3 p.44 | VERIFIED |
| 31 | AA noise symbols, invariant | S3 p.50 | VERIFIED |
| 32 | Affine operations exact apart from rounding | S3 p.56 (affine combination keeps all information except roundoff), p.83 (relative accuracy 1) | VERIFIED |
| 33 | Non-affine approximations with a new noise term | S3 p.59 | VERIFIED |
| 34 | Chebyshev versus min-range | S4b slide 15; S3 p.62, p.70 | VERIFIED |
| 35 | Multiplication O(m log m); radius-product bound | S3 p.77, p.80 | VERIFIED |
| 36 | Division "inherently inaccurate and unstable" | S3 p.80 | VERIFIED |
| 37 | Cancellation, exact range for affine chains | S3 p.83 | VERIFIED |
| 38 | Overshoot | S3 p.69, p.81 | VERIFIED |
| 39 | Mixed AA/IA | S3 p.81-82 | VERIFIED |
| 40 | Sparse forms | S3 p.85 | VERIFIED |
| 41 | Adaptive enumeration discards cells; quartic tighter with AA | S3 p.101-104 | VERIFIED |
| 42 | AA ray casting | S4b slide 42 | VERIFIED |
| 43 | GENMOD inclusion functions via Havemann | Havemann PDF p.38 | VERIFIED (secondary, as stated) |
| 44 | Lane notes | (mine) | Labelled (mine) |

Fixes: rows 17, 20, 21.
WEAK: Moore 1966, the 2004 AA paper and Snyder 1992 were not read first-hand. Sine and cosine in AA are (mine).
