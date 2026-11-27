# R8-A3 Offsets with round and mitre joins

Round 8, section A (soft edges). Access date for every source: **2026-11-27**. PDFs were downloaded to the box and read with poppler `pdftotext` (non-layout mode for the two-column papers). All wording is mine. No code was taken from any source.

**Question this item answers for the lane.** What is a "round" offset exactly (as a set), why do round joins at reflex corners never cross, how does the Voronoi diagram / medial axis give it, what is a mitred offset and where does it go wrong, and what is a sensible mitre limit?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Palfrader P., Held M. (2015). "Computing mitered offset curves based on straight skeletons." *Computer-Aided Design & Applications* 12(4): 414-424. doi:10.1080/16864360.2014.997637 | Publisher PDF, free to read, from the CAD&A journal site (cad-journal.net); file saved as `raw/round8/palfrader-held-2015.pdf` | Footer on every page: "© 2015 CAD Solutions, LLC". Free to read; no CC licence stated | Yes, all 11 pages. Journal page = PDF page + 413 |
| S2 | Held M., Huber S. (2009). "Topology-oriented incremental computation of Voronoi diagrams of circular arcs and straight-line segments." *Computer-Aided Design* 41(5): 327-338. doi:10.1016/j.cad.2008.08.004 | Author copy on S. Huber's site, https://www.sthu.org/research/publications/files/HeHu09.pdf | The PDF carries Elsevier's "Author's personal copy" banner (internal, non-commercial research use). **Legal status of the web posting is not clear to me**: I read it for content only | Yes, all 13 pages. Journal page = PDF page + 325 |
| S3 | Eppstein D., Erickson J. (1999), DCG 22(4) 569-592 (same as R8-A1 S3) | Author preprint via Internet Archive (see R8-A1) | Authors' preprint | Yes, the offset paragraph. Printed page = PDF page - 1 |
| S4 | W3C. *Scalable Vector Graphics (SVG) 2*, W3C Candidate Recommendation Snapshot of 06 October 2026 (the version at /TR/SVG2/ on the access date), chapter 13 "Painting", §13.5.5 (`stroke-linejoin`, `stroke-miterlimit`) and §13.5.7 (computing the stroke shape). https://www.w3.org/TR/SVG2/painting.html | Yes (W3C) | W3C permissive document licence (W3C Software and Document License), as stated in the spec's copyright line | Yes, §13.5.5 and §13.5.7. Cited by section (HTML has no pages) |
| L1 | **Listed: Held M. (1998).** "Voronoi diagrams and offset curves of curvilinear polygons." *Computer-Aided Design* 30(4): 287-300 | **No free legal copy found.** Held's old paper links (cosy.sbg.ac.at/~held/papers/...) return 404 and have no Internet Archive snapshot; ScienceDirect blocks scripted download | Elsevier | **No.** Covered only through S1 p. 415 and S2 p. 331 (both cite it as their refs [13] and [32]) |
| L2 | **Listed: Held M. (2001).** "VRONI: An engineering approach to the reliable and efficient computation of Voronoi diagrams of points and line segments." *Comput. Geom. Theory Appl.* 18(2): 95-123 | Elsevier lists CGTA back issues in its open archive, so a free publisher copy probably exists on ScienceDirect, but ScienceDirect refused scripted access, the old author link 404s, and the amherst.edu course copy now redirects | Elsevier (open archive, if confirmed) | **No.** Substitute: S2 (same group, same engine, extended to arcs) |
| L3 | **Listed: Held M., Huber S. (2009)** = S2 above | - | - | Yes |

**Substitutes used:** S2 stands in for L2 (VRONI 2001) and partly for L1 (Held 1998) on the Voronoi-offset link; S4 is added for the mitre limit, which none of the listed papers defines numerically.

---

## 1. The round offset is a level set of the distance function

**S1, p. 414 (PDF 1).** For a closed polygon P without self-intersections, the *interior offset curve at distance r* is the set of interior points whose Euclidean distance to P is **exactly r**. Every such curve is made of straight pieces parallel to input edges and circular arcs, one arc per reflex vertex, all of radius r. Because every point is at the same distance from P, S1 calls this the *rounded* or *constant-radius* offset. The definition extends to exterior offsets, polygons with holes, and one-sided offsets of open figures (same page).

In set terms (my notation): with d(x) = dist(x, P), the round offset at r is the level set {x : d(x) = r}, and the "inset region" is the sublevel/superlevel set {x in P : d(x) ≥ r}.

**What this gives for free (my inference from the definition, not a quoted theorem).**
- Two round offsets at different distances r1 ≠ r2 are disjoint, since a point has one distance.
- d is 1-Lipschitz (|d(x) - d(y)| ≤ |x - y|, triangle inequality), so consecutive offsets at distances r and r + h are at least h apart in the Hausdorff sense. Round joins therefore cannot cross each other, and an offset cannot cross itself in the sense of two pieces at different distances overlapping. It can still *touch* itself where the offset passes through a medial-axis point (a "bottleneck"); at that r the curve changes topology (splits).
- The arcs at reflex vertices are exactly the parts of the level set lying in the Voronoi cell of the reflex vertex (see §2).

