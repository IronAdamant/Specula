# R8-A1 The straight skeleton: definition, events, degeneracy

Round 8, section A (soft edges on raised figures). Access date for every source: **2026-11-27**. Each PDF was downloaded to the box and read with poppler `pdftotext`. Page numbers below are the **printed page** where the copy has one, with the PDF page in brackets. Everything is in my own words. No code was taken from any source.

**Question this item answers for the lane.** What exactly is the straight skeleton, which events change the shrinking outline, what happens when several things meet at once, and what is proved (and under which general-position assumption)?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Aichholzer O., Aurenhammer F., Alberts D., Gärtner B. (1995). "A novel type of skeleton for polygons." *J. Universal Computer Science* 1(12): 752-761. doi:10.3217/jucs-001-12-0752 | https://www.jucs.org/jucs_1_12/a_novel_type_of/Aichholzer_O.pdf (publisher, open access) | "J.UCS Open Content License" (stated on https://lib.jucs.org/article/27191). Free to read; not a CC licence | Yes, all 10 pages |
| S2a | Aichholzer O., Aurenhammer F. (1996). "Straight skeletons for general polygonal figures in the plane." *Proc. COCOON '96*, LNCS 1090, pp. 117-126. doi:10.1007/3-540-61332-3_144 | **No free legal copy found** of the LNCS text. Semantic Scholar lists it as closed; TU Graz's abstract page has no PDF; a ResearchGate copy exists but was not used | Springer, all rights reserved | **No** (see S2b) |
| S2b | **Substitute for S2a, by the same authors:** Aichholzer O., Aurenhammer F., "Straight skeletons for general polygonal figures in the plane". This is the authors' 14-page PostScript `aa-ssgpf-98.ps.gz` from Aurenhammer's TU Graz page, retrieved through the Internet Archive snapshot of 2012-02-06: https://web.archive.org/web/20120206001635/http://www.igi.tugraz.at/auren/psfiles/aa-ssgpf-98.ps.gz . The file name matches the later version that Huber (S4, bibliography p. 4489-4492 of the text file, entry [AA98]) cites as *Voronoi's Impact on Modern Science*, Book 2, pp. 7-21, Kiev 1998. | Author's own file (archived) | Authors' copy; no licence stated | Yes, all 14 pages (own page numbers 1-14) |
| S3 | Eppstein D., Erickson J. (1999). "Raising roofs, crashing cycles, and playing pool: applications of a data structure for finding pairwise interactions." *Discrete Comput. Geom.* 22(4): 569-592. doi:10.1007/PL00009479. Extended abstract in *Proc. 14th ACM SoCG* (1998) 58-67 | Author copy https://jeffe.cs.illinois.edu/pubs/pdf/cycles.pdf (the live site now shows a bot check to scripts; I read the Internet Archive copy of the same URL, snapshot 2025-04-22). Preprint "submitted July 1, 1998, revised March 28, 1999" | Authors' preprint; no licence stated | Yes. The preprint's own page numbers are one less than the PDF page (printed p. 8 = PDF p. 9). **The DCG journal page numbers were not checked** |
| S4 | Supporting source: Huber S. (2011). *Computing straight skeletons and motorcycle graphs: theory and practice.* PhD thesis, Univ. Salzburg | https://www.sthu.org/research/publications/files/phdthesis.pdf (author's site) | Author copy; no licence stated | Yes, the relevant sections. Printed page = PDF page - 8 |

---

## 1. The wavefront definition

**S1, p. 752-753 (PDF 1-2).** The outline of a simple polygon P is shrunk towards its interior so that every edge moves inwards, parallel to itself, at the same speed. Edge lengths may shrink or grow. Each vertex slides along the angle bisector of its two edges. This goes on as long as the outline keeps the same combinatorics. The **straight skeleton S(P)** is the union of the bisector pieces traced by the moving vertices. Each edge e sweeps a region called its **face**. The bisector pieces are **arcs**, and their end points that are not polygon vertices are **nodes**. S1 contrasts this with the medial axis: the two coincide for a convex polygon, but at a reflex vertex the medial axis has a parabolic arc while the skeleton stays straight (p. 752).

