# R8-B3. Simplifying without losing the shape or the rig

Round 8, section B. Prepared 2026-11-27; access date 2026-11-27.

**The question.** The quadric's formulas; the checks that prevent fold-over and non-manifold results (tied to A4's link condition); and how per-vertex attributes (here, bone weights) are carried.

## Sources

| # | Citation | Free legal copy | Licence | Read? / page scheme |
|---|---|---|---|---|
| L1 | Garland M., Heckbert P. S. (1997). "Surface simplification using quadric error metrics." *Proc. SIGGRAPH '97*: 209-216. doi:10.1145/258734.258849 | Author copy (CMU), file `garland-quadrics.pdf` (also used in R8-A4 as S6) | Author copy; © ACM | Yes. 8 PDF pages for pp. 209-216, so **printed = PDF + 208** |
| L2 | Garland M., Heckbert P. S. (1998). "Simplifying surfaces with color and texture using quadric error metrics." *Proc. IEEE Visualization '98*: 263-269. doi:10.1109/VISUAL.1998.745312 | Author copy: https://www.cs.cmu.edu/~garland/Papers/quadric2.pdf | Author copy; © IEEE | Yes. **PDF pages** (the copy has no printed numbers) |
| L3 | Hoppe H. (1999). "New quadric metric for simplifying meshes with appearance attributes." *Proc. IEEE Visualization '99*, doi:10.1109/VISUAL.1999.809869 (Crossref gives pages "59-510", which presumably includes a colour plate) | Author copy: https://hhoppe.com/newqem.pdf | Author copy; © IEEE | Yes. **PDF pages** |
| S1 | Supporting: Garland M. (1999), PhD thesis CMU-CS-99-105 (see R8-A4, S5). Printed = PDF − 10 | CMU / author | No licence stated | Used only through R8-A4 |

## 1. The quadric (L1)

**Fundamental quadric (L1 §5, PDF 4 = p. 212).**
- Each triangle's plane is p = [a b c d]ᵀ with a² + b² + c² = 1, so ax + by + cz + d = 0.
- K_p = ppᵀ is a symmetric 4×4 matrix. vᵀK_p v is the squared distance from the homogeneous point v = [x y z 1]ᵀ to that plane.
- A vertex's quadric Q is the sum of the K_p of its incident triangles. So its initial error is 0, since a vertex lies in all its triangles' planes.
- Error: Δ(v) = vᵀQv.
- Storage: 10 floats.
- Planes may be **multiple-counted**, at most 3 times each, because every plane starts at its triangle's three vertices. L1 accepts this imprecision for speed and notes an inclusion-exclusion fix if storage allows (PDF 4).

**Contraction (L1 §4, PDF 3 = p. 211).**
- When contracting (v₁, v₂) → v̄, set Q̄ = Q₁ + Q₂.
- **Optimal placement:** solve ∂Δ/∂x = ∂Δ/∂y = ∂Δ/∂z = 0. This means inverting Q with its bottom row replaced by [0 0 0 1] and multiplying by [0 0 0 1]ᵀ (eq. 1).
- **Fallbacks:** if that matrix is singular, find the best point along the segment v₁v₂; if that also fails, take the best of v₁, v₂ and their midpoint (PDF 3).
- **Valid pairs (PDF 3):** (v₁, v₂) is a valid pair if it is an edge, or if ‖v₁ − v₂‖ < t. With t = 0 this is plain edge contraction. t > 0 allows "aggregation" of unconnected parts.
- **Geometric meaning (§5.1, PDF 4):** the level sets are usually ellipsoids, and v̄ is their centre. On flat regions they degenerate to parallel planes, and along creases to cylinders. There the matrix is singular, which triggers the fallbacks.

**Boundaries (L1 §6, PDF 4 = p. 212).** For each face next to a boundary or discontinuity edge, add the plane through that edge **perpendicular** to the face, weighted by a **large penalty factor**.
- L2 repeats this for creases and open boundaries (PDF 3).
- L2 also suggests weighting quadrics by triangle **area** (PDF 2).

## 2. Fold-over and non-manifold checks

**Fold-over (L1 §6, PDF 5 = p. 213).** "Preventing mesh inversion": a contraction can make neighbouring faces fold over.
- Compare each neighbouring face's normal before and after the contraction. If it flips, **penalise heavily or disallow** the contraction.
- L1 uses "essentially the same scheme as others" (it cites [7], Ronfard and Rossignac, as an example).
- The planar exact form, and the Garland-thesis variant with perpendicular planes, are in R8-A4 §3.

**Non-manifold results (L1 PDF 1-3).**
- L1 **does not** preserve topology or manifoldness. It says so plainly: aggregation "will regularly create non-manifold regions", and its design accepts this (PDF 1).
- Supporting non-manifold input is listed as a feature (PDF 1).
- **For a manifold, rig-preserving body mesh:** use t = 0 (edges only, no aggregation) and add the **link condition** from R8-A4 (Dey et al.: Lk(a) ∩ Lk(b) = Lk(ab), with the boundary variant) as a hard test before each contraction. This combination is **my recommendation**, built from R8-A4's sources; L1-L3 do not state it.

## 3. Carrying attributes

