# R8-B1. A smooth field around a skeleton, with a bound on its slope

Round 8, section B (lane: one body surface). Prepared 2026-11-27; access date 2026-11-27.

**The question.** Closed forms for a segment's field when the radius varies along it; for each, a stated bound on the gradient ("the field never changes faster than L per metre"), or a note that none is given; and how each avoids bulges where bones meet.

## Sources

| # | Citation | Free legal copy | Licence | Read? / page scheme |
|---|---|---|---|---|
| L1 | Bloomenthal J., Shoemake K. (1991). "Convolution surfaces." *Computer Graphics (Proc. SIGGRAPH '91)* 25(4): 251-256. doi:10.1145/122718.122757 | Author copy (Bloomenthal's site): https://www.unchainedgeometry.com/JBloom/pdf/CSurfFinal.pdf (6 pp.) | Author copy; © ACM | Yes. The copy has no printed page numbers, so **PDF pages** are cited |
| L2 | Sherstyuk A. (1999). "Kernel functions in convolution surfaces: a comparative analysis." *The Visual Computer* 15(4): 171-182. doi:10.1007/s003710050170 | **None found.** Springer is paywalled. His Monash PhD thesis ("Convolution surfaces in computer graphics", 1998, doi:10.26180/14876034) is listed on Monash Bridges as "In Copyright" with its file **embargoed** (figshare API, checked) | - | **No.** Its kernels are known only second-hand through L3 (PDF 4) |
| L3 | Zanni C., Bernhardt A., Quiblier M., Cani M.-P. (2013). "SCALe-invariant Integral Surfaces." *Computer Graphics Forum* 32(8): 219-232. doi:10.1111/cgf.12199 | HAL author preprint hal-00863523v2: https://inria.hal.science/hal-00863523v2/document | HAL deposit (author licence to HAL; no CC licence) | Yes. Preprint page numbers = PDF − 1 (PDF 1 is the HAL cover). Journal pages are not given |
| L4 | Angles B., Tarini M., Wyvill B., Barthe L., Tagliasacchi A. (2017). "Sketch-based implicit blending." *ACM Trans. Graph.* 36(6), Article 181. doi:10.1145/3130800.3130825 | Author lab copy: https://theialab.ca/pubs/angles2017sketch.pdf (also HAL hal-01590351) | Author copy; © ACM | Yes. Page 181:N = PDF N |
| Sub-1 | **Substitute for the slope bound that none of the listed papers gives:** Hart J. C. "Sphere tracing: a geometric method for the antialiased ray tracing of implicit surfaces" (*The Visual Computer* 12(10), 1996, 527-545, doi:10.1007/s003710050084). The version read is a 51-page preprint with appendices and **course-notes page footers (5-xx)**, hosted on a Stanford CS348B course page: https://graphics.stanford.edu/courses/cs348b-20-spring-content/uploads/hart.pdf | Course-hosted copy of an author preprint (it carries a ResearchGate cover page) | **Legal status unclear**; read for content only | Yes. Cited by PDF page and footer |

A second SCALIS paper (Fast ray tracing of scale-invariant integral surfaces, HAL hal-03169283) was found, but its download did not complete in time and it was **not read**.

## 1. The fields

**Distance surfaces vs convolution (L1, PDF 1-2).**
- A point skeleton gives the potential exp(−‖s−p‖²/2). The surface is f = c.
- For an extended skeleton there are two generalisations:
  - Take the potential from the *nearest* skeleton point only. This gives a distance surface (an offset solid).
  - Sum (integrate) the potentials of *all* skeleton points. This gives a convolution surface, f = h ★ S with a Gaussian kernel h.
- Distance surfaces of a non-convex skeleton show **creases** (curvature discontinuities). Blending them by summation removes the creases but **creates bulges**. In particular, two halves of one segment bulge at the joint, because each piece is a union (max) while the pieces are blended by summation (PDF 2).

**Superposition, the anti-bulge property (L1, PDF 2).** Convolution is linear: h★(S1+S2) = h★S1 + h★S2. So two abutting polygons (or segments) give exactly the surface of their union, and a skeleton can be split freely with no seam (PDF 2-3).

**Evaluation and varying radius in L1 (PDF 3-4).**
- L1 gives **no closed form for a segment**. It uses the Gaussian's separability: planar skeletons are scan-converted into images, blurred in two directions, then multiplied by a Gaussian of the distance to the plane.
- In practice the Gaussian is approximated by a cubic spline "to simplify computation and to limit kernel width" (PDF 3).
- Radius variation comes from two levers:
  - a **weight** w(s) inside the integral. A falling weight gives "a tapered shape, like a carrot", and negative weights stop unwanted blending, for example between fingers (PDF 4);
  - a **varying kernel width** (PDF 4).
- **Slope bound: none given.** The only quantitative statement is a sampling remark: a standard Gaussian passes less than 1% of its energy above half a unit, so about four samples per unit suffice (PDF 3-4).