**S2b, p. 3-4.** This extends the definition from a simple polygon to a planar straight-line graph G. Each edge of G sends two wavefront edges, one to each side. A terminal vertex (degree 1) also sends one wavefront edge of its own, perpendicular to its single edge, so the front near a terminal is rectangular (p. 3-4; S4 p. 6-7 says the same of [AA96]). The fronts of all figures move at once, at the same speed and self-parallel, and the vertices move on bisectors.

**S2b, p. 5.** The key difference from a Voronoi wavefront: in the straight skeleton every point of a front has the same *offset* (perpendicular distance to the supporting line), not the same *distance*. So fronts stay polygonal, and a vertex moves faster than its neighbourhood. Its speed is set by the angle at the vertex.

**For Specula's inset.** At inset time t, the inset outline is exactly the wavefront at time t. For a convex corner of interior angle α, the vertex moves at speed 1/sin(α/2) along the bisector. This is the standard unit-speed construction; S4's Lemma 1.11 (p. 30, PDF 38) gives the general weighted form, which reduces to the bisector for equal speeds. As α tends to 0 or to 2π, the speed blows up. This is the "horn tip" problem in numbers.

## 2. The event kinds

| Event | What happens | Where stated |
|---|---|---|
| **Edge event** | A front edge shrinks to length zero. Its two neighbours become adjacent. In a graph (S2b), if the neighbours also have zero length, the whole front vanishes | S1 p. 753; S2b p. 4; S3 printed p. 7-8 (PDF 8-9) |
| **Split event** | A reflex vertex runs into an edge and splits it. The polygon (or front) splits in two, and new adjacencies form between the split edge and each edge at the reflex vertex. In S2b, a split can also be *interference*, where two different fronts merge into one | S1 p. 753; S2b p. 4; S3 printed p. 8 |
| **Triangle collapse** (simultaneous by nature) | Even in general position, three edge events happen at the same instant when a triangle shrinks to a point | S1 p. 753 |
| **Vertex event** | Two or more reflex vertices, and nothing else, reach the same point at the same moment. **It can create a new reflex vertex**, although the total number of reflex vertices always falls | Named by S3, printed p. 8 (PDF 9), Fig. 3(a); S4 p. 6 (PDF 14) |
| **Multi split event** | Two or more reflex vertices meet at one point at the same time. The front splits into k parts. A multi split that makes a new reflex vertex is what S3 calls a vertex event | S4 p. 6 (PDF 14) |
| **Split-plus-collapse** | A reflex vertex hits an edge at the same moment as the edge between it and a neighbour collapses, leaving one front part. S4 calls this a split event, because a reflex slice appears around the node. **S3 calls the same case an edge event. Vyatkina calls it a "sticking event", and S1 does not discuss it** | S4 p. 4-6 (PDF 12-14), Fig. 2(c), footnote 8 |
| **Flip event** (algorithmic only, not a skeleton event) | In the triangulation-based algorithm, a front vertex sweeps across a triangulation spoke, and the spoke is flipped. It changes no skeleton node | S2b p. 7, Fig. 5 |
| **Inverse edge event / triangular hole** (non-island roofs only) | When flooding an arbitrary roof that is not the straight-skeleton roof, a new edge may start growing at a local minimum of a facet, or a triangular hole may appear (three inverse edge events at once). S1 says "this list of events is not complete" | S1 p. 758 (PDF 7) |

**Naming is not settled.** S4 p. 6 (PDF 14) says outright that the taxonomy is "in flux". Tănase (2009) splits edge and split events into three sub-classes each, and calls S4's Fig. 2(c) case "reflex edge annihilation". Specula's event names should be its own, with a note on how each maps onto these.

## 3. What each paper says about simultaneous events