**L2, the generalised quadric (§5.1, PDF 4; checked on the page image).**
- Treat each vertex as a point in Rⁿ: 3 position coordinates followed by n − 3 attributes. Scale the model into the unit cube of Rⁿ so that the attributes and the position have comparable scale (PDF 4).
- For a triangle T = (p, q, r) in Rⁿ, build an orthonormal pair:
  - e₁ = (q − p)/‖q − p‖;
  - e₂ = (r − p − (e₁·(r − p))e₁) / ‖r − p − (e₁·(r − p))e₁‖.
- The squared distance from v to T's 2-plane is D² = vᵀAv + 2bᵀv + c, where:
  - A = I − e₁e₁ᵀ − e₂e₂ᵀ;
  - b = (p·e₁)e₁ + (p·e₂)e₂ − p;
  - c = p·p − (p·e₁)² − (p·e₂)².
- The same summing, contraction and optimal-placement steps then work in Rⁿ. Placement *synthesises* new attribute values; it does not interpolate the endpoints (PDF 4-5).
- Fig. 10 shows that simply copying v₁'s attributes fails visibly (PDF 4).
- **Cost (PDF 5):** an n×n A has (n+2 choose 2) unique coefficients: 10 for geometry; 21 for 2-D texture; 28 for colour or for normals.
- **Bounded attributes (PDF 5):** optimal placement can leave the valid range (colours outside [0,1]). L2 clamps to the nearest valid value. Normals are renormalised.
- **Attribute weighting:** all attributes are weighted equally. "It seems doubtful that there is a single optimal weighting" (PDF 5).
- **Discontinuities (§6.1, PDF 5):** seams (for example a texture wrap) need duplicated vertices plus constraints. L2 says this "will not work for all cases" and proposes, without implementing it, several quadrics per vertex.
- Rejected alternative: a **segregated** metric (separate quadrics for position and attributes) is cheaper. It works with subset placement but poorly with optimal placement, because it misses cross-correlation (PDF 4).

**L3, Hoppe's sparse quadric (§4, PDF 3; checked on the page image).**
- Project p onto the face's plane *in R³*, not in R^{3+m}. The face error Q^f(v, s) is the geometric error plus Σ_j the attribute errors.
- **Geometric term:** the zero-extended L1 quadric, (nnᵀ, dn, d²).
- **Attribute term:** for each scalar attribute j, fit a linear function ŝ_j(p) = g_jᵀp + d_j over the face. It must interpolate the three corner values and have no component along the normal (n·g_j = 0), which gives a 4×4 linear system. The error is (ŝ_j(p) − s_j)².
- **Summed quadric:**
  - A = [[nnᵀ + Σ g_j g_jᵀ, −g₁ … −g_m], [−g_jᵀ, I]];
  - b = (dn + Σ d_j g_j, −d₁ … −d_m);
  - c = d² + Σ d_j².
- **Storage: 11 + 4m coefficients**, against (4+m)(5+m)/2 for L2 (Table 1: colour 23 vs 28; colour + normals 35 vs 55; plus texture 43 vs 78). It is faster to evaluate because the m×m block is the identity times a scalar (PDF 3).
- Per-attribute weights λ_j, or rescaling, set attribute error against geometric error (PDF 3).
- **Bounded attributes (PDF 3):** attributes are clamped to their bounds and Q re-evaluated; normals are renormalised. L3 notes that exact constrained quadratic programming would be more expensive.
- **Discontinuities (PDF 4):** use attribute *wedges*, so a vertex can hold several attribute vectors. The collapse quadric then has dimension 3 + km and never needs to be stored.
- **Related enhancements (PDF 4-6):** *memoryless* simplification (recompute quadrics from the current geometry, after Lindstrom-Turk) and *volume preservation* both improve accuracy, in that order of decreasing importance (PDF 1).
- **Failure noted (PDF 5):** on the planar mandrill test, numerical noise amplified by the shrinkage effect and memoryless re-evaluation made the geometry undulate significantly; volume preservation removed this.

## 4. Applying this to bone weights (my synthesis; not in any source)

- **Treat each bone weight as one scalar attribute.** With B bones, L2's dense quadric needs (B+5 choose 2) coefficients per vertex, which is impractical for, say, B = 30 (595). L3 needs 11 + 4B (131).
- In practice a vertex carries only k ≤ 4 non-zero weights. The union over a collapse is small, so L3's form fits.
- **Bounds and partition of unity:** weights lie in [0,1] and sum to 1. Neither L2 nor L3 enforces a sum constraint. Following their practice (clamp, then renormalise), clamp each synthesised weight to [0,1] and divide by the sum. This is the analogue of L3's normal renormalisation.
- **Rig seams:** where weights change sharply (for example between a finger and a hand), treat the change as a discontinuity and either add L1's perpendicular penalty planes along the seam or use L3's wedges.
- **Order of checks before each collapse:**
  1. link condition (R8-A4);
  2. exact orient sign per incident face (R8-A4 §3);
  3. the quadric cost.
  
  Reject on 1 or 2. Do not merely penalise, since L1 allows either.

## Disagreements
- **Projection space:** L2 projects in Rⁿ; L3 projects in R³. L3 argues its metric is more accurate and sparser. L2's metric depends more on attribute scaling, which is why it rescales to the unit cube.
- **Fold-over:** L1 allows either a penalty or a rejection; R8-A4 recommends rejection with exact predicates.

## Not obtained or not verified
- L3's printed page range is uncertain (Crossref lists "59-510"), so PDF pages are cited.
- The bone-weight application in §4 is mine and untested.
