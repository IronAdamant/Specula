# R8-A4 Snap rounding, the link condition, the fold-over test, and orientation-preserving welding

Round 8, section A (soft edges). **The most detailed item in section A.** Access date for every source: **2026-11-27**. PDFs were downloaded to the box and read with poppler `pdftotext`; two scanned-font PDFs (Hobby, and Ruppert in R8-A7) were OCR'd with Tesseract and every formula used here was checked against a rendered page image. All wording is mine. No code was taken from any source.

**Question this item answers for the lane.** When vertices are rounded to a grid or welded together, what is guaranteed and what can go wrong? (1) What do the snap-rounding schemes promise, and where do they fail? (2) When can an edge be contracted without changing topology (the Dey-Edelsbrunner-Guha-Nekhayev link condition)? (3) How is geometric fold-over detected? (4) How do you weld or cluster vertices so that no triangle flips?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| L1 | **Listed: Guibas L., Marimont D. (1998).** "Rounding arrangements dynamically." *Int. J. Comput. Geom. Appl.* 8(2): 157-176. doi:10.1142/S0218195998000096. Conference version *Proc. 11th ACM SoCG* (1995) 190-199, doi:10.1145/220279.220300 | **None obtained.** World Scientific is paywalled; the ACM DL PDF returned HTTP 403 (bot check) to scripts; a ResearchGate copy exists but its status is unknown and it was not used | World Scientific / ACM | **No.** Its three properties are taken from S3 (de Berg-Halperin-Overmars p. 2), S2 and S2b, which all restate them |
| S1 | **Listed: Hobby J. D. (1999).** "Practical segment intersection with finite precision output." *Comput. Geom. Theory Appl.* 13(4): 199-214. Read as the author's Bell Labs technical-report version (18 pp.), file `93_2-27.pdf` from Hobby's publication page mirrored at https://9p.io/who/hobby/ (the page lists it as [Hobby93c] with the CGTA citation) | Author's own file | No licence stated | Yes, all 18 pages, by OCR (Type-3 fonts defeat pdftotext); formulas checked on page images. Page = PDF page. **The CGTA journal pagination was not checked** |
| S2 | **Listed: Halperin D., Packer E. (2002).** "Iterated snap rounding." *Comput. Geom. Theory Appl.* 23(2): 209-225 | Copy read from a University of Trier seminar page (`isr-trier.pdf`, 19 pp.). **Legal status unclear**, read for content only. CGTA back issues are in Elsevier's open archive, so the publisher copy is probably free on ScienceDirect, but ScienceDirect refused scripted access | Elsevier | Yes, all 19 pages. Page = PDF page |
| S2b | **Substitute for S2 (same work, clearly legal):** Packer E. (2002). *Finite-Precision Approximation Techniques for Planar Arrangements of Line Segments.* MSc thesis, Tel Aviv University (supervisor D. Halperin), from the TAU Computational Geometry Lab site | Institutional copy | No licence stated | Yes, chapters 2-3. Printed page = PDF page - 2 |
| S3 | **Listed: de Berg M., Halperin D., Overmars M. (2007).** "An intersection-sensitive algorithm for snap rounding." *Comput. Geom. Theory Appl.* 36(3): 159-165. Read as Utrecht tech report UU-CS-2004-055 | Utrecht University Repository, https://dspace.library.uu.nl/handle/1874/17994 | UU repository licence and re-use conditions (linked on the cover page) | Yes, all 10 pages (PDF 1 is the repository cover). Printed page = PDF page - 1 |
| S4 | **Listed: Dey T. K., Edelsbrunner H., Guha S., Nekhayev D. V. (1999).** "Topology preserving edge contraction." *Publ. Inst. Math. (Beograd) (N.S.)* 66(80): 23-45 | Publisher copy, elib.mi.sanu.ac.rs/files/journals/publ/86/n080p023.pdf. The live server returned HTTP 500 to scripts; read via the Internet Archive snapshot of 2024-08-16 of that same URL | Open access journal of the Mathematical Institute SANU; no licence text on the PDF | Yes, all 23 pages. Journal page = PDF page + 22 |
| S5 | **Substitute for the fold-over test (no listed source):** Garland M. (1999). *Quadric-Based Polygonal Surface Simplification.* PhD thesis, CMU-CS-99-105, Carnegie Mellon University | Author/CMU copy (`garland-thesis.pdf`) | No licence stated | §2.3.4 and §3.7-3.8. Printed page = PDF page - 10 |
| S6 | Supporting: Garland M., Heckbert P. S. (1997). "Surface simplification using quadric error metrics." *Proc. SIGGRAPH 97*, 209-216 (listed under B3) | Author copy (`garland-quadrics.pdf`) | ACM copyright | §3.2 valid pairs (PDF 3); "Preventing Mesh Inversion" (PDF 5). Cited by PDF page |
| S7 | Supporting: Hoppe H., DeRose T., Duchamp T., McDonald J., Stuetzle W. (1993). "Mesh optimization." *Proc. SIGGRAPH 93*, 19-26 | Author copy (`hoppe-meshopt.pdf`, 8 pp., no printed page numbers) | ACM copyright | §4.2 legal moves. Cited by PDF page |
| S8 | Supporting: Floater M. S. (2003). "One-to-one piecewise linear mappings over triangulations." *Math. Comp.* 72(242): 685-696 | AMS site, free access (https://www.ams.org/journals/mcom/2003-72-242/S0025-5718-02-01466-7/) | © AMS; free to read | Theorems 4.1-4.2 and §5. Journal page = PDF page + 684 |
| - | Not obtained: Hershberger J. (2013), "Stable snap rounding", CGTA 46(4) (SoCG 2011). Later work that addresses SR's drift. ACM/Elsevier blocked scripts | - | - | **No** |
| - | Not obtained: Goodrich, Guibas, Hershberger, Tanenbaum (1997), "Snap rounding line segments efficiently in two and three dimensions", SoCG 1997 | - | - | **No.** Known only through S3 p. 2 |

---

## Part 1. Snap rounding

### 1.1 The scheme (S3 p. 2 = PDF 3; S2 p. 2; S2b p. 10 = PDF 12)

Tile the plane with unit squares ("pixels") centred on integer points. A pixel is **hot** if it contains a vertex of the arrangement (a segment end point or an intersection point). Replace each vertex by the centre of its hot pixel. Replace each edge by the polyline through the centres of all hot pixels the edge meets, in the order it meets them. Features may collapse in the process.

**Hobby's form (S1, p. 3, checked on the page image).** Points that round to grid point (i, j) form the *tolerance square* (i, j) + R with R = {(x, y) : -1/2 ≤ x < 1/2, -1/2 ≤ y < 1/2} (half-open, so every point rounds to exactly one grid point). Find all intersection points with Bentley-Ottmann, round them, and **whenever a segment meets the tolerance square of a rounded point, bend the segment through that point** ("inserting P into s"). S1 notes the naive alternative (re-run intersection on its own rounded output until nothing new appears) works in common cases "but there is no guarantee" the iteration count is small (p. 2, Figs. 1-2).

### 1.2 What is guaranteed

**Guibas-Marimont properties, as restated by S3 p. 2 (PDF 3):**
1. **Fixed-precision representation.** All output vertices are pixel centres. (Not trivial: one must show rounding creates no *new* intersections.)
2. **Geometric similarity.** Each rounded segment s* lies inside the Minkowski sum of s and one pixel centred at the origin. So every output point is within half a pixel in x and in y (at most √2/2 pixel in Euclidean distance) of its original segment.
3. **Topological similarity.** A and A* are "topologically equivalent up to the collapsing of features": there is a continuous deformation of the segments to their rounded versions **during which no segment ever passes completely over a vertex** of the arrangement.

**Hobby's theorem (S1 §4, p. 13-14, checked on the page image).** Call a segment set *fully intersected* if distinct segments meet only at end points or not at all. Using the closed-the-other-way square R⁻ (negated R) and the rounded vertex set ⌈T⌋ (the grid points of T + R⁻), Hobby's discretisation replaces a segment by the chain of sub-segments AB whose open neighbourhood AB + R⁻ contains no rounded point other than A and B. **Theorem 4.1: if T is the set of end points of a fully intersected segment set, the discretised set is also fully intersected.** So rounding cannot create crossings that were not there.
- Lemma 4.2 (p. 13): for any segment there is a diagonal direction (1, ±1) (sign taken from the slope) in which no two of its rounded points have the same coordinate. This lets each rounded chain be treated as a function graph.
- Lemma 4.3 (p. 13-14): two segments that meet only at end points (or not at all) have discretisations whose pieces also meet only at end points (or not at all).
- **Order preservation (p. 15).** The proof shows there is no interleaving: in the rotated coordinates the vertical order of the two rounded chains is the same as for the originals. Weaker than Greene-Yao: two rounded segments may overlap along *several disjoint pieces*, whereas Greene-Yao guarantee a single point or a single polyline (p. 15).

**ISR's restatement (S2 p. 2; S2b p. 10).** SR can be viewed as continuously deforming curves so that **no vertex of the arrangement ever crosses through a curve**.

**What this means for orientation (my inference, labelled).** If a vertex never crosses a curve during the deformation, the sign of orient(segment, vertex) can go to **zero** (the vertex lands on the rounded chain, a collapse) but **cannot flip**. So snap rounding is *weakly* orientation-preserving for vertex-edge pairs: collapses yes, inversions no. Triangles whose area is non-zero after rounding keep their sign; some go to zero area. A mesh built on SR output must therefore expect and remove zero-area triangles, but need not repair inverted ones. (Not stated in this form by any of the sources.)

### 1.3 Failure cases and known weaknesses

| Weakness | Detail | Source |
|---|---|---|
| **Vertex arbitrarily close to a non-incident edge** | After SR, a rounded edge can pass through a hot pixel without going through its centre and come very near the vertex there. With b bits of output coordinates (grid 2^b × 2^b), the distance can be as small as about 1/√((2^b - 1)² + 1), roughly 2^-b, versus the pixel width 1. Construction: move one end of segment t to the corner of its pixel and push the other end far down (Fig. 2). (My note: a later floating-point pass could misjudge such a near-degenerate configuration.) S2 p. 4 notes one could keep extra topology data, but that makes the output "more cumbersome" | S2 p. 3-4 (PDF 3-4); S2b ch. 2; S3 p. 2-3 |
| **Degree-2 vertices that are not end points** | SR can put a bend in a segment where it crosses a hot pixel that holds no real vertex of that segment (Fig. 2). S3 removes them ("orange" pixels) and proves the simplified arrangement keeps all three SR properties plus *non-redundancy* (Theorem 3.1, p. 7-8 = PDF 8-9). Proof idea: the triangle q1 p q2 lies inside the Minkowski sum of an ursegment and a pixel, so no pixel centre in P lies in it, so the bend can be straightened without passing over a vertex | S3 p. 3 (PDF 4) and §3 |
| **Output complexity blow-up** | Σ over hot pixels of segments crossing them can be Θ(n³) (S3 p. 2, Fig. 1). For both SR and ISR: one chain can pass Θ(n²) hot pixels, total chain-pixel incidences Θ(n³), and the rounded arrangement (ignoring overlaps) has Θ(n²) segments | S3 p. 2; S2 Theorem 4.3, p. 9 |
| **Rounded segments can meet and part many times** | "Two rounded segments can meet and diverge many times" | S3 p. 8 (PDF 9); S1 p. 15 |
| **Wrong-file warning (process note)** | The file 93_4-13 on Hobby's page is an unrelated spline-rounding paper (Grosse-Hobby). Kept as `grosse-hobby-spline-WRONG.pdf`; not used | - |

### 1.4 Iterated snap rounding (ISR) and its drift

**Procedure (S2 p. 4-5).** Compute the hot pixels once. For each input segment, call a recursive Reroute: find the hot pixels the segment crosses in order, and if it is not already a single link between two pixel centres, recurse on each link between consecutive centres. Rerouting adds no new hot pixels (p. 4). Lemma 3.1 (p. 6): ISR equals repeated SR on its own output until nothing changes; the recursion terminates because every chain is weakly x- and y-monotone, so each hot pixel is found at most once per tree, and there are O(n²) hot pixels.

**Guarantees.**
- Corollary 3.2 (p. 7): ISR preserves topology in the same sense as SR.
- **Lemma 3.3 (p. 7): (i) any output chain that passes through a hot pixel passes through its centre; (ii) every vertex is at least 1/2 pixel from every non-incident segment.** This is the point of ISR.

**Failure: drift.**
- Lemma 3.4 (p. 7): a final chain lies in the Minkowski sum of its segment and a square of side k_i, where k_i is the recursion depth for that segment. So the approximation can be k_i times worse than SR.
- **S2b Theorem 3.8 (printed p. 21, PDF 23): the maximum distance between an input segment and its ISR chain is Θ(n²)** pixels. Upper bound: O(n²) hot pixels, each adding at most √2/2. A matching lower-bound construction is given (Fig. 3.6).
- S3 p. 8 (PDF 9): ISR "is only defined algorithmically and it may result in very poor preservation of the geometric similarity". S3 p. 3 (PDF 4): "the drift of a segment in this scheme can be very large".
- **Sources agree on the trade-off:** SR gives ½-pixel similarity but near-degenerate vertex-edge gaps; ISR gives ½-pixel vertex-edge clearance but Θ(n²) drift. Hershberger's "stable snap rounding" (2013) targets this but was not obtained.

**Running time (for reference).** ISR: O(n log n + I + L^(2/3) N^(2/3+ε) + L) with multi-level partition trees (S2 Theorem 4.2, p. 9). SR: O((n + I) log n) by S3 Theorem 2.1 (p. 6 = PDF 7).

## Part 2. The link condition (Dey, Edelsbrunner, Guha, Nekhayev)

### 2.1 Definitions (S4)

- **Star, closure, link (p. 25 = PDF 3).** For a set B of simplices: the closure is all faces of simplices in B; the star St B is all cofaces of simplices in B; the **link Lk B** is the set of faces of cofaces of B that are disjoint from B (formally the closed star minus the star). The link is always a complex; the star usually is not.
- **Order and boundaries (p. 27-28 = PDF 5-6).** The order of a simplex σ is the smallest i such that its star is combinatorially equivalent to the star of a (k - i)-simplex, k = dim St σ. Order Bound: ord σ ≤ dim St σ - dim σ. The **j-th boundary** Bd_j K is the set of simplices of order at least j; it is again a complex (Property 3, p. 43). M_j is the class of complexes with Bd_{j+1} K empty. For a triangulated 2-manifold with boundary, Bd_1 is the boundary curve and the complex is in M_1; a closed 2-manifold is in M_0. Example (p. 27-28): the "shark fin" (two disks glued along a path) has vertices of order 0, 1, 2.
- **Edge contraction (p. 29 = PDF 7).** Contracting ab replaces St a ∪ St b by the star of a new vertex c; as a vertex map a, b ↦ c and everything else fixed. An **unfolding** is a simplicial homeomorphism |K| → |L|; it is **local** if it differs from the contraction only inside the closed star of ab, **relaxed** if only inside a slightly larger neighbourhood.
- **Extended boundaries (p. 30 = PDF 8).** Bd^ω_i K = Bd_i K plus a dummy vertex ω coned to every simplex of Bd_{i+1} K. Lk^ω_i is the link inside that extended complex.

### 2.2 The Link Conditions (S4 p. 30 = PDF 8)

**Lk^ω_i a ∩ Lk^ω_i b = Lk^ω_i ab, for every i ≥ 0.**

Example (Fig. 2, p. 30): for a surface patch, if Lk a ∩ Lk b = {x, y} = Lk ab (x, y the two opposite vertices), contraction has a local unfolding. If Lk a ∩ Lk b = {x, y, z, xz} ≠ Lk ab, the contraction has **no unfolding at all**.

**Redundancy (p. 31 = PDF 9, Table 1).** For a complex in M_j there are j + 1 conditions. For i = j = d the condition is void; for i = j = d - 1 it is implied by the i = d - 2 condition. For a 1-complex (graph), contraction changes topology iff a and b have a common neighbour (a triangle of edges) or both have degree ≠ 2 (Fig. 3).

### 2.3 Theorems

| Theorem | Setting | Statement | Page |
|---|---|---|---|
| A | 1-complex G in M_1 | Lk^ω_0 a ∩ Lk^ω_0 b = ∅ ⇔ local unfolding ⇔ any unfolding | p. 32 (PDF 10) |
| **B** | **2-complex K in M_2** (any 2-complex) | ab satisfies (i.0) Lk^ω_0 a ∩ Lk^ω_0 b = Lk^ω_0 ab **and** (i.1) Lk^ω_1 a ∩ Lk^ω_1 b = ∅ ⇔ contraction has a **local** unfolding | p. 33 (PDF 11) |
| **B′** | **closed 2-manifold** (K in M_0) | **Lk a ∩ Lk b = Lk ab ⇔ local unfolding ⇔ any unfolding** | p. 36 (PDF 14) |
| C | 3-complex in M_3 | Link Conditions ⇒ relaxed unfolding (one direction only) | p. 37 (PDF 15) |
| **C′** | **closed 3-manifold** | Lk a ∩ Lk b = Lk ab ⇔ local unfolding ⇔ any unfolding | p. 40 (PDF 18) |

**Why violations are fatal on a closed surface (Theorem B′ proof, p. 36).** Let L′ = (Lk a ∩ Lk b) minus Lk ab. If L′ contains an edge xy, then triangles axy and bxy merge into one, so xy ends up in only one triangle: not a closed manifold. If L′ contains only a vertex x, then cx ends up in four triangles: not a manifold. **The 3D proof (p. 40) is the same with tetrahedra.**

**Surfaces with boundary.** The boundary curve is Bd_1 (order-1 simplices), so a bordered surface is in M_1, and Theorem B applies. Practical reading of Table 1 for d = 2, j = 1 (checked against Table 1 and its text, p. 31: for j = 1 the i = 1 condition is subsumed by i = 0, so one test remains): add a dummy vertex ω joined to every boundary edge (closing each boundary loop with a cone), then test Lk a ∩ Lk b = Lk ab in that closed-up complex. The condition (i.1) on the boundary curve itself is implied.

**S7's equivalent rules for triangle meshes (Hoppe et al., PDF p. 5, proof deferred to their tech report [6]).** Contracting edge {i, j} is legal iff: (1) every vertex k adjacent to both i and j forms a face {i, j, k}; (2) if i and j are both boundary vertices, then {i, j} is a boundary edge; (3) the mesh keeps more than 4 vertices (no boundary vertex involved) or more than 3 (if one is on the boundary). Rule (1) is the link condition for a closed surface; rule (2) is what the ω-cone adds at the boundary; rule (3) stops a tetrahedron collapsing. An edge *swap* {i, j} → {k, l} is legal iff {k, l} is not already an edge (same page).

**Limits stated by S4.**
- The theorems are **purely combinatorial/topological.** They say nothing about where c is placed or whether the embedded result self-intersects or folds. Geometry needs Part 3.
- For dimension ≥ 4 it is open whether the link condition characterises topology-preserving contractions (p. 40-41, §7).
- Irreducible triangulations (p. 41): the tetrahedron boundary is the only 2-sphere triangulation with no contractible edge; every compact 2-manifold has finitely many irreducible triangulations; but the 3-sphere has infinitely many (knotted three-edge cycles). So **repeated link-condition contraction can get stuck** with no legal edge left.

## Part 3. The geometric fold-over test

**Definition of fold-over (S5 §2.3.4, p. 24 = PDF 34, Fig. 2.12).** Two planar triangulations can cover the same region while one has a triangle "folded over" onto another; error metrics that compare geometry only cannot tell them apart. The folded one is not even a simplicial complex because the triangles overlap. For local operations (edge contraction) S5 says simple consistency checks avoid it (p. 24-25).

**Test 1: normal-flip threshold (S6 PDF p. 5, paragraph "Preventing Mesh Inversion"; S5 §3.7, p. 56 = PDF 66).** For each face around v_i and v_j, compare the normal before and after the contraction; if it changes by more than a threshold, call it flipped and either forbid the contraction or penalise it heavily. **Weakness stated by S5:** it needs a threshold value to be chosen.

**Test 2: perpendicular-plane zone (S5 §3.7, p. 56-57 = PDF 66-67, Fig. 3.15), Garland's preferred check.** For every face around v_i except the ones shared with v_j, take its edge opposite v_i and the plane through that edge perpendicular to the face. The new position v̄ must be on the same side of that plane as v_i. Same for v_j's faces. The planes bound a "zone" where v̄ may go.

**Penalise or forbid? (S5 §3.7, p. 56).** Penalising guarantees progress but a bad contraction may still happen if everything else costs more; forbidding prevents inconsistencies but can stall, with rejected contractions only re-examined when their neighbourhood changes.

**Planar (2D) form, exact (my derivation, not from a source).** In the plane the perpendicular plane through an opposite edge (p, q) is just the line pq, so Test 2 becomes: for each surviving triangle (p, q, v) in the changed fan, **sign orient2d(p, q, v̄) = sign orient2d(p, q, v)** and is non-zero. With Shewchuk's exact orient2d (R8-A5) this has no threshold and no rounding failure.

**Why the planar test is enough for one fan (my derivation, labelled).** Suppose the changed region is a fan around the new vertex v̄ whose outer boundary is a simple closed polygon Q that does not move (this is what the link condition guarantees for an interior edge on a closed or bordered surface after cone closure). If every fan triangle has positive orientation, each has an angle at v̄ in (0, π), and these angles add up to 2π times the winding number of Q about v̄. Since Q is simple, that winding number is 0 or 1; with every angle positive it must be 1, so the triangles' angular sectors cover the full turn exactly once and the fan tiles the inside of Q with no overlap. That is, v̄ lies in the kernel of Q. Conversely, if v̄ is in the kernel, all fan triangles are positive. So "all fan triangles keep positive sign" ⇔ "no fold-over, locally". For a vertex on the boundary the fan is open and the same argument needs the boundary edges kept fixed and the angle sum compared with the boundary angle; I did not work that case through.

**Global version (S8).** Floater's Theorem 4.1 (p. 691 = PDF 7; Theorem 4.2, local injectivity on each quadrilateral, same page): a convex-combination PL map of a triangulated disk that sends the boundary homeomorphically onto the boundary of a convex region is one-to-one. Section 5 (p. 694-695 = PDF 10-11) completes Theorem 4.1 by deriving global injectivity from that local injectivity, still under Theorem 4.1's hypotheses. This supports the local-to-global step, **but only under S8's hypotheses** (convex target boundary, convex-combination interior). It does not by itself cover a non-convex 1-ring.

## Part 4. Orientation-preserving welding and clustering

**What the listed sources give.**
- **Pair contraction with a distance threshold (S6 §3.2, PDF p. 3; same rule in S5 §3.9 step 1, p. 58 = PDF 68).** A pair (v1, v2) is *valid* if it is an edge **or** if |v1 - v2| < t. With t = 0 it is plain edge contraction. With t > 0 unconnected parts can join ("aggregation"), which S6 accepts because "topology is less important" for rendering. **Non-edge welding deliberately changes topology** and is not covered by the link condition at all.
- **Vertex clustering** (Rossignac-Borrel, Low-Tan) is mentioned in S5 §3.8 (p. 57 = PDF 67) as compatible with the quadric metric (sum the quadrics of all vertices in a cluster). No orientation guarantee is given.
- **Snap rounding (Part 1)** is itself a welding scheme with a weak orientation guarantee: no vertex-edge sign flips, but collapses to zero are allowed.

**A recipe that combines them (my synthesis; each step is backed by the cited result, the combination is not from any single source).**
1. Restrict welds to **edges** (or chains of edge contractions); treat a non-edge weld as an explicit topology change, if allowed at all (S6).
2. For each candidate edge ab, check the **link condition** Lk a ∩ Lk b = Lk ab, after closing boundary loops with a dummy cone vertex (S4 Theorems B, B′; S7 rules 1-3). This rules out topological change.
3. Choose the target c (midpoint, endpoint, quadric optimum, or grid point).
4. Check every triangle in the new star with **exact orient2d** for the same, non-zero sign as before (Part 3, planar form). This rules out fold-over in 2D.
5. If welding to a grid, accept zero-area results only as a deliberate collapse step and remove them, because SR-style rounding allows signs to go to zero but not to flip (Part 1).
6. Expect to get stuck: some meshes have no legal contraction left (S4 p. 41), and forbidding bad contractions can stall progress (S5 p. 56).

## Summary of key answers

- **SR guarantees (Guibas-Marimont via S3 p. 2):** pixel-centre vertices; each rounded segment within (segment ⊕ one pixel); a continuous deformation exists in which no segment passes completely over a vertex. **Hobby (S1 Thm 4.1):** no new crossings; order of chains preserved; overlaps may be several disjoint pieces.
- **SR failure:** vertex-to-non-incident-edge distance can be about 2^-b of a pixel (S2 p. 3); degree-2 junk vertices (S3); Θ(n³) incidences.
- **ISR:** ½-pixel vertex-edge clearance (S2 Lemma 3.3) but Θ(n²) drift (S2b Thm 3.8).
- **Link condition:** Lk a ∩ Lk b = Lk ab, exactly characterising topology-safe edge contraction on closed 2- and 3-manifolds (S4 Thms B′, C′); for bordered surfaces use the ω-cone or Hoppe's three rules (S7).
- **Fold-over:** normal-flip threshold (S6, S5) or Garland's perpendicular-plane zone (S5 p. 56-57); in 2D this is an exact orient2d sign check (my derivation).

## Not obtained or not verified

- L1 Guibas-Marimont 1998: not read. Its properties are cited second-hand (S3, S2).
- Hershberger 2013 "Stable snap rounding" and Goodrich et al. 1997: not read.
- S2's Trier copy: legal status unclear (S2b used as the clearly legal substitute).
- Hobby's journal pagination: not checked; tech-report pages used.
- The planar fold-over sufficiency argument and the orientation reading of SR's topological similarity are **my own derivations**, flagged as such.