- **S1 (1995).** It notes only that a collapsing triangle gives three simultaneous edge events even in general position (p. 753). For the graph-and-roof model, it says "we exclude degeneracies caused by special shapes of P" and assumes every node has degree 3 (p. 755, PDF 4). Nothing more on ties.
- **S2b (later AA paper).** The triangulation algorithm's correctness is "proved easily" by induction (p. 7). I searched the whole 14 pages for "general position", "degenerate" and "simultaneous": **the paper does not discuss tie-breaking.** A later paper (PHH12, see A2) shows the basic algorithm "may loop if multiple concurrent events are not processed properly, even if exact arithmetic is used" (PHH12 p. 2).
- **S3 (1999), printed p. 8 (PDF 9).** In degenerate cases a skeleton node can have degree above 3, caused by simultaneous events at one spot. Most such cases can be handled one at a time by standard perturbation, which replaces the high-degree node by degree-3 nodes joined by zero-length edges. Footnote 2: "we can usually process simultaneous events in arbitrary order." **The exception is the vertex event:** any perturbation that removes it "radically changes" the skeleton (Fig. 3(b)), so an algorithm must handle vertex events directly. On printed p. 12 (PDF 13): at a vertex event involving k reflex vertices, the algorithm deletes k rays and replaces 2k unbounded slabs with the correct reflex slabs. If a new reflex vertex appears, it is entered as a new ray. Even with vertex events there are at most 2r − 1 reflex vertices over the whole run.
- **S4, p. 72-74 (PDF 80-82).** Huber shows (his Fig. 39) that an arbitrarily small move of one vertex turns a vertex event into a much longer reflex arc. The skeleton **does not depend continuously on the input** (for example, in Hausdorff distance). The consequences he draws: (i) perturbation cannot be used to remove special cases; (ii) limits of skeletons can depend on the approximating sequence; (iii) to decide whether two reflex vertices meet "at exactly the same time" you need either ε-comparisons (|a − b| ≤ ε) or exact predicates with root bounds. For real CAD-type input, where coordinates are already rounded, he argues that ε-comparisons are practically forced, because a symmetric design meant to have a vertex event will otherwise miss it (p. 74).

## 4. What is proved, and under which assumption

| Property | Statement (my words) | Assumption | Source |
|---|---|---|---|
| Tree | For a simple n-gon, S(P) is a tree with the n polygon vertices as leaves, **n − 2 nodes, 2n − 3 arcs, n faces**, each face connected | Proof counts one node per event. Huber notes that, strictly, multi split events must also be excluded for the exact counts; with them you get **at most** n − 2 nodes and at most 2n − 3 arcs | S1 Lemma 1, p. 754; S4 Lemma 1.2 and footnote 7, p. 4 (PDF 12) |
| Size vs medial axis | S(P) has 2n − 3 arcs; the medial axis has 2n + r − 3 arcs, r of them parabolic (r = reflex vertices) | Simple polygon | S1 p. 753 |
| Reflex arcs | Reflex arcs (valleys of the roof) only start at reflex polygon vertices. Every new vertex created by an event is convex | Simple polygon, unweighted. Note: S3 Fig. 3(a) and S4 p. 6 show that a **vertex event can create a new reflex vertex**, so this lemma relies on the no-vertex-event case. S1 does not discuss vertex events | S1 Lemma 2, p. 754 |
| Roof | Lift each node by its offset time (equal slopes, say 45°). The result is a terrain whose facets lie in the tilted half-planes of the edges. Every roof corresponds to a plane "bisector graph" and back | Degeneracies excluded (degree-3 nodes) | S1 Theorem 3, p. 755-756 |
| Roof is not unique; the skeleton roof is | Many roofs exist for one P. The straight-skeleton roof ("island") is the only one in which every valley touches P (Thm 6), and the only one in which every raindrop runs off to its own facet's edge (Thm 7, the "gradient property") | As above | S1 p. 758-759 (PDF 7-8) |
| Monotone faces | Each face f(e) is monotone in the direction of e: every line normal to e meets it in a connected set | Unweighted. **Fails for weighted skeletons** (S4 p. 30, PDF 38, Fig. 15a) | S1 Lemma 4, p. 757; S2b Lemma 1, p. 4 |
| Size for a graph G | Exactly 2n + t − 2 nodes, counting a node at infinity for each unbounded arc (t terminals). For a simple polygon, the interior part is a tree with n − 2 nodes | Planar straight-line graph | S2b Lemma 2, p. 5 |
| No distance definition | No distance function defined without knowing S beforehand gives S as a lower envelope or Voronoi diagram. Incremental insertion and divide-and-conquer fail | | S1 p. 759; S2b p. 6-7 |
| **Lower envelope after all (for unweighted)** | The skeleton is the projection of the lower envelope of **edge slabs and reflex slabs**. An edge slab is a half-strip rising at 45° above an edge. A reflex slab is a half-strip rising from the valley of a reflex vertex | **Unweighted only.** The reflex slabs are not locally defined, because their inner edges depend on the whole collision history. That is how this result squares with S1's negative result. S3 Fig. 6 is a weighted counter-example | S3 Lemma 4.2, printed p. 10-11 (PDF 11-12) |