## 2. Voronoi diagram and medial axis give the round offset directly

**S1, p. 414-415 (PDF 1-2).** Conventional offsetting has four steps: (1) translate each input segment to make an elementary offset segment, (2) close the gaps to form a *raw* offset, (3) compute all pairwise self-intersections of the raw offset, (4) remove the invalid loops. S1 notes that strategies exist to cut the cost of step 3 and to identify invalid loops "even if multiple intersections coincide" (that is where conventional code breaks).

Persson first sketched Voronoi-based interior offsetting; Held [L1] gave "the first thorough study". The Voronoi diagram of a polygon splits the interior into cells, one per input segment (and per reflex vertex), each holding the points closer to that site than to any other. Its edges are straight pieces and conic arcs. The **medial axis** (points with more than one closest boundary point) is a subset of the Voronoi diagram. S1 states that Voronoi diagrams are "generally uncontested" as the premier choice for offsetting in both speed and reliability, and that computing the diagram first pays off even if only a few offsets are needed (p. 415).

How the offset is read off (my explanation; S1 Fig. 1 on p. 415 draws a family of round offsets over the Voronoi cells but does not spell out the procedure): inside the cell of a segment the level set d = r is a straight piece parallel to the segment; inside the cell of a reflex vertex it is a circular arc of radius r. Walking the cells in order gives the offset without any self-intersection test, because the cells partition the interior. *This is how round joins at reflex corners avoid crossing: each arc is clipped to its own Voronoi cell.*

**S2, p. 329 (PDF 4).** Definitions used by Held's codes: *cone of influence* CI(s) of a site (whole plane for a point; the slab between the two end-normals for a segment; the wedge between the end radii for an arc); Voronoi cell VC(s, S) = closure of {q in int CI(s) : d(q, s) ≤ d(q, S)}. The *clearance disk* of a point p is the closed disk centred at p with radius d(p, S). Voronoi nodes are points where three or more Voronoi edges meet. Lemma 3 (p. 329): every bisector between proper sites is a piece of a conic (ellipse, hyperbola, parabola, with lines and circles as limits).

**S2, p. 331 (PDF 6), apex splitting.** A conic Voronoi edge may contain the apex of its conic. Held's codes insert a dummy degree-two node at the apex. Side benefit stated in the paper: after apex splitting, **the clearance (distance to the input) increases or decreases monotonically along every Voronoi edge** from one node to the other. S2 says this matters in practice for bottleneck detection and for "computing offset curves based on the Voronoi diagram [32]" ([32] is L1, Held 1998).

Consequence (my inference, labelled): with monotone clearance on every edge, the offset at distance r crosses each Voronoi edge at most once, and whether it crosses is decided by comparing r with the two end-node clearances. That makes offset extraction a clean graph traversal.

**S2, p. 327 (PDF 2) and p. 329 (PDF 4), failure/limits.** S2 explains why arcs matter: offsetting a polygon creates arcs, and users expect to feed an offset back in as input. Approximating arcs by many segments makes memory "sky-rocket" for PCB-size inputs (p. 327). Held's codes add four dummy points at a scaled bounding box so every real site has a bounded cell; the price is that the diagram is not correct over the whole plane, so **outward offsetting only works up to a distance set by the bounding-box scale factor** (p. 329, PDF 4).

## 3. Mitred offsets and the straight skeleton

**S1, p. 414 (PDF 1).** A mitred offset drops the arcs and extends the neighbouring offset segments until they meet. Each mitred segment is then at distance r from the *supporting line* of its source segment, not from the segment itself.

**S1, p. 415 (PDF 2).** The straight skeleton is the trace of the vertices of shrinking mitred offsets, so the mitred offset at distance t is exactly the straight-skeleton wavefront W(t) (see R8-A1). **S3, printed p. 13 (PDF 14)** says the same: once the skeleton is known, any mitred offset follows in linear time.

**Failure case 1: far-away mitre tips.** **S1, p. 416 (PDF 3):** mitred offset intersections at *acute angles at reflex vertices* "can be far away from their defining input". **S1, p. 419 (PDF 6):** the speed of a wavefront vertex grows without bound as the angle sharpens, so a reflex wavefront vertex traces a long arc; Fig. 2 shows one arc crossing almost the whole polygon.

**Failure case 2: conventional clipping libraries disagree.** **S1, p. 421-422 (PDF 8-9):** Clipper and GEOS compute mitred and approximately rounded offsets by Boolean clipping of elementary offset areas, apparently weeding out wrong regions with winding numbers. Their mitred offsets differ from skeleton-based ones (Fig. 8: two excess areas are not removed). S1's explanation: mitred offsetting lets vertices travel arbitrary distances, which is a different problem from rounded offsetting, and the same winding-number method is used for both. The Felkel-Obdržálek straight-skeleton algorithm "is known to be flawed" (p. 421, refs [15, 28]); S1 describes Cacciola's CGAL code as "a corrected and improved version" of it (same page; see R8-A2).

