# R8-B2. Meshing once, well

Round 8, section B. Prepared 2026-11-27; access date 2026-11-27.

**The question.** Which method guarantees a closed, manifold mesh, and under what condition? And what does each do when two sheets of the surface pass through one cell?

## Sources

| # | Citation | Free legal copy | Licence | Read? / page scheme |
|---|---|---|---|---|
| L1 | Ju T., Losasso F., Schaefer S., Warren J. (2002). "Dual contouring of Hermite data." *ACM Trans. Graph.* 21(3) (Proc. SIGGRAPH 2002): 339-346. doi:10.1145/566654.566586 | Author copy: https://www.cs.wustl.edu/~taoju/research/dualContour.pdf | Author copy; © ACM | Yes. Printed = PDF + 338 (from the page footers) |
| L2 | Schaefer S., Ju T., Warren J. (2007). "Manifold dual contouring." *IEEE TVCG* 13(3): 610-619. doi:10.1109/TVCG.2007.1012 | Author copy: https://people.engr.tamu.edu/schaefer/research/dualsimp_tvcg.pdf | Author copy; © IEEE | Yes. The copy has its own page numbers 1-10, so **PDF pages** are cited |
| L3 | Chernyaev E. V. (1995). "Marching Cubes 33: construction of topologically correct isosurfaces." CERN report CERN-CN-95-17 (presented at GRAPHICON '95) | CERN Document Server: https://repository.cern/records/7zfxg-q0t96 (file cn-95-017.pdf) | CERN institutional repository; no licence stated | Yes. It is a scanned and **OCR'd** report, so **PDF pages** are cited. Formula fragments were taken only where the OCR is clean |
| L4 | Lewiner T., Lopes H., Vieira A. W., Tavares G. (2003). "Efficient implementation of Marching Cubes' cases with topological guarantees." *Journal of Graphics Tools* 8(2): 1-15. doi:10.1080/10867651.2003.10487582 | Author copy: https://thomas.lewiner.org/pdfs/marching_cubes_jgt.pdf | Author copy | Yes. PDF pages (8) |
| Sub-1 | **Later bug report on L3/L4 (added for "known bugs"):** Custódio L., Etiene T., Pesco S., Silva C. T. (2013). "Practical considerations on Marching Cubes 33 topological correctness." *Computers & Graphics* 37(7): 840-850. doi:10.1016/j.cag.2013.04.004 | Author copy: https://www.sci.utah.edu/~etiene/pdf/mc33.pdf | Author copy | Yes. PDF pages |

## 1. Marching Cubes and its ambiguity (L3)

- The original MC has 15 cases and tiles each with one fixed topology. **Holes** can open on a face shared by two cells, for example configuration 6 next to the complement of configuration 3 (PDF 3, Fig. 2).
- Inside a cell the field is taken as **trilinear** (eq. 1, PDF 3). Honouring every topology that the trilinear function permits needs **33** table entries instead of 15 (PDF 3).
- **Face ambiguity (PDF 4):**
  - A face is ambiguous when its diagonal corners carry the same sign, so all four edges are crossed and the contour on the face is a hyperbola (after Nielson and Hamann).
  - With corner values A, C positive and B, D negative, compare the iso-value with the bilinear value at the asymptotes' crossing, (AC − BD)/(A + C − B − D).
  - For iso-value 0 this reduces to: **AC > BD means the positive corners are joined**; otherwise they are separated. The denominator is always positive (PDF 4).
- **Internal ambiguity (PDF 4-5):**
  - Two same-sign regions can be separated on every face yet joined *inside* the cell. Configuration 4 is the typical example: it has no ambiguous face.
  - Chernyaev gives two tests:
    - compare the projected hyperbolas on opposite faces;
    - **slice method** (three steps): on planes parallel to a face, the face-test expression is a quadratic a·t² + b·t + c in the height t. The positive regions are joined only if a < 0, the vertex t_max = −b/(2a) lies in (0, 1), and the slice at t_max has the right corner signs and passes the face test.
- **Two sheets in one cell:** MC33 resolves the faces first, then the interior, and picks the subcase that tiles the cell with the trilinear topology. That is either two separate sheets or a tunnel joining them (cases 3, 4, 6, 7, 10, 12, 13 have subcases; L4 PDF 5).

## 2. L4: a full implementation of MC33

- L4 says Chernyaev's internal-ambiguity method "was not complete". L4 completed it and **computed and tested the 730 subcases** of the enhanced table ("was not complete": PDF 3; 730 subcases: PDF 2).
- L4 claims a result "guaranteed to be a manifold surface, with no crack, with the topology of the trilinear interpolation of the scalar field over each cube" (PDF 2).
- Resolving face ambiguities alone prevents cracks but **not** wrong topology. Fig. 4 shows two trilinear tilings of case 6 with the same face resolution (PDF 3).
- A test table records, for each ambiguous case (3, 4, 6, 7, 10, 12, 13), which faces to test (PDF 5).

**Known bugs (Sub-1).** The 2013 paper lists four issues in L3 and/or L4:
- **Issue I, case 13.5 (PDF 5):** the saddle-sign function along t really changes sign three times (two roots plus a pole), but MC33's quadratic can follow only two. So "Chernyaev's interior ambiguity test does not always yield topologically correct isosurfaces". The tunnel orientation in case 13.5.2 is also left unspecified (§5.1.1, PDF 6).
- **Issue II, non-manifold output (PDF 6-7):** Chernyaev's *triangulation table* lets faces lie coplanar with the voxel faces. When two neighbouring voxels both hold tunnels and share an ambiguous face, the result has **non-manifold edges** (Fig. 8). This happens for any method using that table, however well it picks cases. It was earlier noted by Lopes and Brodlie.
- **Issue III (PDF 6-7):** L4's cutting-plane height can miss the tunnel (Fig. 9, case 6).
- **Issue IV (PDF 7):** a missing step in the implementation mis-resolves cases 10 and 12.
- Sub-1 also cites Etiene et al.'s finding that L4's implementation "fails to produce topologically correct isosurfaces" (PDF 1).

**So L4's manifold guarantee does not hold as published.**

## 3. Dual contouring (L1)

**Vertex placement (pp. 340-342 = PDF 2-4).**
- Each sign-change edge carries Hermite data: an exact crossing p_i and its normal n_i.
- Each cell's vertex minimises the QEF E[x] = Σ (n_i·(x − p_i))² (eq. 1, PDF 3 = p. 341).
- The algorithm has two steps:
  1. Put one vertex in every cell with a sign change.
  2. For every sign-change edge, emit a quad joining the vertices of the four cells around it (p. 341 = PDF 3).
- **Numerics (p. 341-342 = PDF 3-4):**
  - The normal-equations form (AᵀA, Aᵀb, bᵀb; 10 floats) is **numerically unstable**. On a 256³ grid, bᵀb ≈ 10⁶, so single-precision error is of order 1 on a flat area.
  - L1 instead stores a QR form.
  - Near-coplanar normals make the matrix nearly singular, so the minimiser "may lie far outside the defining cube". The fix is an SVD pseudo-inverse with singular values **below 0.1 truncated**, solved so as to stay near the centroid of the p_i (p. 342 = PDF 4).

**Closed surface (p. 344 = PDF 6).**
- On an octree, polygons are generated only on **minimal edges** (edges that do not properly contain a neighbour leaf's edge).
- The resulting mesh is **always closed** for any simplified octree. Every mesh edge lies in an even number of polygons, because the boundary of a shared face is a closed loop with an even number of sign changes.
- **Even, not two:** L1 says octrees can be built in which **6 or more polygons share one edge** (p. 344). So "closed" here does not mean "manifold".
- This rule needs no restricted octree and no crack patching. In contrast, the earlier octree method it compares against "occasionally yields contours with cracks" (p. 344).

**Two sheets in one cell (p. 345 = PDF 7).**
- DC puts **one** vertex per cell, so two sheets crossing a cell are merged into one vertex.
- The dual contour is **non-manifold** exactly for the sign configurations that are ambiguous in cube-based methods (p. 345). L2 makes the same point: "for all of the ambiguous sign configurations" (L2 PDF 2).
- **Topology-safe simplification test (p. 345):** collapse eight children into one cell only if:
  1. the coarse dual contour is manifold;
  2. each fine cell's contour is manifold;
  3. the fine and coarse contours are topologically equivalent on every sub-face.

## 4. Manifold dual contouring (L2)

**Uniform grid, two sheets (PDF 2).**
- Follow Nielson's **Dual Marching Cubes**: put one vertex per *cycle* of a modified MC table, so a cell crossed by two sheets gets two vertices. Each edge belongs to exactly one vertex.
- The quad for an edge joins the vertices associated with that edge in the four cells.
- The result is the dual of the MC surface, so it is **always manifold**, because MC's own output is.
- Hermite data enters through one QEF per vertex.

**Octree simplification (Proposition 1, PDF 5; proof in Appendix I).** Let C_v be v's cell and S_v the fine surface clustered into v. The adaptive output is a **closed 2-manifold if, for every vertex v**:
1. χ(S_v) = V − E + F = 1; and
2. S_v meets the four edges of each face of C_v in 0 or 2 points.

Notes on the criterion:
- The conditions are **sufficient, not necessary**.
- Condition 1 also forces genus 0, so the genus is preserved.
- The criterion places no limit on crossings per cell edge, because several clustered vertices can share a cell.
- Fig. 5 gives the counter-examples: a sphere (χ = 2), a cylinder (χ = 0), and a half-cylinder with four crossings on one face.
- Thirteen numbers per vertex (χ and 12 edge counts) are carried up the tree recursively (PDF 5).
- The cost of the check is small: for example, clustering took 10.99 s with the criterion against 10.89 s without, on the largest model in Table I (PDF 7).

**Failure case L2 admits (PDF 8).** The output is topologically manifold but **may still contain intersecting polygons**:
- multiple vertices in one cell can be placed by the Hermite data so that the sheets cross inside the cell;
- this can also happen with one vertex per cell (Ju and Udeshi);
- "the original DC algorithm as well as its variants are all subject to such geometric errors".

## 5. Comparison table

| Method | Closed? | Manifold? | Condition | Two sheets in one cell | Known failures |
|---|---|---|---|---|---|
| MC (15 cases) | can crack | - | - | one fixed tiling | holes on ambiguous shared faces (L3 PDF 3) |
| MC33 (L3) | yes, given consistent face tests | intended | trilinear cell field; correct face and interior tests | face test (AC vs BD) then interior test, then subcase tiling: separate sheets or a tunnel | case 13.5 test wrong; non-manifold triangulation table (Sub-1) |
| L4 (MC33 implementation) | yes | claimed | as MC33 | 730-subcase table | non-manifold edges from coplanar faces; cases 10/12 bug; cutting-plane error (Sub-1) |
| DC (L1) | **yes, always** (minimal-edge rule) | **no**: an edge may have 4, 6 or more polygons | - | merged into one vertex, so non-manifold | QEF minimiser outside the cell; self-intersection |
| DC with safe simplification (L1 §4) | yes | keeps the fine grid's topology | 3-check test | as DC | as DC |
| Manifold DC (L2) | yes | **yes** | Prop. 1 (χ = 1 and 0/2 face crossings) at every clustered vertex | one vertex per MC cycle | intersecting polygons possible (PDF 8) |

**For the engine's own answer (my comparison, not from a source).** The listed sources give a manifold guarantee only through MC-style connectivity: the MC table (L2's dual), or a tested MC33 table (keeping Sub-1's corrections in mind). No method guarantees freedom from self-intersection without extra geometric tests ([28] Ju and Udeshi, cited by L2, was not read). A mesher that keeps one vertex per sheet per cell and checks χ = 1 plus 0/2 face crossings before clustering matches L2's sufficient condition.

## Disagreements
- **L4 vs Sub-1:** L4 claims manifold output with the trilinear topology; Sub-1 shows non-manifold edges and wrong topology in specific cases.
- **"Closed" in L1 vs "manifold" in L2:** L1's guarantee is an even number of polygons per edge, which L2 counts as non-manifold.

## Not obtained or not verified
- Ju and Udeshi (intersection-free contouring) and Nielson's Dual Marching Cubes: cited only.
- L3 is OCR'd. Its equations were read only where the OCR is clean, and its case figures were not inspected one by one.