**Sources disagree (apparently):** S1 (p. 759) says the roof "cannot be expressed as the lower envelope" of locally defined linear functions. S3 (Lemma 4.2) says it *is* the lower envelope of edge and reflex slabs. S3 reconciles the two on printed p. 10-11: its reflex slabs are not local. Both statements hold.

## 5. Bounds on the work (for reference)

- S1 p. 760 (PDF 9): checking every pair of edges for the next event gives O(n³) time and O(n) space. A priority queue gives O(n² log n) time but O(n²) storage. Finding the next *split* event efficiently is the hard part.
- S2b p. 8-9: the triangulation algorithm has at most 2n + t − 2 edge plus split events (Lemma 4). Flip events come only from reflex vertices, and each triple of reflex vertices gives at most two flips, so O(n³) flips (Lemma 5). Total O(n³ log n) time, O(n) space. The authors say it "can be expected to be practically efficient" (abstract, p. 1); p. 9 reports observed running times close to O(n log n), not much above the cost of the initial triangulation.
- S3: O(n^{1+ε} + n^{8/11+ε} r^{9/11+ε}) time and space (Theorem 4.3, printed p. 12). Mitred offsets cost linear time once the skeleton is known. Bevelled offsets come from adding a zero-length edge at each reflex vertex, perpendicular to its bisector (printed p. 13, PDF 14).

## 6. Known failure cases (the part the lane needs most)

1. **Discontinuity under perturbation (vertex events).** Two reflex vertices meeting exactly (common in symmetric outlines and axis-parallel shapes: notches, gear teeth) give a different skeleton from the nearly-meeting case. The nearly-meeting case makes a long reflex arc and a new reflex corner in the inset (S3 Fig. 3(b), printed p. 8; S4 Fig. 39, p. 73). **For Specula:** a notch whose two reflex corners are symmetric will flip between two topologies under rounding unless ties are decided deliberately.
2. **Tiny reflex angles.** S1 p. 760 (PDF 9): adding a reflex vertex with a very small exterior angle "may alter the skeleton structure completely". The authors suggest cutting such vertices locally so that exterior angles are at least 90°. This is a direct precedent for clipping sharp horn tips before insetting.
3. **Split-plus-collapse ambiguity** (S4 Fig. 2(c)). Different papers name and therefore handle it differently. An implementation needs a stated rule.
4. **Weighted case.** Monotone faces, the slab lower envelope and the reflex/convex angle test all fail. Near-parallel edges with different weights give a singular definition (S4 p. 30-31, PDF 38-39, Figs. 15-16).
5. **Exact degree-3 counts** assume no multi split events (S4 footnote 7, p. 4).

## Not found / could not verify

- **S2a (COCOON '96 text) not read.** No free legal copy found. S2b is the authors' own later version; its wording and page numbers will differ from the LNCS pages 117-126.
- The DCG journal page numbers for S3 were not checked. The cites above are to the authors' preprint.
- None of the three listed papers gives an explicit rule for ordering events that tie within rounding. See A2 for the implementation papers that do.