**Closed-form kernels (L3 citing [MS98, She99], PDF 4 = preprint p. 3; formulas checked on the page image).** With d the distance and σ a resizing constant:
- Cauchy of order i: k(d) = 1 / (1 + (d/σ)²)^{i/2}. It is C^∞ but has global support.
- Inverse of order i: k(d) = 1 / (d/σ)^i. It is C^∞ with global support and infinite on the skeleton, so the skeleton is always inside the surface. That makes subtractive ("digging") operators ineffective near the skeleton.
- Compact polynomial of order i: k(d) = (1 − (d/σ)²)^{i/2} for d < σ, and 0 otherwise. It is "only" C^⌊(i−1)/2⌋ but has local support.

**Two older models of varying radius (L3, PDF 4 = p. 3).**
- Jin's "standard formulation": f(p) = ∫ τ(s) k(‖Γ(s)−p‖) ds (eq. 6). τ scales the kernel's *height* while the support width stays fixed. General closed forms exist for segments [HC12].
- Hornus: f(p) = ∫ k(‖Γ(s)−p‖/τ(s)) ds (eq. 7). τ scales the kernel's *width* at constant height. A closed form exists only for the Inverse kernel of degree 2.

The listed faults of these models (PDF 4-5 = pp. 3-4) are:
1. no radius control: the radius depends non-linearly on τ;
2. blurring and vanishing details: thin parts collapse, though not with the Inverse kernel;
3. scale-dependent blending: rescaling a model changes how it blends.

**SCALIS (L3 §3-5, PDF 6-9 = pp. 5-8; formulas checked on the page images).**
- **Constant τ (eqs. 8-9):** evaluate the unit-weight convolution in a space scaled by 1/τ, so f_{τ,S}(p) = f_{1,S/τ}(p/τ). This gives f_{ατ,αS}(αp) = f_{τ,S}(p), which is **scale invariance**. Independence from how the skeleton is subdivided is kept (PDF 6 = p. 5).
- **Varying τ(s) along a curve (eq. 10, PDF 7 = p. 6):** f(p) = ∫ k(‖Γ(s)−p‖/τ(s)) ds/τ(s). Compared with Hornus, this adds a density 1/τ along the skeleton. It is C^∞ for the Inverse and Cauchy kernels and C^⌊(i+1)/2⌋ for the compact polynomial kernel.
- **Surface skeletons (eq. 11):** divide by τ(u,v)² instead.
- **Segment with linear radius (PDF 7 = p. 6):** τ(t) = τ₀ + Δτ·t and Γ(t) = a + t(b−a) for t ∈ [0,1]. Then ‖p−Γ(t)‖² = ‖b−a‖²t² − 2(b−a)·(p−a)t + ‖p−a‖². Substituting this into eq. 10 leaves antiderivatives of two forms:
  - ∫ w(t)(at² − 2bt + c)^{i/2} dt, with w polynomial;
  - ∫ (at² − 2bt + c)^{i/2} / (dt + e)^k dt.
  
  These are solved by the recurrences in appendix Tables 4-5, following [HC12, Hub12].
