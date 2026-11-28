# R9-C2 Guaranteed bounds: interval arithmetic, affine arithmetic, and bounding a procedural model

Round 9, section C. Access date for every source: **2026-11-28**. PDFs were read with poppler `pdftotext`; the Stolfi and de Figueiredo monograph was converted from the authors' PostScript with ghostscript. Pages are **PDF pages** unless marked "printed". Everything is in my own words. Formulas are restated in standard mathematical form, which is not copyrightable, and **no code was taken**: the sources' routines are described, not reproduced. Reasoning marked **(mine)** is my own.

**Questions this item answers** (brief C2):
- What are the rules for add, multiply, divide, power, sin and cos?
- What is the dependency problem?
- How does outward rounding work?
- How can the output of a procedural model be bounded?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Moore R.E. (1966). *Interval Analysis.* Prentice-Hall | **None found** (in-copyright book) | Publisher copyright | **Not read.** Its role is reported by S2 and S3 |
| S2 (substitute for S1) | Hickey T., Ju Q., van Emden M.H. (2001). "Interval arithmetic: from principles to implementation." *Journal of the ACM* 48(5):1038-1068. doi:10.1145/502102.502106 | Co-author's site: https://maarten.vanemden.com/Publications/hickeyJuvE.pdf (same file as read, 250,734 bytes) | ACM copyright; author-posted | Yes: introduction, multiplication and division (PDF p.1-10) |
| S3 (substitute for S1 and S4) | Stolfi J., de Figueiredo L.H. (1997). *Self-Validated Numerical Methods and Applications.* Course notes, 21st Brazilian Mathematics Colloquium, IMPA | Author's site (gzipped PostScript): https://ic.unicamp.br/~stolfi/EXPORT/papers/by-tag/fig-sto-97-iaaa.ps.gz; converted to PDF on the box (122 pp.) | No licence stated; free from the author's site | Yes: Ch. 2 (IA, PDF p.21-48), Ch. 3 (AA, PDF p.49-92), Ch. 4 intro and level sets (PDF p.93-104). **Printed page = PDF page minus 6** |
| S4 | de Figueiredo L.H., Stolfi J. (2004). "Affine arithmetic: concepts and applications." *Numerical Algorithms* 37:147-158. doi:10.1023/B:NUMA.0000049462.70970.b6 | **None found.** OpenAlex: closed | Publisher copyright | **Not read.** Substitutes: S3, and S4b, the authors' 2002 talk with the same title |
| S4b | de Figueiredo L.H., Stolfi J. (2002). "Affine Arithmetic: Concepts and Applications." Invited talk slides, SCAN 2002 | https://lhf.impa.br/ftp/oral/aa.pdf (46 slides) | No licence stated | Yes |
| S5 | Snyder J.M. (1992). "Interval analysis for computer graphics." *Proc. SIGGRAPH 92*, pp. 121-130; and Snyder J.M. (1992), *Generative Modeling for Computer Graphics and CAD*, Academic Press | **None found** that downloaded (the one candidate link failed) | Publisher copyright | **Not read.** Described only through a secondary account in Havemann 2005 (R9-C1 S5, PDF p.38) |

## 1. The basic promise

**The invariant** (S3 PDF p.12, printed p.6; S2 PDF p.2-4):
- Every computed interval must **contain** every value the exact quantity could take.
- S2 calls this the "Fundamental Theorem of Interval Arithmetic". S2 credits Moore with first proving it.
- **Consequence:** an interval result is a guarantee. It may be loose, but it is never wrong, provided the exact algorithm is correct (S3 PDF p.26).

## 2. The rules (S3 §2.4, PDF p.28-38; S2)

Write an interval as [a, b].

