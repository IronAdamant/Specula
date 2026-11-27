# R8-A2 Computing the straight skeleton robustly

Round 8, section A. Access date for every source: **2026-11-27**. All PDFs were read with poppler on the box. "PDF p." is the page in the downloaded file; "p." is the printed page where the copy has one. Everything below is in my own words. No code was taken.

**Question for the lane.** How does each method order events that tie within rounding? How does each handle a reflex vertex hitting a vertex instead of an edge, and collinear or parallel edges? What must be exact and what may be approximate? What bounds on the work are stated?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| FO | Felkel P., Obdržálek Š. (1998). "Straight skeleton implementation." *Proc. SCCG '98* (Budmerice), ed. L. Szirmay-Kalos, pp. 210-218 | **No confirmed legal copy.** The authors' original URL `cgg.cvut.cz/Publications/felkel_sccg_98.ps.gz` is gone and has no Internet Archive snapshot (checked). Copies exist on readkong.com and phlatforum.com; both are uploads of unclear status. I read the readkong text transcription (https://www.readkong.com/page/straight-skeleton-implementation-8425187) for content only. The phlatforum PDF would not download (TLS error) | Unknown | Yes, from the transcription. Cites use the reprint's own page numbers 1-10, **not** the proceedings pages 210-218 |
| H11 | Huber S. (2011). PhD thesis, Univ. Salzburg (see A1) | https://www.sthu.org/research/publications/files/phdthesis.pdf | Author copy | Yes. Printed = PDF − 8 |
| HH12 | Huber S., Held M. (2012). "A fast straight-skeleton algorithm based on generalized motorcycle graphs." *IJCGA* 22(5): 471-498. doi:10.1142/S0218195912500124 | Author copy: https://www.sthu.org/misc/SKRW14/papers/Huber_2012_AFastStraightSkeletonAlgorithmBasedOnGeneralizedMotorcycleGraphs.pdf (28 pp., draft dated 11 Oct 2012) | Author copy | Yes. The draft's page numbers (1-28) are **not** the journal's 471-498 |
| PHH12 | Palfrader P., Held M., Huber S. (2012). "On computing straight skeletons by means of kinetic triangulations." *Proc. ESA 2012*, LNCS 7501, pp. 766-777. doi:10.1007/978-3-642-33090-2_66 | Author copy: https://www.sthu.org/research/publications/files/PHH12.pdf (12 pp.) | Author copy | Yes. PDF pages only |
| CGAL | Cacciola F., Loriot S., Rouxel-Labbé M. *CGAL 6.2.1 User Manual: 2D Straight Skeleton and Polygon Offsetting*, and the reference page for `Straight_skeleton_builder_2` | https://doc.cgal.org/latest/Straight_skeleton_2/index.html ; https://doc.cgal.org/latest/Straight_skeleton_2/classCGAL_1_1Straight__skeleton__builder__2.html | Manual text © CGAL project. **The library is GPL/commercial; Specula takes no code from it** | Yes (web pages; no page numbers, cited by section) |
| **Sub-1** | **Substitute or follow-up for FO's errors and for CGAL's design:** Eder G., Held M., Palfrader P. (2020). "On implementing straight skeletons: challenges and experiences." *Proc. SoCG 2020*, LIPIcs 164, paper 38. doi:10.4230/LIPIcs.SoCG.2020.38 | https://drops.dagstuhl.de/storage/00lipics/lipics-vol164-socg2020/LIPIcs.SoCG.2020.38/LIPIcs.SoCG.2020.38.pdf | **CC BY** (stated on p. 38:1) | Yes. Page = 38:N = PDF N |
| **Sub-2** | **Substitute (account of FO's errors):** Wang D. (2017). "Fold-and-cut: straight skeleton method implementation." MIT 6.849 course project report | https://courses.csail.mit.edu/6.849/spring17/project/fold-and-cut.pdf | Course page; no licence stated | Yes, PDF pp. 2-5 |
| **Sub-3** | **Supporting detail for PHH12:** Palfrader P. (2013). *Computing straight skeletons by means of kinetic triangulations.* MSc thesis, Univ. Salzburg | https://www.palfrader.org/research/2013/ (file "Palfrader - Computing Straight Skeletons by Means of Kinetic Triangulations, 2013 [Palfrader13].pdf") | Author copy | Chapter 5 only. Printed = PDF − 10 |

Not obtained: Yakersberg E. (2004), MSc thesis, Technion, cited by H11 p. 27 as listing further correctness issues in FO. **No free copy found.**

---

## 1. Felkel and Obdržálek (1998): the method, and where it is wrong

**Method (FO p. 2-8).** The method keeps a set of circular lists of active vertices (SLAV), one list per outer boundary, hole and split-off sub-polygon. It works only with pointers to the *original* edges and never builds the intermediate outlines (p. 2).
- **Convex case (p. 2-3).** For each vertex, compute its bisector and its **nearer** intersection with the two neighbouring bisectors. Queue that point, keyed by its distance to the edge's supporting line. Pop the next one. Skip it if its two source vertices are already "processed". If the predecessor's predecessor equals the other source (a peak), output three arcs. Otherwise output two arcs, make a new node, and compute its new bisector and its nearer intersection (steps 1-2).
- **Split point for a reflex vertex V (p. 4-5).** Test every original edge as the possible "opposite" edge. The candidate point lies on V's bisector, at equal distance from the opposite edge's line and from the lines of V's two edges. It is kept only if it lies in the region bounded by that edge and the bisectors at its two ends. The paper says a plain bisector-against-segment test does not work. **Of all candidates, the one nearest to V is chosen** (p. 5). A check is needed to exclude an edge at V that is parallel to the tested edge (p. 5).
- **Split handling (p. 5-7).** V is marked processed, two nodes are created at the split point, and one list is broken into two. Because one original edge can be shared by several sub-polygons after earlier splits, the correct sub-edge is found by testing that the point lies right of one bisector and left of the next ("multiple splitting of edges", p. 5-6).
- **Holes (p. 8).** These need the outer boundary counter-clockwise and the holes clockwise.
- **Claimed cost.** O(nm + n log n) time with m reflex vertices (abstract p. 1; p. 8, which also writes it as O(n²) / O(nm)), O(n) storage. The paper states that it "handles convex, non-convex polygons and polygons with holes correctly" (p. 8). It also reports measured quadratic times (Fig. 8, p. 9).

**Published accounts of where it is wrong.**

| Fault | Who says so | Input that breaks it |
|---|---|---|
| The **nearest** candidate split point is not necessarily the real split event. A reflex vertex can pass that point and later crash into a different edge e′ | H11 p. 26-27 (PDF 34-35), Fig. 13: "does not necessarily determine the correct split event" | Fig. 13: a reflex vertex whose nearest candidate edge has moved away by the time the vertex arrives |
| Keeping only the **nearer** of the two neighbouring-bisector intersections is wrong. The event that actually happens can be the farther one | Sub-2, PDF p. 2, Fig. 2 | Fig. 2 |
| The "opposite edge" test as given (the strong test) is wrong. A weaker angular test is needed to queue candidates, and the strong test applied when popped. **All** candidate split events must be stored, not just the nearest, and both edge and split candidates per vertex. A split can "turn into" an edge event | Sub-2, PDF p. 3-4, Figs. 3-6 | Fig. 5: many candidates pass the weak test, and only the farthest becomes the split. Fig. 6: the nearest candidate is an edge event, but the vertex ends in a split |
| The skip rule for stale edge events should be "skip unless **both** source vertices are unprocessed". Sub-2 reads FO as skipping only when both are processed | Sub-2, PDF p. 4, Fig. 7 | Fig. 7 |
| "Different issues concerning the correctness" | H11 p. 27 cites Yakersberg (2004) | Not read (no copy) |
| CGAL's code began from FO, but its author "significantly adapted the original algorithm in order to get it to work correctly" | H11 p. 27 (personal e-mail with Cacciola, 2010) | n/a |

Wikipedia also says FO is incorrect, citing these sources. That was not used as evidence.

## 2. How each orders events that tie

| Method | Tie rule | Source |
|---|---|---|
| FO | None stated. One priority queue keyed by distance to the edge line | FO p. 3 |
| Aichholzer-Aurenhammer triangulation algorithm (basic) | None. PHH12 shows it "may loop if multiple concurrent events are not processed properly, **even if exact arithmetic is used**" | PHH12 PDF p. 2 |
| PHH12 / Surfer (floating point or MPFR) | (a) Compute every value **canonically**, so that any two computations of the same triangle's collapse time give bit-identical results. (b) **No precision threshold** is used to decide whether two times are equal, because that could stop termination. (c) Loop detection: keep a history of (triangle count, time, sorted vertex triple). The count drops at every non-flip event, so the history is cleared then. If a triple repeats, all flips between the two occurrences are declared simultaneous, the flips inside the collapsed polygon are rolled back, the polygon is re-triangulated with a monotone chain along the line it collapsed to, and the run resumes. (d) Order along that line is taken from least-squares lines at the earliest and latest collapse times. If the two orders differ, the swapped vertices are declared coincident. These steps guarantee termination on finite precision | PHH12 PDF p. 6-10, Figs. 3-5; Sub-3 ch. 5 (PDF p. 53-65) |
| Sub-3 (exact-arithmetic variant) | Priority key = (collapse time, event type, flip-edge length). Non-flip events go before flips. Among simultaneous flips, flip the **longest** edge first. Termination comes from three facts: the triangle count falls; or the number of collapsing triangles falls; or the longest collapsing edge gets shorter (a finite set of lengths at a fixed time) | Sub-3 p. 52-54 (PDF 62-64), Fig. 25 |
| Sub-1 / Surfer2 (exact, CORE) | Ties are broken by type: **events with infinitely fast vertices first, then other real events, then flip and convex-hull-update events. Among flips, the longest edge first.** Comparisons are exact. They note that exact comparison of many simultaneous events is costly, and suggest per-component queues and possibly a lazy priority queue | Sub-1 p. 38:7-38:10, 38:14-15 |
| HH12 / Bone (double) | It avoids geometric decisions when relinking. An edge event "repots" the incident edges onto the new vertex, so that **no circular-order test is needed** when several events happen at once (e.g. convex vertices on a circular arc) | HH12 p. 24-25 (§6.1) |
| H11 | For real data with rounded coordinates, ε-comparisons are practically unavoidable; exact predicates with root bounds are the alternative | H11 p. 74 (PDF 82) |
| CGAL | "Events that occur first are processed first." The manual says correctness depends on computing and ordering events correctly, and that a queued event "might no longer be applicable" after an earlier one. **No tie rule is stated in the manual** | CGAL `Straight_skeleton_builder_2` reference page, "Algorithm" |

## 3. A reflex vertex hitting a vertex rather than an edge

- **CGAL (reference page, "Algorithm").** When a reflex vertex meets another reflex vertex, it is a **vertex event**: two new contours form without a true split, and one of the new contour vertices may be reflex. CGAL says it follows FO "with the addition of vertex events as described in" Eppstein-Erickson.
- **Sub-1 p. 38:7.** Without general position, two non-incident vertices can coincide. Topologically this is a split. In the kinetic triangulation it is handled like **two edge events, one for each triangle on the collapsed diagonal** (Fig. 5, left).
- **H11 p. 4-6 (PDF 12-14).** A reflex vertex reaching an edge just as the edge to its neighbour collapses is a split (by H11's naming). k reflex vertices meeting is a multi split. If it makes a reflex vertex, it is a vertex event. See A1 §2 for the naming clash.
- **HH12 p. 25 (§6.2).** In a symmetric non-convex 4-gon, a convex vertex and a moving Steiner vertex share a trajectory. A "simple-minded implementation ... is doomed to fail", so a "multi convex vertex" type was added. (H11 p. 72 gives the same case: a motorcycle crashing exactly into the opposite convex vertex.)
- **FO** keeps a split candidate only if it lies inside the region between the edge and its end bisectors (p. 4-5). A point exactly on an end bisector, i.e. hitting a vertex, is a boundary case the paper does not discuss.

## 4. Collinear and parallel edges

- **Infinitely fast vertices (PHH12 PDF p. 5-6, Fig. 3a; Sub-1 p. 38:7-8, Fig. 5).** When an edge collapses between two **parallel, overlapping** front edges, the new vertex would have to run along them at infinite speed. PHH12 marks it as infinitely fast, picks whichever end triangle of its fan has the shorter front edge, and handles it as an edge event with the next vertex on that edge. Sub-1 handles such vertices immediately.
- **Collinear edges becoming adjacent (Sub-1 p. 38:4-5, Monos).** The bisector of two collinear edges is defined as perpendicular to them, starting at the node where they met. When merging chains, a look-ahead is needed to find where that bisector actually starts (Fig. 3).
- **Bisector not unique (Sub-1 p. 38:15, Fig. 10).** For parallel collinear front edges, the vertex's path depends on the history of the propagation (and on other edges' weights), not only on the two edges. That is why exact *predicates* alone are hard to design for ordering collapses.
- **Velocities are a bad basis (HH12 p. 25-26, Fig. 13).** A vertex's velocity accumulates error through chains of earlier events and can be infinite. Bone instead finds an edge's collapse point as the point equidistant to the three original supporting lines, from the input data. HH12 say this takes "the roots of a quadratic polynomial" (citing Held's Voronoi work).
- **Always-zero determinants (Sub-1 p. 38:8).** Three or more front vertices can become collinear and stay collinear. A convex-hull test then reads zero at all times, and needs a "right now" event or a flip.
- **Parallel collapse against Steiner edges (HH12 p. 25).** A face collapsing to a line against a chain of collinear Steiner edges needs an extra merge procedure.
- **Weighted parallel edges (H11 p. 30-31).** The definition itself is singular; see A1 §6.

## 5. What must be exact, and what may be approximate

| Claim | Source |
|---|---|
| CGAL's skeleton needs **exact predicates but not exact constructions**. With the exact-predicates-inexact-constructions kernel, the skeleton is combinatorially correct even if node positions are not exact | CGAL manual, "Low Level API"; H11 p. 75 (via Cacciola); Sub-1 p. 38:10 |
| **Offsets need exact constructions to be guaranteed simple.** At an offset just under half the gap between two parallel edges, rounding can make nearly-touching edges collinear or crossing. CGAL builds the skeleton with inexact constructions and converts it to an exact kernel before offsetting | CGAL manual, section "Create Offset Polygons from a Polygon (With or Without Holes)", paragraphs after the example |
| Exact constructions cost about **100×** in CGAL. Event times are roots of quadratics and need square roots. Surfer2 and Monos use an exact kernel with square roots by default | Sub-1 p. 38:8, 38:10 |
| Avoid square roots where local combinatorics allow. A triangle with two front edges never has a flip. With one front edge, split and flip times come from a **linear** distance-to-line function. Only triangles with no front edge need the quadratic | Sub-1 p. 38:8-9 |
| Bone and Surfer run in plain IEEE double. Robustness comes from canonical computation, from topology-only updates, from avoiding thresholds (Surfer) or using ε (Bone), and from checks afterwards (face boundaries connected, terrain facets planar) | PHH12 PDF p. 9; HH12 §6; H11 p. 73, 77 |

## 6. Stated bounds on the work

| Method | Bound | Source |
|---|---|---|
| FO | Claimed O(nm + n log n) time, O(n) space. H11 restates it as O(nr + n log n). Measured quadratic | FO p. 1, 8; H11 p. 26 |
| CGAL (pre-2012 versions tested) | O(n² log n) time and O(n²) memory in practice, because it queues potential split events for **all** reflex-vertex/edge pairs. Could not run inputs much over 10 000 vertices in 6 GiB | H11 p. 75-77 (PDF 83-85), Table 3; HH12 abstract |
| AA triangulation | O(n³ log n) worst case. Flip count O(n³), but no input with more than O(n²) is known | PHH12 PDF p. 4; HH12 p. 6 |
| PHH12 / Surfer | About n/4 flips on average over about 20 000 inputs. Only a dozen needed more than 2n. O(n log n) observed; about 10 s for 10⁶ vertices in double | PHH12 PDF p. 10-11 |
| HH12 / Bone | Given the motorcycle graph, O((n + k) log n) time and O(n) space, where k ∈ O(nr) "switch events"; O(nr log n) worst case, said to be tight (Lemma 6). Observed O(n log n) on 22 300 datasets | HH12 p. 20, abstract |
| Sub-1 / Surfer2 | Exact arithmetic. Simultaneous events cost heavily under CORE. Axis-parallel inputs were not the slow ones; the slow ones were inputs where opposite edges become incident at the same time in different parts of the polygon | Sub-1 p. 38:13-14 |

## 7. Failure cases to design against (summary)

1. Nearest-candidate split selection (FO). Keep **all** candidates and re-validate each when popped.
2. Concurrent flip events looping, even with exact arithmetic (PHH12). Use a tie key of type then edge length, or loop detection.
3. Thresholded time-equality tests can stop termination (PHH12 PDF p. 9). Prefer canonical, bit-identical recomputation.
4. Infinitely fast vertices between overlapping parallel edges (PHH12, Sub-1).
5. Velocities accumulating error. Compute event points from the original lines (HH12).
6. Offsets from inexact constructions can self-touch (CGAL).
7. Convex vertex and Steiner/motorcycle path on the same trajectory (HH12, H11).

## Not found / could not verify

- **FO has no legal copy found**, and its page numbers here are the reprint's 1-10, not 210-218.
- Yakersberg (2004) was not found.
- CGAL's exact event-ordering rule for ties is not in the manual. Reading the source code was out of bounds (no third-party code).
- HH12 and PHH12 were read as author copies. Journal and LNCS page numbers were not checked.