- **Gradients (PDF 8 = p. 7):** closed forms for the gradient follow by differentiating under the integral sign (Leibniz's rule). The authors say these "were never done before with previous models".
- **Kernel-specific results (PDF 8 = p. 7):**
  - For the Inverse kernel of order i, scale-invariant SCALIS equals the standard formulation with weight (Δτ·t + τ₀)^{i−1}.
  - For the compact polynomial kernel, first find the part of the segment inside the support. This is a quadratic inequality in t, solved through its discriminant; the result is always one connected interval of [0,1].
- **Radius control (§5.1, eq. 12, PDF 8 = p. 7):** divide by N_k(c), the field value at distance τ from an infinite line of constant weight τ (independent of τ by scale invariance). Then τ(s) *is* the radius wherever enough skeleton length is nearby. For the compact polynomial kernel the length needed satisfies (στ)² = τ² + (L/2)², so L = 2τ√(σ² − 1).
- **Extreme cases (§5.2, PDF 9 = p. 8):** near segment ends and at local radius maxima, the shape gets thinner than τ. The fix adds a phantom skeleton length at graph nodes, based on each node's 1-ring.
- **Slope bound: none stated** in L3.

**Gradient-based composition (L4).**
- Blending by any operator g(f_a, f_b) that sees only the two values causes **bulges** where objects overlap, **premature bulging** before contact, and tunnels (Fig. 3, PDF 2 = 181:2).
- The remedy, after Gourmel et al. 2013, is to make the operator depend on θ, the angle between ∇f_a and ∇f_b: f_c = g(f_a, f_b, θ) (eqs. 1-2, PDF 2).
- L4's primitives use the iso-value 0.5, with a **linear fall-off** in distance: R₁ maps to 1/2 and R₁ + R₂ to 0. Any monotone C⁰ fall-off is allowed, because g carries the smoothness (PDF 5).
- **Slope bound: none stated** in L4.
- **My inference (not stated in L4):** where the fall-off is linear in a Euclidean distance (which has gradient norm 1), the primitive's field changes by 0.5 over R₂, so |∇f| = 1/(2R₂) there. The composed field's slope then also depends on g, for which no bound is given.
- **L4's own limitations (PDF 11 = 181:11):**
  - fall-offs must be roughly similar, or the result "can diverge arbitrarily" (for example when the two R₂ differ greatly);
  - artefacts appear where the scalar fields have gradient discontinuities;
  - the operators are not associative.

## 2. The slope bound

**From a substitute (Sub-1, Hart).**
- *Definition (PDF 9):* f is Lipschitz with constant λ if |f(x) − f(y)| ≤ λ‖x − y‖. Sums add their bounds; compositions multiply them (PDF 9).
- *Theorem 1 (PDF 9):* f/λ is then a signed-distance *bound*. This is exactly the "never changes faster than L per metre" statement a mesher needs.
- *Appendix D.1 (PDF 45-46, footers 5-113 to 5-114; checked on the page image):*
  - The soft-object cubic is C_R(r) = 2r³/R³ − 3r²/R² + 1 for r < R, and 0 otherwise (eq. 36).
  - Its slope is greatest at r = R/2, so **Lip C_R = 3/(2R)** (eq. 41).
  - A field f = T − Σ C_{R_i}(‖x − p_i‖) therefore has Lipschitz bound **Σ 3/(2R_i)**. Hart notes that local bounds, summing only the primitives that touch x, are tighter (PDF 46).
- **Apparent misprint (my check):** Theorem 5 is printed as d(x,B) ≥ (2/3) f(x) Σ R_i. The proof (a sum of Lipschitz constants 3/(2R_i)) gives d ≥ f(x) / Σ 3/(2R_i) = (2/3) f(x) / Σ (1/R_i). The two agree only for a single primitive. **Use the form from the proof.**

**Applied to the listed kernels (my derivation; not stated in any source).**
- Write u = d/σ. Then |dk/dd| = (1/σ)|dk/du|, and the maxima are:

  | Kernel | max \|dk/du\| | Where |
  |---|---|---|
  | Cauchy, order i | i·u*(1 + u*²)^{−(i/2+1)} | u*² = 1/(i+1). i = 2: 0.6495; i = 3: 0.8587; i = 4: 1.0352; i = 6: 1.3293 |
  | Compact polynomial, i = 2 | 2 | the support edge u = 1, where it is discontinuous in slope (C⁰) |
  | Compact polynomial, i = 4 | 8/(3√3) ≈ 1.5396 | u = 1/√3 |
  | Compact polynomial, i = 6 | 1.7173 | u = 1/√2 |
  | Inverse | unbounded near the skeleton | - |

  The values were computed on the box with Python.
- For f(p) = ∫ w(s) k(‖Γ(s)−p‖) ds, differentiating under the integral (as L3 does) gives |∇f(p)| ≤ ∫ w(s)·max|k′| ds. So the bound is **Lip f ≤ (max|dk/du| / σ)·(total skeleton weight within the support)**.
- For SCALIS eq. 12 the same step gives Lip f ≤ (1/N_k(c))·∫ (max|dk/du|/(σ·τ(s)²)) ds over the skeleton within reach. **Thin parts (small τ) dominate the slope.** This is the price of exact radius control.

## 3. Bulges where bones meet: summary

| Model | Mechanism | Remaining bulge risk | Source |
|---|---|---|---|
| Distance surface plus summation | none | bulges at every joint | L1 PDF 2 |
| Convolution (constant weight) | superposition: a split skeleton gives the same field | none along a chain. Y-junctions are bulge-free only if the weights are stacked (L3 PDF 12 = p. 11) | L1 PDF 2; L3 |
| Weighted / Hornus convolution | as above | thin parts blur and vanish; blending depends on scale | L3 PDF 4-5 |
| SCALIS | scale-invariant warp plus normalisation | loses superposition for stacked skeletons, so Y-junction quality "highly depends on the kernel used and on its σ"; mitigated by splitting thin branches (PDF 12 = p. 11). Radius shortfall at ends is fixed by phantom length (PDF 9) | L3 |
| Gradient-based operator | choose the blend by the angle between gradients | only within the sketched behaviour; non-associative; needs similar fall-offs | L4 PDF 2, 11 |

## 4. Disagreements
- **Continuity of the compact polynomial kernel.** L3 gives the kernel as C^⌊(i−1)/2⌋ (PDF 4) but the SCALIS integral as C^⌊(i+1)/2⌋ (PDF 7). These are not contradictory, since integration along the skeleton raises regularity by one (L3 says so, PDF 7).
- **Fall-off smoothness.** L4 says min/max blends need fall-offs that are at least C¹, while its own operators accept C⁰ fall-offs (PDF 5).
- **Hart's Theorem 5:** the printed statement and the proof disagree (see §2).

## Not obtained or not verified
- L2 (Sherstyuk 1999) and his thesis (embargoed): not read.
- The SCALIS ray-tracing paper (hal-03169283): not read (incomplete download).
- No listed source states a slope bound. The bounds in §2 are Hart's (a substitute) plus my derivations.