| Operation | Rule | Notes |
|---|---|---|
| Negation | −[a, b] = [−b, −a] | Exact; no rounding (PDF p.28) |
| Addition | [a, b] + [c, d] = [a + c, b + d] | Lower bound rounded down, upper bound rounded up. An empty operand gives an empty result (PDF p.28) |
| Subtraction | [a, b] − [c, d] = [a − d, b − c] | (PDF p.29) |
| Scaling by a constant k | Multiply both ends; swap them if k < 0; k = 0 gives [0, 0] | Zero handled separately so that 0 × ∞ cannot give NaN (PDF p.29-30) |
| Multiplication | [min, max] of the four corner products ac, ad, bc, bd | The product is linear in each factor, so the extremes are at corners. Checking the signs first cuts the work to two multiplications in most cases and four in one case (nine sign cases) (PDF p.30-32; S2 also uses a nine-case split, PDF p.6) |
| Reciprocal | If [a, b] lies strictly on one side of 0: [1/b, 1/a]. If it straddles 0: the whole real line | Zero bounds handled separately because of IEEE signed zero (PDF p.32-33) |
| Division | x / y with its own case table. Any y straddling 0 gives the whole real line in S3 | A dedicated routine is slightly more exact than x × (1/y). For example [3, 3] / [3, 3] = [1, 1] exactly (PDF p.33-34) |
| Square root, log | Monotonic: evaluate at the ends. Any part of the argument outside the domain is cut off ("soft" policy) | Example: √[−3, 4] = [0, 2] (PDF p.26, p.34-35) |
| Exponential | Monotonic: evaluate at the ends; overflow becomes +∞ | (PDF p.35-36) |
| **Sine and cosine** | Not monotonic. For cos: scale the interval by 1/π. If it spans an even integer, the upper bound is 1; if it spans an odd integer, the lower bound is −1; otherwise take the end values. If it spans both an even and an odd integer, both bounds are fixed and the result is [−1, 1] (this case is implied by S3's two rules rather than stated separately). Floor and ceiling are computed in floating point to avoid integer overflow. Only cosine's routine is spelled out; sine follows as cos(x − π/2) **(mine)** | **π is not a float**, so it must itself be held as an interval between two neighbouring floats (PDF p.36-37) |
| Other functions | Any function with a directed-rounding routine, or a routine with a known error bound ε (widen by ε) | (PDF p.37-38, §2.4.13) |
| **Integer power xⁿ** | **Not covered by the sources read.** Derived **(mine)** by S3's method for non-monotonic functions (end values plus interior extremes). Odd n is monotonic: [aⁿ, bⁿ]. Even n: if 0 lies in [a, b], the result is [0, max(aⁿ, bⁿ)]; otherwise [min, max] of the ends | Write x² as a power, not as x × x. For x = [−1, 2], x × x gives [−2, 4] but x² gives [0, 4] **(mine)**. See §4 |

**When the divisor contains zero, the sources disagree.** Both views are given.
- **S3:** division by an interval that straddles zero returns the whole real line (PDF p.34).
- **S2:** the exact set can be **two rays**. Example: 1 divided by an interval from −∞ to 1 gives everything strictly below 0 together with everything at or above 1 (S2 PDF p.8). Its Theorem 7 (Ratz's formula, for bounded intervals) lists the cases; one of them is that dividing an interval that excludes 0 by exactly [0, 0] gives the **empty set** (S2 PDF p.8).
- S2 offers three representations: the least single interval containing the set (here the whole line); a union of closed intervals (more information, more cost); or a union that also tracks open ends (S2 PDF p.8).
- S2 also allows **unbounded** intervals and division by intervals containing zero, which classical texts forbid (PDF p.1-2). S3 likewise allows infinite bounds but forbids NaN endpoints (PDF p.25).

**Inputs outside the domain** (S3 PDF p.26):
- **Partly outside** (√ of an interval that dips below 0): cut silently. The overlap is an artefact of over-wide bounds, not a real negative value.
- **Wholly outside:** something is wrong with the program. Either signal an error ("early detection") or return the **empty interval** and make every later operation propagate it ("super-soft" failure).
- S3 calls the choice a classical tension between early detection and soft failure.

## 3. Outward rounding (S3 §1.4.3 and §2.3.1, PDF p.18-19, p.25; S2)

- **Rule.** Round every lower bound **toward −∞** and every upper bound **toward +∞**. Do the same for any intermediate value that affects a bound, always in the direction that widens the result (PDF p.25).
- **Mixed directions.** Sometimes the right direction for a sub-expression depends on another value's sign; such cases need care or external tests (PDF p.19).
- **Cost.** Changing the processor's rounding mode is expensive, around a dozen instructions. The trick is to compute a downward-rounded value as the negation of an upward-rounded one, so one mode serves both (PDF p.19).
- **Overflow** gives an infinite bound, which is still valid. **NaN** must be tested for and replaced by the whole line or the empty interval (PDF p.25).
- **S2's view.** IEEE 754's rules for signed infinities and signed zeros make sound, **optimal** (tightest possible) floating-point intervals efficient. S2's tables let an implementation avoid run-time tests that stall pipelines (S2 PDF p.1-2).

## 4. The dependency problem (S3 §2.6-2.7, PDF p.42-44)

**Cause.** Interval arithmetic treats every operand as varying **independently** over its range. When two operands are the same or related quantities, it counts combinations that cannot happen.

**Examples** (PDF p.42-43):
- x − x with x in [2, 5] gives [−3, 3], not [0, 0].
- x(10 − x) with x in [4, 6] gives [16, 36]. The true range is [24, 25], so the result is **20 times too wide**.

**Error explosion** (PDF p.43):
- In a chain of steps, the overall relative accuracy is roughly the **product** of the stages' accuracies.
- Width can therefore grow exponentially with depth.
- S3's example: iterating a function that converges to a constant gives intervals that **diverge** under IA (Fig. 2.2).

**Splitting the domain helps little** (PDF p.44):
- IA has first-order error, so relative accuracy does not improve with narrower inputs.
- Being a factor of 1,000 too loose may need about 1,000 sub-intervals.

**Remedies** (PDF p.44):
- Rewrite formulas so each variable **appears once**. If every variable occurs only once, IA gives the exact range (apart from rounding).
- Lump several operations into one hand-written "macro" operation with its own tight bounds.
- Use affine arithmetic (§5).

## 5. Affine arithmetic (S3 Ch. 3; S4b)

**Representation** (S3 PDF p.49-50, printed p.43-44):
- A quantity is a first-degree polynomial: x = x0 + x1·ε1 + … + xn·εn.
- Each **noise symbol** εi is unknown but lies in [−1, 1].
- Shared noise symbols record **correlation** between quantities.
- New symbols record approximation and rounding errors.

**The invariant.** At any moment between operations, **one** assignment of the noise symbols makes every affine form equal its true value (PDF p.50).

**Affine operations are exact** apart from rounding (PDF p.56). Addition, subtraction, scaling and adding a constant just combine coefficients. Hence x − x = 0 exactly.

**Non-affine operations** (PDF p.59):
- Replace the function by an affine approximation over the operands' range.
- Add **one new noise term** whose size bounds the approximation error.
- Two choices (S4b slide 15):
  - **Chebyshev**: smallest error;
  - **minimum range**: smallest range, and preserves signs.
  - Both have **quadratic** error in the input width.

**Multiplication** (PDF p.76-77):
- Expand the product. The constant and linear terms are kept.
- The quadratic remainder is replaced by its midpoint plus a new noise term of half its range.
- The exact range of that remainder costs O(m log m) in the number of terms m.
- A simpler bound, the product of the two forms' radii, is used in the routine shown (PDF p.80).

**Reciprocal and division** (PDF p.76, p.80):
- Reciprocal uses the min-range approximation and returns the whole line if the range straddles 0.
- Division by quantities whose range comes near zero is "inherently inaccurate and unstable". A little slack can make the divisor's range cross zero, and then division fails.

**Sine and cosine:** **not given in S3's AA chapter** (only square root, exponential, reciprocal, multiplication and division). They follow the same Chebyshev or min-range recipe **(mine)**.

**AA against IA** (PDF p.81-85):
- **Where AA is better:** any chain of affine operations is evaluated with exact range (PDF p.83). Errors made at one step can cancel later instead of always adding (PDF p.83). In S3's tests AA avoids the error explosion of iterated functions (Fig. 3.9).
- **Where AA is worse:** the Chebyshev approximations for √x and exp(x) can **overshoot**, giving a wider range for a single variable than plain IA (PDF p.69, p.81). This happens mostly in simple computations where cancellation has no chance to work.
- **Fix: a mixed AA/IA model** that keeps both and intersects them. It can succeed where both pure models fail (PDF p.82).
- **Cost:** AA is more complex and more expensive. Forms are sparse: each stores only its non-zero terms with symbol indices (PDF p.85).

## 6. Bounding a procedural model's output

**Adaptive enumeration** (S3 Ch. 4, PDF p.100-104):
1. Evaluate the inclusion function on a cell.
2. If the result excludes 0, **discard** the cell: no surface passes through it.
3. Otherwise split the cell, until cells are small or cannot be split.

In S3's examples, large parts of the domain are discarded early. AA gives a much tighter curve enclosure than IA on a quartic curve (PDF p.104).

**Ray casting implicit surfaces with AA** (S4b slide 42):
- AA keeps the linear correlation of x, y, z along the ray.
- This narrows where a root can lie, with quadratic convergence near simple roots.

**Snyder's GENMOD, as described by Havemann 2005** (R9-C1 S5, PDF p.38; not read first-hand):
- Every object supports point evaluation, an **inclusion function** (interval in, enclosing interval out) and differentiation.
- Recursive splitting of the input interval then gives general SOLVE and MINIMIZE operators. These can do constraint solving, ray intersection, implicit surfaces, surface intersection and self-intersection tests, and arc-length parameterisation.
- **Stated weakness:** these operators are flexible but can be very expensive.

**How it applies to the lane (mine):**
- **Every generator parameter is an interval.** Evaluate the whole plan once in interval (or AA) mode to get guaranteed bounds on every derived length, angle and position, before any mesh exists.
- **Containment checks become decidable but conservative.** "Hand inside reach envelope" or "eye inside orbit" either passes for sure, fails for sure, or is **inconclusive**, in which case split the parameter box and retry.
- **Watch for dependency in the plan.** Ratios of measurements that share a term (for example limb proportion = segment / total, where total includes segment) are exactly the x/(x + y) case that IA over-widens. Use AA, or rewrite each ratio so each variable appears once.
- **Use the empty-interval convention to report infeasibility.** A wholly out-of-domain operation then returns an explicit "no valid value", which can name the operation (see C3 on reporting).

## 7. Failure cases in one list

1. **Over-wide bounds** from dependency, growing exponentially along deep chains (S3 PDF p.42-43).
2. **Splitting is expensive** against first-order error (S3 PDF p.44).
3. **Division near zero** gives the whole line (S3), or two rays that a single interval cannot hold (S2).
4. **Wrong rounding direction** silently breaks the guarantee; π and other constants must themselves be intervals (S3 PDF p.25, p.36).
5. **NaN** must be trapped (S3 PDF p.25).
6. **AA overshoot** on simple non-linear steps (S3 PDF p.69, p.81).
7. **The guarantee holds only if the exact algorithm is correct** (S3 PDF p.26).
