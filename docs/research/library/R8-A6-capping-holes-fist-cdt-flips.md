# R8-A6 Capping a figure with holes: FIST ear clipping, constrained Delaunay, and why Lawson flips terminate

Round 8, section A. Access date for every source: **2026-11-27**. PDFs downloaded to the box and read with poppler. All wording is mine; no code was taken from any source.

**Question this item answers.** (1) How does FIST bridge holes? (2) How does FIST classify ears robustly, especially with collinear and coincident points (where the engine's cutter had three faults)? (3) What alternatives does constrained Delaunay offer? (4) Why does Lawson's edge flipping terminate?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| L1 | **Listed: Held M. (2001).** "FIST: Fast industrial-strength triangulation of polygons." *Algorithmica* 30(4): 563-596. doi:10.1007/s00453-001-0028-4 | **Not obtained.** Springer is paywalled. The author's project page (https://www.cosy.sbg.ac.at/~held/projects/triang/triang.html, read 2026-11-27) has the preprint link `papers/algo01.ps.gz` commented out in its HTML, and that URL returns **HTTP 404**. The Internet Archive refused TLS connections from the box during this session (repeated retries) | Springer | **No.** See Sub-1 |
| Sub-1 | **Substitute for L1 (same author, same code, earlier paper):** Held M. (1998). "Efficient and reliable triangulation of polygons." *Proc. Computer Graphics International (CGI'98)*, Hannover, June 1998, 633-643 (IEEE), doi:10.1109/CGI.1998.694322 (pages and DOI confirmed via Crossref). Author's preprint `cgi98.ps.gz`, 11 pp., (its link on the FIST project page is also commented out); read from the Internet Archive snapshot of 2011-11-08 of http://www.cosy.sbg.ac.at/~held/papers/cgi98.ps.gz, converted with ps2pdf | Author's preprint | No licence stated; © IEEE for the proceedings version | Yes, all pages. Cited by preprint page (= PDF page). **Proceedings pagination not checked.** Thin on hole bridging (it says only that FIST was "extended to handle multiply-connected polygons", p. 2), hence Sub-2 |
| Sub-2 | **Substitute for L1's hole bridging:** Eberly D. (2002, last modified 3 Aug 2024). "Triangulation by Ear Clipping." Geometric Tools, https://www.geometrictools.com/Documentation/TriangulationByEarClipping.pdf | Publisher (author) site | **CC BY 4.0** (stated on the document) | Yes, all pages. Page = PDF page |
| L2 | **Listed: Shewchuk J. R. (1996).** "Triangle: Engineering a 2D quality mesh generator and Delaunay triangulator." In *Applied Computational Geometry* (WACG/FCRC '96), LNCS 1148, Springer, 203-222. Author copy https://people.eecs.berkeley.edu/~jrs/papers/triangle.pdf | Author's copy | No licence stated (© Springer) | Yes, all 10 pages. Cited by PDF page. **LNCS pages not mapped** |
| L3 | **Listed: Shewchuk J. R. (2008).** "General-dimensional constrained Delaunay and constrained regular triangulations, I: Combinatorial properties." *Discrete & Computational Geometry* 39(1-3): 580-637. Author preprint dated 9 July 2007, 58 pp. (`gdcdt1.pdf` on his site) | Author's copy | No licence stated (© Springer) | §1-2 and Theorems 13 and 32. Printed page = PDF - 4 |
| L4 | **Listed: "a proof that Lawson's edge flips terminate."** Lawson's original: C. L. Lawson (1977), "Software for C¹ surface interpolation," in *Mathematical Software III* (J. R. Rice, ed.), Academic Press, 161-194 | **Not obtained** (book chapter; no free copy found) | - | **No.** See Sub-3 |
| Sub-3 | **Substitute for L4:** Shewchuk J. R. (2012). *Lecture Notes on Delaunay Mesh Generation.* UC Berkeley, 5 Feb 2012. https://people.eecs.berkeley.edu/~jrs/meshpapers/delnotes.pdf. It credits the flip algorithm to Lawson [73] and gives a full termination proof | Author's copy | No licence stated | §2.3-2.6 and §2.8. Printed page = PDF - 4 |

## 1. Ear clipping: definitions and the basic algorithm (Sub-1 p. 2)

**Definitions (P a simple polygon, counter-clockwise).**
- A vertex is **convex** if its interior angle is below 180°, otherwise **reflex**. A reflex vertex with an angle of exactly 180° is "tangential".
- v_i v_k is a **diagonal** if the open segment lies in P's interior.
- Three consecutive vertices v_{i-1} v_i v_{i+1} form an **ear** if v_{i-1} v_{i+1} is a diagonal.
- The **cone** C(v_{i-1}, v_i, v_{i+1}) at a convex v_i is the open set strictly right of line v_i→v_{i-1} and strictly left of v_i→v_{i+1}. For a reflex v_i it is the complement of the other cone and the two rays.

**Two equivalent ear tests (both necessary and sufficient), Facts 2.1 and 2.2:**

| Condition | CE1 | CE2 |
|---|---|---|
| (1) | v_i convex | v_i convex |
| (2) | v_{i-1} ∈ C(v_i, v_{i+1}, v_{i+2}) and v_{i+1} ∈ C(v_{i-2}, v_{i-1}, v_i) | the same |
| (3) | segment v_{i-1} v_{i+1} meets no polygon edge except at its own end points | the **closed** triangle v_{i-1} v_i v_{i+1} contains **no reflex vertex** |

**Cost (p. 2-3).** Condition (3) costs O(k) per test for CE1 and O(r) for CE2 (k vertices, r reflex). The whole algorithm is O(n²) with CE1 and O(n·r) with CE2.

**Speed-ups (p. 3-4).** A bounding-volume tree or a regular grid prunes the test. FIST registers items "close" to a cell boundary in both cells, to "err on the safe side". The grid with CE2 was fastest. Deleting reflex vertices that have become convex "may help" find ears in self-overlapping polygons.

## 2. Robustness in FIST (Sub-1 §2.2 and §4)

### 2.1 One predicate, evaluated consistently (p. 3)

1. **Sort the vertices lexicographically. Discard exact duplicates** (identical coordinates), so coincident vertices share one index.
2. The **only geometric primitive** is "is v_i left of, on, or right of the oriented line v_j v_k", i.e. the sign of a 3×3 determinant.
3. **Consistency rule:** always evaluate the determinant with its three vertices in increasing index order. If that reorders the cyclic order, flip the sign of the result. So the same triple always gives the same answer, wherever it is asked.
4. The sign is decided against a **user-specified epsilon**. Held states that numerical error can give a wrong −1/0/+1, "but it will be consistent throughout the entire algorithm".

**What the source does not do:** it does not use exact arithmetic. It argues (p. 4) that exact arithmetic is not enough for real data anyway:
- vertices may truly lie on other edges, and angles may truly be 0° or 360°;
- symbolic perturbation can turn a barely legal polygon into a clearly non-simple one.

The remedy, it says, is an algorithm that copes with any input.

**My note:** combining Held's index-ordered evaluation with Shewchuk's exact orient2d (R8-A5) gives consistency **and** correctness. Held's epsilon gives consistency only.

### 2.2 Orientation and the outer loop (p. 4, §4.1)

- **Orientation of each loop comes from its signed area**, not from a local test. Signed area is unaffected by degeneracies and gives a sensible answer for a figure-eight: the larger lobe wins.
- **The outer loop is the one with the largest absolute area.** This is exact for clean multiply-connected input and "a reasonable guess" otherwise.
- Held reports that **local tests failed** on slightly messed-up data, and that the global method is **not guaranteed** for every twisted input.

### 2.3 Classifying angles 0°, 180°, 360° (p. 4-5, §4.2)

- When the determinant at v_i is (nearly) zero, the angle is 0°, 180° or 360°.
- **180°**: the vectors to v_{i-1} and v_{i+1} point in opposite directions.
- **0° versus 360°** (vectors in the same direction) **cannot be decided locally.** FIST walks away from v_i clockwise and counter-clockwise until it finds two edges that do not overlap, then classifies by "a lengthy case analysis" (not spelled out in Sub-1).

### 2.4 Ears with collinear and coincident points (p. 5, §4.3, Fig. 1)

**Degenerate ears are accepted and clipped at once:**
- if v_{i-1} = v_i or v_i = v_{i+1} (equal index);
- or if v_{i-1} lies on edge v_{i+1} v_{i+2} (possibly equal to v_{i+1} or v_{i+2}), even though that breaks the cone condition;
- symmetrically for v_{i+1} on edge v_{i-2} v_{i-1}.

These bypass the random or sorted ear choice. This "preferential treatment" is also what makes the coincident-vertex rule below safe.

**Reflex vertex v_k on the boundary of a candidate ear:**
- reject v_{i-1} v_{i+1} as a diagonal if v_k is in its relative interior (Fig. 1a);
- the triple is not an ear if v_k lies in the relative interior of edge v_{i-1} v_i or v_i v_{i+1} (Fig. 1b).

**Coincident v_k = v_i (two polygon vertices at one point), Fig. 1c-d:**
- Testing segment v_{i-1} v_{i+1} against edges v_{k-1} v_k and v_k v_{k+1} can help, but fails when those edges are collinear with the ear's sides.
- Then FIST computes two signed areas: A_ik of the sub-polygon v_i, v_{i+1}, ..., v_k, and A_ki of v_k, v_{k+1}, ..., v_i. **The triple is rejected as an ear only if both A_ik and A_ki are positive.**
- **Known false acceptance (stated by Held):** if A_ki = 0 the rule wrongly accepts the case of Fig. 1c. Held argues this is harmless:
  - a non-twisted polygon has zero area only if it collapses to a chain, which has a 0° vertex;
  - degenerate ears are clipped first;
  - so that sub-polygon is gone before the bad "ear" is considered.

**My mapping to the engine's three faults (inference).** The three classic failure points are:
1. ears whose closed triangle has a reflex vertex **on** its boundary (CE2's "closure");
2. coincident vertices from bridges or welds (the A_ik / A_ki rule);
3. 0°/360° angles where a local test cannot decide (the walk in §2.3).

A cutter that uses open-triangle tests, compares coordinates rather than indices, or classifies angles locally will fail in exactly these places.

### 2.5 Recovery when no ear is found (p. 2 and p. 5-6, §4.4)

Multi-phase, more aggressive at each step:
1. **Re-classify all vertices.** Reflex vertices may have disappeared.
2. **Look for a valid diagonal** to split the polygon. All three checks are needed:
   - (1) it is locally inside the polygon;
   - (2) its open segment meets no edge;
   - (3) **the winding number of the polygon about its midpoint is 1.**
   Without (3) a self-overlapping polygon can be split into "twisted" sub-polygons (Fig. 2a, diagonal v5 v9).
3. **Local self-intersection fix.** If edges v_{i-1} v_i and v_{i+1} v_{i+2} cross in their interiors, clip triangle v_i v_{i+1} v_{i+2} and then v_{i-1} v_i v_{i+2}. This produces two overlapping triangles **on purpose**: inserting the crossing point instead would leave a gap in a 3D surface (Fig. 2b).
4. **"Desperate mode."** Clip a random convex vertex; if none exists, a random reflex one. Leave this mode as soon as possible.

**Failure case, stated by the source:** with defective input the output is not a valid triangulation. It "degrades gracefully" (p. 2).

### 2.6 Ear choice and slivers (p. 4, §3.2)

- **Sequential mode** clips neighbouring ears in turn, so a convex polygon becomes a fan.
- **Random mode** picks any ear.
- **Sorted mode** keeps ears in a priority queue and clips the "nicest" (closest to equilateral) first.

Random and sorted modes reduce slivers at a small cost in time.

## 3. Hole bridging (Sub-2, Eberly; FIST's own method is in L1, not read)

**Idea (Sub-2 p. 6-8).** Join a hole to the outer boundary by a two-way "bridge" edge between mutually visible vertices. The result is one **pseudosimple** polygon: two coincident edges in opposite directions, and bridge end vertices repeated. Then ear-clip it.

**Finding a mutually visible pair (Sub-2 p. 8-10, six steps):**
1. Take M, the hole vertex with maximum x.
2. Cast the ray M + t(1, 0). Intersect it only with directed outer edges V_i V_{i+1} that have M on their left, with V_i on or below the ray and V_{i+1} on or above it. Keep the nearest hit I (smallest positive t). The one-sided rule matters once earlier bridges exist (see below).
3. If I is a vertex, it and M are mutually visible.
4. Otherwise let P be the end point of the hit edge with larger x.
5. If no reflex outer vertex (other than P) lies inside triangle M I P, then M and P are visible.
6. Otherwise choose, among the reflex vertices inside the triangle, the one R that **minimises the angle between M→I and M→R**.

**Failure case of a tempting variant (Sub-2 p. 10-11, Fig. 15).** Choosing the reflex vertex **nearest** to M (suggested to Eberly in 2021) is wrong. Lalish's 2023 counterexample:
- outer polygon (0,5), (9,5), (7,4), (8,4), (10,0), (10,8), (0,8);
- hole (1,6), (1,7), (2,6);
- M = (2,6), I = (10,6), P = (10,0).

The nearest reflex vertex (8,4) is not visible from M; the minimum-angle one (9,5) is.

**Multiple holes (Sub-2 p. 11).**
- Process holes in decreasing order of their maximum x, each time merging into the current pseudosimple outer polygon.
- Bridges appear in the outer polygon as both V_i V_{i+1} and V_{i+1} V_i; the step-2 rule ensures at most one of the pair is a candidate.

**Reported problems and preconditions (Sub-2 p. 12-13).**
- Preconditions: holes strictly inside the outer polygon, holes not overlapping, and **no shared vertices**. Meeting them needs pre-processing.
- A 2019 report: failures when two holes touch at a vertex because of floating-point rounding. Eberly's code allows rational (exact) arithmetic to avoid this.
- A 2023 report claimed bridges sharing an outer vertex must be sorted. Eberly argues they sort themselves: the repeated vertex has different array positions in the pseudosimple polygon, and the left-of-edge rule picks the right copy. He walks through examples, while saying "perhaps my analysis is incorrect".
- **Apparent typo in Sub-2 (my reading, checked on the page image of p. 13):**
  - Equation (4) lists the merged polygon as {0,1,5,4,6,5,1,8,7,9,10,11,2,3}.
  - Following equation (3) and the bridge rule, it should be {0,1,5,4,6,5,1,8,7,9,8,1,2,3}.
  - The text also says "0 ≤ j ≤ 13" though only vertices 0-9 exist; the listed triangles use only 0-9.
  - The algorithm is unaffected, but anyone transcribing the example should know.

**Alternative from L2 (Triangle, p. 4): no bridges at all.**
- Build a constrained Delaunay triangulation of all boundary segments (outer and hole loops).
- Then delete triangles by a depth-first "triangle-eating virus" planted at a user-given point in each hole (and from the outside), stopped by segments.
- Shewchuk notes this avoids needing oriented curves with well-defined insides.
- Triangle also deletes these triangles **before** refinement, to avoid over-refinement caused by small features outside the domain (p. 5).

## 4. Constrained Delaunay triangulation (L2, L3, Sub-3)

**Definitions (L3 printed p. 1-2 = PDF 5-6; Sub-3 §2.8, printed p. 36-42 = PDF 40-46, visibility is Definition 36 at printed p. 40).**
- A PSLG / PLC is the input vertices and segments.
- A triangle is **constrained Delaunay** if its interior meets no segment and its circumcircle encloses no vertex that is **visible** from the triangle's interior. Segments block visibility.
- A 2D CDT **maximises the minimum angle** among all triangulations that contain the segments (L3 PDF 6; Sub-3 Theorem 16, printed p. 42).

**Existence.**
- Every PLC in 2D has a CDT (Sub-3 Corollary 15, printed p. 42).
- In higher dimensions a CDT need not exist. L3's **CDT Theorem (Theorem 32, PDF 54):** a "weakly ridge-protected" PLC has one. This is relevant only for 3D capping.

**Constrained Delaunay Lemma (Sub-3 Theorem 13, printed p. 41-42 = PDF 45-46; L3 Theorem 13, PDF 34).** A triangulation of a PLC is a CDT iff every edge that is not a segment is locally Delaunay.

**In Triangle (L2 p. 3-4).**
- Segments are inserted either by repeated midpoint splitting with Lawson insertion (conforming Delaunay, new vertices) or, by default, as a CDT (delete overlapped triangles and re-triangulate each side; no new vertices).
- Constrained edges are never flipped.
- Triangle can detect segment intersections and insert vertices there (p. 4).

## 5. Why Lawson's flip algorithm terminates (Sub-3 §2.5, printed p. 32-34 = PDF 36-38)

**The algorithm.** Repeatedly find an edge that is not locally Delaunay and flip it.

**Lemma 4 (printed p. 32).** Every edge is either locally Delaunay, or flippable with the new edge locally Delaunay. So an edge that is not locally Delaunay can always be flipped, and the flip fixes it.

**Theorem 5 (printed p. 33): for n points, the flip algorithm stops after O(n²) flips with a Delaunay triangulation.** Proof idea, in my words:
1. Lift every vertex (x, y) to (x, y, x² + y²) on the paraboloid. The triangulation becomes a surface T⁺ in 3D.
2. By Lemma 1 (printed p. 29: a point is inside a circle iff its lift is below the plane of the lifted circle), an edge that is not locally Delaunay lifts to a reflex edge.
3. Flipping a reflex edge replaces the upper two faces of the tetrahedron spanned by the four lifted points with its lower two faces. In effect, the tetrahedron is glued to the underside of T⁺.
4. **So each flip moves the lifted surface strictly downward. A flipped-away edge can never come back**, because returning would require moving up.
5. There are at most n(n - 1)/2 possible edges, so there are at most that many flips.
6. When no flip is possible, all edges are locally Delaunay. By the Delaunay Lemma (Theorem 3, printed p. 30-31) the result is Delaunay.

**Supporting facts.**
- **Lemma 8 (printed p. 34):** each flip increases the minimum angle and decreases the largest circumcircle among the changed triangles (Thales). Lawson's original argument was based on max-min angle; the lifting proof is Shewchuk's presentation (the attribution is in Sub-3 printed p. 35 = PDF 39).
- **Implementation (printed p. 34):** keep a queue of edges that might be bad, starting with all of them. Pop one; if it still exists and is not locally Delaunay, flip it and push its four quadrilateral edges. Running time O(n + k), with k flips and k ∈ Θ(n²) in the worst case. Fast in practice when points move only slightly.
- **Constrained version (Sub-3 Theorem 14, printed p. 42):** never flipping a segment, the same procedure ends after O(n²) flips with a CDT. Proof "straightforward extension", omitted by the source.

**Failure cases (my inferences, labelled).**
- The proof needs **strict** tests: an edge is flipped only when the fourth point is **strictly** inside the circumcircle. Cocircular quadrilaterals are left alone; flipping them would not lower the surface and could cycle forever.
- With an **inexact** in-circle test the "always lowers" argument fails, and the loop can cycle. Shewchuk reports a non-robust Delaunay code that never terminated (R8-A5, S1 Table 9; S2 Fig. 1).
- Use the exact INCIRCLE (R8-A5) with the strict test.

## Summary of key answers

- **FIST ear test (CE2):** convex apex, in-cone conditions, and **closed** triangle free of reflex vertices.
- **FIST robustness:**
  - lexicographic sort with duplicate removal, so indices identify coincident points;
  - one orient predicate evaluated in canonical index order (consistent, but epsilon-based);
  - signed area for orientation and outer loop;
  - 0°/360° angles classified by walking, not locally;
  - degenerate ears clipped first;
  - coincident apex resolved by the two sub-polygon areas;
  - recovery chain ending in "desperate mode".
- **Hole bridging (Sub-2):** max-x hole vertex, ray to +x, nearest proper hit; if blocked, the **minimum-angle** reflex vertex in M I P (not the nearest one). Holes are processed by decreasing max x. Preconditions: no touching holes.
- **Bridge-free alternative:** CDT plus hole-seed flood removal (Triangle).
- **Flip termination:** the lifted surface strictly drops with each flip, so ≤ n(n - 1)/2 flips; also for CDT. This needs a strict, exact in-circle test.

## Not obtained or not verified

- **L1 (FIST 2001):** not read. Its hole-bridging method in particular is unknown to me. Sub-1 (CGI'98, same author and code) and Sub-2 (Eberly) are substitutes.
- **L4 (Lawson 1977):** not read. Sub-3 is the substitute.
- **Page mapping:** LNCS pages for L2 and CGI proceedings pages for Sub-1 were not mapped.
