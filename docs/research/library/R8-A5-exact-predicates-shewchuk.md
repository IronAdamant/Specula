# R8-A5 Exact answers from floating point: Shewchuk's adaptive predicates

Round 8, section A. Access date for every source: **2026-11-27**. PDFs were downloaded to the box and read with poppler. **Every constant below was read off a rendered page image** (pdftotext loses Shewchuk's symbols), and the decimal values were computed by me with exact rational arithmetic. No code was taken from any source; the algorithms are described in words only.

**Question this item answers.** (1) The error bounds of the orientation and in-circle predicates, as formulas in machine epsilon. (2) Which results are identical on every platform, and which operations the predicates need: only add, subtract and multiply?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | **Listed: Shewchuk J. R. (1997).** "Adaptive precision floating-point arithmetic and fast robust geometric predicates." *Discrete & Computational Geometry* 18(3): 305-363. doi:10.1007/PL00009321. Read as the author's long version, CMU-CS-96-140 / `robustr.pdf`, 59 pp., from https://people.eecs.berkeley.edu/~jrs/papers/robustr.pdf | Author's own copy | No licence stated (author self-archive; journal © Springer) | Yes: §1-2.5, §3.2, §4 to §5. Printed page = PDF page - 4. **DCG journal pages not mapped** |
| S2 | **Listed: Shewchuk J. R.** *Lecture Notes on Geometric Robustness.* UC Berkeley, CS 274 course notes, 1999 onward; the downloaded PDF shows a build date of "June 13, 2026" (probably LaTeX's automatic date, not a revision date). https://people.eecs.berkeley.edu/~jrs/meshpapers/robnotes.pdf | Author's own copy | No licence stated | Yes: §1-3. Printed page = PDF page - 4 |
| S3 | **Supporting (for the Rust side), not listed:** Rust RFC 3514, "Float semantics", start date 2023-10-14, rust-lang/rfcs#3514. https://rust-lang.github.io/rfcs/3514-float-semantics.html | Public web page | Rust project, MIT / Apache-2.0 | Yes, the guide and reference sections |

## 1. What the predicates compute

- **ORIENT2D(a, b, c)** is the sign of the 2×2 determinant |a - c, b - c| (that is, (a_x - c_x)(b_y - c_y) - (a_y - c_y)(b_x - c_x)). It is positive if a, b, c are in counter-clockwise order, negative if clockwise, and zero if collinear (S2 p. 8 = PDF 12, Expression (2)).
- **INCIRCLE(a, b, c, d)** is the sign of the 3×3 determinant whose rows are (p_x - d_x, p_y - d_y, (p_x - d_x)² + (p_y - d_y)²) for p = a, b, c. It is positive if d is inside the circle through a, b, c when those are counter-clockwise (S2 p. 8, Expression (4)).
- **ORIENT3D** and **INSPHERE** are the 3D analogues (S2 §3.1-3.2; S1 §4.3-4.4).
- **Translated forms are preferred.** S2 p. 8: the forms that subtract c (or d) first have "much smaller errors" than the raw 3×3 / 4×4 forms and should be used whenever floating point with rounding is involved. S1's predicates use the translated forms.

## 2. Machine epsilon and the arithmetic model

- **ε = ½ ulp(1) = 2^-p.** For IEEE 754 double, p = 53, so **ε = 2^-53 ≈ 1.11 × 10^-16**. For single, ε = 2^-24 (S1 printed p. 28 = PDF 32). Exact rounding means each basic operation's relative error is at most ε.
- **Required arithmetic:** radix 2 with exact rounding (S1 abstract and p. 1-2). That is, each +, -, × must return the correctly rounded exact result.
- **Tie-breaking.** Round-to-even is needed by FAST-EXPANSION-SUM. LINEAR-EXPANSION-SUM is the variant that also works with other tie rules (S1 §2.4, printed p. 9-10 = PDF 13-14).
- **Overflow and underflow are not handled** (S1 printed p. 3 = PDF 7). **But the four predicates in S1 do not overflow or underflow if every input's exponent is in [-142, 201] and IEEE double is used** (same page; checked on the page image). In practice: keep coordinates roughly between 10^-42 and 10^60 in magnitude, or zero. (The decimal range is my arithmetic: 2^-142 ≈ 1.8 × 10^-43, 2^201 ≈ 3.2 × 10^60.)

## 3. The adaptive scheme (S1 §4.2, printed p. 38-41 = PDF 42-45, Fig. 21)

Each predicate is evaluated in up to four stages. It stops as soon as the sign is certain:

1. **A**: plain floating-point evaluation of the translated determinant. Accept the sign if |A| exceeds the A error bound.
2. **B**: the exact value of the determinant of the already-rounded differences (the four subtractions at the leaves are assumed exact). Exact products are formed as two-term expansions. B′ is a p-bit approximation of B. Shewchuk notes B is the true answer whenever the leaf subtractions were exact, which is the common case (printed p. 39).
3. **C**: B′ plus a first-order correction for the rounding error of the leaf subtractions. The correction is added approximately, but the bound is still O(ε²) because the sign is only in doubt when B is O(ε) (printed p. 39).
4. **D**: the exact determinant, built incrementally from B's expansions.

**Exceptions.** For INSPHERE, D is not built incrementally; it is computed from scratch ("due to programmer laziness") if C is not accurate enough (printed p. 44 = PDF 48). **No INSPHERE error-bound table is given in S1.**

## 4. The error bounds (formulas)

Notation: ⊕, ⊖, ⊗ are rounded floating-point operations. |·| is absolute value. Each bound is evaluated in floating point **and** already accounts for its own rounding, as long as the coefficient is pre-computed exactly.

### ORIENT2D (S1 Table 1, printed p. 41 = PDF 45)

Let x5 = (a_x ⊖ c_x) ⊗ (b_y ⊖ c_y) and x6 = (a_y ⊖ c_y) ⊗ (b_x ⊖ c_x), so A = x5 ⊖ x6 (printed p. 40 = PDF 44). Then:

| Stage | Accept the sign if the value's magnitude is at least |
|---|---|
| A | (3ε + 16ε²) ⊗ (\|x5\| ⊕ \|x6\|) |
| B′ | (2ε + 12ε²) ⊗ (\|x5\| ⊕ \|x6\|) |
| C | (3ε + 8ε²) ⊗ \|B′\| ⊕ (9ε² + 64ε³) ⊗ (\|x5\| ⊕ \|x6\|) |

**How stage A's constant arises (S1 printed p. 40-41).**
- Each exact quantity is the computed value plus or minus a bounded error. Then t5 = x5 ± (3ε + 3ε² + ε³)|x5| and t6 likewise.
- The sign of A is certain if |A| ≥ (3ε + 6ε² + 8ε³)(|x5| + |x6|).
- Allowing for the rounding of the bound's own sum and product gives a coefficient of (3ε + 12ε² + 24ε³).
- Rounding that coefficient up to a p-bit number gives **3ε + 16ε²**.

### ORIENT3D (S1 Table 3, printed p. 44 = PDF 48)

Translate by d. Define:
- α_a = |a_z ⊖ d_z| ⊗ ( |(b_x ⊖ d_x) ⊗ (c_y ⊖ d_y)| ⊕ |(b_y ⊖ d_y) ⊗ (c_x ⊖ d_x)| )
- α_b = |b_z ⊖ d_z| ⊗ ( |(c_x ⊖ d_x) ⊗ (a_y ⊖ d_y)| ⊕ |(c_y ⊖ d_y) ⊗ (a_x ⊖ d_x)| )
- α_c = |c_z ⊖ d_z| ⊗ ( |(a_x ⊖ d_x) ⊗ (b_y ⊖ d_y)| ⊕ |(a_y ⊖ d_y) ⊗ (b_x ⊖ d_x)| )

Let S = α_a ⊕ α_b ⊕ α_c. Then:

| Stage | Bound |
|---|---|
| A | (7ε + 56ε²) ⊗ S |
| B′ | (3ε + 28ε²) ⊗ S |
| C | (3ε + 8ε²) ⊗ \|B′\| ⊕ (26ε² + 288ε³) ⊗ S |

### INCIRCLE (S1 Table 5, printed p. 45 = PDF 49)

Translate by d. Define:
- α_a = ((a_x ⊖ d_x)² ⊕ (a_y ⊖ d_y)²) ⊗ ( |(b_x ⊖ d_x) ⊗ (c_y ⊖ d_y)| ⊕ |(b_y ⊖ d_y) ⊗ (c_x ⊖ d_x)| )
- α_b and α_c follow the same cyclic pattern (b with c, a; c with a, b).

The squares are floating-point squares. Let S = α_a ⊕ α_b ⊕ α_c. Then:

| Stage | Bound |
|---|---|
| A | (10ε + 96ε²) ⊗ S |
| B′ | (4ε + 48ε²) ⊗ S |
| C | (3ε + 8ε²) ⊗ \|B′\| ⊕ (44ε² + 576ε³) ⊗ S |

### INSPHERE

No table in S1. Stage D is computed from scratch (printed p. 44).

### Decimal values for IEEE double (ε = 2^-53), computed by me

All of these coefficients are exactly representable as doubles. S1 says so for Table 1 (printed p. 41: "each coefficient is expressible in p bits"); I checked the other tables by exact rational arithmetic.

| Coefficient | Value |
|---|---|
| 3ε + 16ε² (O2D A) | 3.3306690738754716e-16 |
| 2ε + 12ε² (O2D B′) | 2.2204460492503146e-16 |
| 3ε + 8ε² (C, all predicates) | 3.3306690738754706e-16 |
| 9ε² + 64ε³ (O2D C) | 1.1093356479670487e-31 |
| 7ε + 56ε² (O3D A) | 7.771561172376103e-16 |
| 3ε + 28ε² (O3D B′) | 3.330669073875473e-16 |
| 26ε² + 288ε³ (O3D C) | 3.2047474274603644e-31 |
| 10ε + 96ε² (ICC A) | 1.1102230246251577e-15 |
| 4ε + 48ε² (ICC B′) | 4.440892098500632e-16 |
| 44ε² + 576ε³ (ICC C) | 5.423418723394464e-31 |

**Side property (S1 printed p. 41).** The ORIENT2D bounds are zero when all three points lie on one horizontal or vertical line. So the common axis-aligned collinear case is decided at stage A with no exact arithmetic.

## 5. Which operations are needed

- **Only addition, subtraction and multiplication.** S1 §3.2 (printed p. 28 = PDF 32): any expression built from +, -, × can be evaluated adaptively this way. The four predicates are polynomials in the coordinates.
  - **No division**, **no square root** (squares in INCIRCLE are products), and no library functions.
  - The comparisons are against the bounds above.
  - Absolute value and sign tests are exact bit operations.
- **Exact products without a fused multiply-add.**
  - Two-Product uses Dekker's splitting (S1 Theorem 17, printed p. 18 = PDF 22; checked on the page image). Multiply a by (2^s + 1) and recover a high part and a low part using two subtractions.
  - This works for p ≥ 3 and any split point s with p/2 ≤ s ≤ p - 1.
  - For p = 53 that means 27 ≤ s ≤ 52. The usual choice s = 27 gives the multiplier 2^27 + 1 = 134,217,729 (my arithmetic).
  - So Two-Product needs only ⊕, ⊖, ⊗.
- **Exact sums.** Two-Sum and Fast-Two-Sum (S1 §2.3) recover the rounding error of an addition with extra additions and subtractions only.

## 6. What is the same on every platform

**Shewchuk's conditions for correctness (S1 §5 "Caveats", printed p. 47-48 = PDF 51-52; and S1 p. 1-2):**
1. **Binary arithmetic with exact rounding** for +, -, × (IEEE 754 satisfies this).
2. **No extended-precision intermediate registers.** On x87 (Intel 80486 / Pentium) the 80-bit internal registers make the roundoff-recovery steps fail. Workarounds (printed p. 47-48): `volatile` variables force a store to memory, but this is slow and can still go wrong by **double rounding** (rounded once to the extended format, then again to double), which can leave a roundoff error not expressible in p bits (citing Priest). Shewchuk's better fix is to set the processor to round internally to double precision; how to do that is compiler-specific and not portable.
3. **No "optimising" compiler rewrites.**
   - Example: a compiler that "proves" b_virtual = b in Fast-Two-Sum and deletes the subtraction breaks the algorithm.
   - **My addition (not in S1, which predates common FMA hardware):** contracting a×b + c into one fused multiply-add changes the rounding the same way and must also be ruled out.
4. **Round-to-even ties** if FAST-EXPANSION-SUM is used (S1 §2.4).
5. **Exponents in [-142, 201]** for the inputs (no overflow or underflow; S1 p. 3).

**When those hold, the result of each predicate is the exact sign of the exact determinant of the input doubles.** The answer is therefore a mathematical fact about the inputs and is bit-for-bit the same on every conforming platform. Stage A, B, C or D may differ between implementations, but the returned sign cannot. (This follows from exactness; it is my wording, not a quotation.)

**What is not platform-independent:** constructed values (intersection points, circumcentres) are rounded and do not inherit exactness. S1 §1 (printed p. 1 = PDF 5: exact arithmetic "is not a panacea"; it makes robust the algorithms with purely combinatorial output) and S2 p. 1 (PDF 5: constructors are harder to make robust because constructed objects may need far more precision than the input) make this point.

**The Rust side (S3, labelled supporting source).**
- RFC 3514 specifies that Rust's +, -, *, /, %, mul_add, sqrt, casts and comparisons on f32/f64 "exactly match IEEE 754-2008" with roundTiesToEven, default exception handling and no flush-to-zero. Rust does not support other rounding modes or changed FP control bits.
- An exact IEEE match for `a * b + c` means two roundings, so the compiler may not fuse it (my reading).
- NaN payloads are the only non-determinism. Predicates never create NaNs from finite inputs.
- **Documented exceptions in S3:**
  - 32-bit x86 without SSE2 (i586 targets) uses x87 with different internal precision, giving non-IEEE results.
  - 32-bit ARM NEON SIMD flushes subnormals to zero, a risk only if the code is auto-vectorised.
- **So, for std-only Rust on mainstream 64-bit targets (my conclusion):** an implementation using only f64 `+ - *` and comparisons satisfies conditions 1-4. Avoid i586 targets. Keep inputs inside the exponent range. Avoid `mul_add` only if a software fallback is unwanted: it is IEEE-exact by S3 but may be a library routine on targets without FMA hardware.

## 7. Failure cases and related warnings

- **Inexact predicates break algorithms, not just answers.** One wrong INCIRCLE sign among four nearly collinear points can make a Delaunay algorithm produce an invalid triangulation (S2 printed p. 3-4 = PDF 7-8, Fig. 1).
- **Measured failure (S1 Table 9, printed p. 48 = PDF 52):** the non-robust (approximate-predicate) version of Shewchuk's 3D incremental Delaunay code **failed to terminate** on a tilted-grid input of 10,000 points; the robust version finished in 108.5 s.
- **Tolerances (epsilon tests) do not restore consistency.** They make the answers of different predicate calls mutually inconsistent (S2 printed p. 6 = PDF 10).
- **Translate to the origin first.** The polygon-area example shows the error of the untranslated formula depends on absolute coordinates (S2 printed p. 6 = PDF 10). Subtracting two doubles of the same sign within a factor of two is exact, so translating triangles near the origin is often error-free (S2 printed p. 6-7 = PDF 10-11).
- **Exact predicates with rounded constructions can still fail.** Use exact predicates on input coordinates only, or on constructions represented exactly (my recommendation, following S1 p. 1 and S2 p. 1).

## Summary of key answers

- **ε = 2^-53.**
- **ORIENT2D:** A: (3ε + 16ε²)(|x5| + |x6|); B′: (2ε + 12ε²)(...); C: (3ε + 8ε²)|B′| + (9ε² + 64ε³)(...).
- **ORIENT3D:** A: (7ε + 56ε²)·S; B′: (3ε + 28ε²)·S; C: (3ε + 8ε²)|B′| + (26ε² + 288ε³)·S.
- **INCIRCLE:** A: (10ε + 96ε²)·S; B′: (4ε + 48ε²)·S; C: (3ε + 8ε²)|B′| + (44ε² + 576ε³)·S.
- **INSPHERE:** no table; exact from scratch after C.
- **Operations:** only +, -, × with exact rounding (no division, no sqrt, no libm). Two-Product uses Dekker's split with multiplier 2^27 + 1.
- **Platform independence:** the sign is exact, hence identical, given radix-2 exact rounding, round-to-even, no extended registers, no FMA contraction or algebraic "optimisation", and input exponents in [-142, 201]. Rust f64 meets these on all targets except i586 (S3).