**Failure case 3: missed collapse in finite precision.** **S1, p. 418-419 (PDF 5-6):** if the three vertices of a kinetic triangle meet at one point at one time, the signed-area polynomial is a quadratic with a *double root*. A tiny rounding error can make the parabola miss the axis, so **the collapse is missed entirely**. S1's mitigations: (a) for vertex pairs, the squared distance is quadratic in time but its derivative is linear, so solve the linear equation for the time of closest approach; if the pair is a wavefront edge it must collapse then (unless the vertices run in parallel); if it is a triangulation spoke, test its length at that time and call it a multi split event when it is near zero; (b) a vertex-to-opposite-edge distance (Fig. 6, green) as an alternative clock.

## 4. Bevels and the linear axis: bounding the mitre

**S3, printed p. 13 (PDF 14).** Bevelled offsets come from the same skeleton algorithm if one adds a **zero-length edge at each reflex vertex, perpendicular to that vertex's bisector**.

**S1, p. 416 (PDF 3) and p. 419-420 (PDF 6-7), linear axis (Tănase and Veltkamp).** At a reflex vertex, insert several zero-length "hidden" wavefront edges so that the angles between consecutive edges leaving that vertex are equal. The wavefront vertices then fan out evenly. Each fanned vertex has speed **at most √2**, tending to 1 as the number of hidden edges grows. Since edges move at unit speed, **no offset vertex at orthogonal offset distance d is farther than √2 · d from the input** (p. 420). The traces of the fanned reflex vertices are left out of the linear axis itself (dotted in Fig. 3). Tănase and Veltkamp prove the linear axis converges to the medial axis as the number of hidden edges goes to infinity (p. 420). S1 adds that the √2 threshold "could also be adapted according to a user-specified parameter" (p. 416).

## 5. The mitre limit as a number

**S4, §13.5.5.** For two segments meeting at angle θ, the SVG mitre length is stroke-width / sin(θ/2). `stroke-miterlimit` is a limit on the mitre as a multiple of the stroke width; **initial value 4**; negative values are invalid; values between 0 and 1 are allowed in SVG 2 but any mitre exceeds them. SVG 2 defines mitre length from the tangents at the join, not from where the inner stroke edges meet, so curved segments behave consistently (same section, note on the "historical definition").

**S4, §13.5.7 (stroke shape algorithm).** Build the bevel triangle from the join point P and the two offset points P1, P2 on the outer side. Round join = bevel plus a circular sector of diameter stroke-width centred at P between P1 and P2. Mitre join = bevel plus the triangle P1, P2, P3 where P3 is where the outer offset lines meet; **if 1/sin(θ/2) ≤ miterlimit, use the mitre, otherwise fall back to the bevel**.

**Cross-check against the skeleton (my calculation).** The skeleton vertex at a corner of interior angle α moves at 1/sin(α/2) (R8-A1, §1). So the mitre tip at offset d sits at distance d/sin(α/2) from the corner, exactly the SVG mitre ratio. A mitre limit L therefore means "bevel when α < 2·arcsin(1/L)":

| Limit L | Bevel below corner angle |
|---|---|
| √2 ≈ 1.414 | 90.0° (this is S1's linear-axis √2 bound) |
| 2 | 60.0° |
| 4 (SVG initial value) | 28.96° |
| 10 | 11.48° |

(Values computed on the box with Python `math.asin`; not taken from a source.)

## 6. Summary for the implementation lane

- **Round offset = exact distance level set.** Generate it from a Voronoi/medial-axis structure, clipping each arc to its reflex-vertex cell; never by intersecting a raw offset. Clearance is monotone along apex-split Voronoi edges, so each edge is crossed at most once (S2 p. 331 plus my inference).
- **Mitred offset = straight-skeleton wavefront.** It can run arbitrarily far at sharp reflex corners (S1 p. 416, 419). Bound it either by a bevel (zero-length edge, S3) or by the linear axis (√2·d bound, S1 p. 420), or by an SVG-style mitre limit (S4).
- **Robustness.** Do not trust a quadratic's double root for a three-way collapse; use the linear derivative of pairwise distance (S1 p. 419).
- **Outward offsets** from a bounding-box-seeded Voronoi diagram are only valid up to a distance set by the box scale (S2 p. 329).

## 7. Not obtained or not verified

- L1 (Held 1998) and L2 (VRONI 2001) were **not read**. Statements attributed to them above come only through S1 and S2's descriptions.
- S2's web posting carries Elsevier's personal-copy banner; legal status unclear.
- The PostScript Language Reference's mitre-limit default (often quoted as 10) was **not checked**; only SVG 2's value (4) is cited.
