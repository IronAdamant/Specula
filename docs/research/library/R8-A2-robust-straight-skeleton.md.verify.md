# Second check: R8-A2-robust-straight-skeleton.md

Independent re-check, 2026-11-27. Claims were re-found by grep in the page-marked text of each raw file (phh12, ehp20, dw-foldcut, huber-held-2012, palfrader13-thesis, huber-thesis). The CGAL pages were re-read from the saved HTML. The FO transcription was **re-fetched** from readkong (HTTP 200) and searched.

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | FO method: SLAV; nearer bisector intersection; nearest split candidate; "processed" skip rule; multiple splitting; outer CCW / holes CW; O(nm + n log n), O(n) storage; "handles ... correctly" | fo-readkong.txt (re-fetched) | VERIFIED for content. **WEAK for FO page numbers**: the transcription has no reliable page breaks, so the "p. 1-10" reprint cites could not be re-confirmed |
| 2 | FO proceedings pages 210-218, SCCG'98, ed. Szirmay-Kalos | cgal-cite.html bibliography [3] | VERIFIED |
| 3 | H11: FO's nearest split point "does not necessarily determine the correct split event", Fig. 13; Yakersberg; Cacciola "significantly adapted" | huber PDF 34-35 = p. 26-27 | VERIFIED |
| 4 | Sub-2: need both edge-event intersections (Fig. 2); weak/strong opposite tests (Fig. 4); store all split events, edge and split; Fig. 7 both unprocessed | dw-foldcut PDF 2-5 | VERIFIED |
| 5 | PHH12: AA basic algorithm "may loop ... even if exact arithmetic is used" | phh12 PDF 2 (also PDF 7) | VERIFIED |
| 6 | PHH12: canonical computation; no precision thresholds | phh12 PDF 9 | VERIFIED |
| 7 | PHH12: history triples; repeat → resolution; monotone chain | phh12 PDF 8-9 | VERIFIED |
| 8 | PHH12: least-squares lines at t_min / t_max; differing order → coincident | phh12 PDF 10 | VERIFIED |
| 9 | PHH12: infinitely fast vertices, shorter-edge triangle choice | phh12 PDF 6 | VERIFIED |
| 10 | PHH12: O(n³) flips best known bound; no input with more than O(n²) flips known | phh12 PDF 4 | VERIFIED |
| 11 | PHH12: about n/4 flips on average, a dozen > 2n, ~20 000 inputs, up to 2.5 M vertices | phh12 PDF 10-11 | VERIFIED |
| 12 | Sub-1 CC BY | ehp20 PDF 1 | VERIFIED |
| 13 | Sub-1 tie order: infinitely fast first, other real events, then flip / convex-hull update; longest flip edge first; exact via Core | ehp20 PDF 9 (38:9) | VERIFIED (inside the cited 38:7-38:10) |
| 14 | Sub-1 real events over flips via secondary key; longest-edge monotonicity argument | ehp20 PDF 7 | VERIFIED |
| 15 | Sub-1 non-incident vertices coinciding → like two edge events | ehp20 PDF 7 | VERIFIED |
| 16 | Sub-1 collinear-edge bisector rule; look-ahead | ehp20 PDF 4-5 | VERIFIED |
| 17 | Sub-1 always-zero determinant / "right now" event | ehp20 PDF 8 | VERIFIED |
| 18 | Sub-1 ~100× exact constructions; square roots | ehp20 PDF 10 | VERIFIED |
| 19 | Sub-1 linear distance-to-line for triangles with a front edge; quadratic only without one | ehp20 PDF 8-9 | VERIFIED |
| 20 | Sub-1 axis-parallel not troublesome; component queues; lazy queue | ehp20 PDF 13, 9, 15 | VERIFIED |
| 21 | Sub-1 bisector not unique for parallel collinear edges, depends on history | ehp20 PDF 15 | VERIFIED |
| 22 | Sub-3 priority key and longest-edge flip; Fig. 25 | palfrader13 PDF 59-63 | VERIFIED |
| 23 | HH12 repot; "doomed to fail"; multi convex vertex | huber-held-2012 p. 24-25 | VERIFIED |
| 24 | HH12 velocities vs three supporting lines; "roots of a quadratic polynomial" | huber-held-2012 p. 25-26 | VERIFIED |
| 25 | HH12 Lemma 6 O((n + k) log n), O(n) space, k ∈ O(nr); tight O(nr log n); 22 300 datasets | huber-held-2012 p. 20, p. 1, 6 | VERIFIED |
| 26 | CGAL: "events that occur first are processed first"; "might no longer be applicable"; based on [3] FO with vertex events from [2] EE | cgal-ssb.html; cgal-cite.html refs [2], [3] | VERIFIED |
| 27 | CGAL: reflex vertex hitting reflex vertex → no actual split, two parts | cgal-ssb.html | VERIFIED |
| 28 | CGAL: exact predicates but not exact constructions; offsets need exact constructions (near-collapsing parallel edges become collinear or cross) | cgal-ss.html | VERIFIED |
| 29 | CGAL memory: 6 GiB, about 10 000 vertices, Table 3 | huber PDF 83-85 = p. 75-77 | VERIFIED |
| 30 | H11 p. 75 "via Cacciola" for exact predicates; H11 p. 73, 77 post-checks (connected faces, planar facets) | huber-thesis | WEAK: not re-found by keyword search in this pass. The CGAL manual (row 28) independently supports the exact-predicates point |
| 31 | H11 p. 74 ε-comparisons | huber PDF 82 | VERIFIED (see A1 verify row 18) |
| 32 | HH12/PHH12 journal and LNCS pages | - | UNVERIFIABLE (author copies), as stated |

No factual errors found. Rows 1 and 30 are marked WEAK (page-level only).
