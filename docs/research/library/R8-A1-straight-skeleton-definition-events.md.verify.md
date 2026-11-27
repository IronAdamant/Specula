# Second check: R8-A1-straight-skeleton-definition-events.md

Independent re-check, 2026-11-27. Every page cite was re-found by grep in the page-marked text of the raw files (aaag95, aa96, ee99, huber-thesis). The page maps were re-derived from the running heads:
- J.UCS page = PDF + 751 (header "J.UCS vol. 1 no. 12 (1995), 752-761");
- S2b page = PDF page (14 pp.);
- EE99 printed = PDF - 1;
- Huber printed = PDF - 8.

The J.UCS licence page was re-fetched live.

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | S1 citation, J.UCS 1(12) 752-761; Open Content License | aaag95 PDF 1; lib.jucs.org/article/27191 (re-fetched) | VERIFIED |
| 2 | S2a has no free copy; S2b is the authors' 14-page file from the Wayback snapshot | aa96.ps.gz on disk; title page | VERIFIED (file present; title and authors match). The no-free-copy statement is a search result and cannot be proven exhaustively: WEAK |
| 3 | Wavefront definition; medial axis parabolic arcs at reflex vertices | aaag95 PDF 1-2 (p. 752-753) | VERIFIED |
| 4 | PSLG extension; terminals give one wavefront edge each | aa96 PDF 3-4 | VERIFIED |
| 5 | Vertex moves faster than its neighbourhood; speed set by the angle | aa96 PDF 5 | VERIFIED |
| 6 | Huber Lemma 1.11 (vertex speed, weighted) | huber PDF 38 = p. 30 | VERIFIED |
| 7 | Edge and split events; interference | aaag95 p. 753; aa96 PDF 4 | VERIFIED |
| 8 | Three simultaneous edge events from a collapsing triangle in general position | aaag95 PDF 2 = p. 753 | VERIFIED |
| 9 | Vertex event can create a new reflex vertex; Fig. 3(a) | ee99 PDF 9 = printed 8 | VERIFIED |
| 10 | Multi split, sticking event, Fig. 2(c), "AAAG95 did not discuss this case" | huber PDF 12-14 = p. 4-6 | VERIFIED |
| 11 | Flip event | aa96 PDF 7 (Fig. 5 on PDF 8) | VERIFIED |
| 12 | "This list of events is not complete" | aaag95 PDF 7 = p. 758 | VERIFIED |
| 13 | Taxonomy "in flux"; Tănase's "reflex edge annihilation" | huber PDF 14 = p. 6 | VERIFIED |
| 14 | "we exclude degeneracies caused by special shapes"; degree 3 | aaag95 PDF 4 = p. 755 | VERIFIED |
| 15 | AA paper "proved easily"; no discussion of general position or ties | aa96 PDF 7; grep for "general position", "degenera", "simultaneous" found only the construction-order sentence on PDF 7 | VERIFIED |
| 16 | Perturbation into degree-3 nodes with zero-length edges; footnote 2 "arbitrary order"; vertex events "radically" change the skeleton | ee99 PDF 9 | VERIFIED |
| 17 | At most 2r - 1 reflex vertices over the history | ee99 PDF 13 = printed 12 | VERIFIED |
| 18 | Huber Fig. 39; not continuous (Hausdorff); perturbation unusable; limits; ε or exact; CAD argument | huber PDF 81-82 = p. 73-74 | VERIFIED |
| 19 | Lemma 1 (tree, n faces, n - 2 nodes, 2n - 3 arcs) | aaag95 PDF 3 = p. 754 | VERIFIED |
| 20 | Huber Lemma 1.2 and footnote 7 (multi split) | huber PDF 12 = p. 4 | VERIFIED |
| 21 | Medial axis 2n + r - 3 arcs, r parabolic | aaag95 PDF 2 = p. 753 | VERIFIED |
| 22 | Lemma 2 (reflex arcs only from reflex vertices) | aaag95 PDF 3 = p. 754 | VERIFIED |
| 23 | Theorem 3 (roofs and bisector graphs) | aaag95 PDF 5 = p. 756 (statement); the definitions start on p. 755 | VERIFIED |
| 24 | Theorems 6 and 7 (island, gradient property) | aaag95 PDF 7-8 = p. 758-759 | VERIFIED |
| 25 | Lemma 4 (monotone faces) | aaag95 PDF 6 = p. 757; aa96 Lemma 1 PDF 4 | VERIFIED |
| 26 | Weighted faces not monotone | huber PDF 38 = p. 30 | VERIFIED |
| 27 | Graph size 2n + t - 2 (Lemma 2) | aa96 PDF 5 | VERIFIED |
| 28 | No lower-envelope distance definition; incremental insertion and divide-and-conquer fail | aaag95 PDF 8 = p. 759; aa96 PDF 7 | VERIFIED |
| 29 | EE99 Lemma 4.2 (edge + reflex slabs) | ee99 PDF 11-12 | **FIX applied**: page range narrowed from "printed 9-11 (PDF 10-12)" to printed 10-11 (PDF 11-12) |
| 30 | EE99 Fig. 6 weighted counterexample | ee99 PDF 12 | VERIFIED |
| 31 | O(n³) / O(n² log n) with O(n²) storage | aaag95 PDF 9 = p. 760 | VERIFIED |
| 32 | AA: O(n³ log n) time, O(n) space; Lemmas 4 and 5 | aa96 PDF 8-9 | VERIFIED |
| 33 | "can be expected to be practically efficient" | aa96 PDF 1 (abstract) | **FIX applied**: the quote was attributed to p. 8-9; it is in the abstract. Added p. 9's observed near-O(n log n) running time |
| 34 | EE99 time bound O(n^(1+ε) + n^(8/11+ε) r^(9/11+ε)), Theorem 4.3 | ee99 PDF 13 = printed 12 | VERIFIED |
| 35 | Bevelled offsets via a zero-length edge | ee99 PDF 14 = printed 13 | VERIFIED |
| 36 | Tiny exterior angle "may alter the skeleton structure completely"; cut to ≥ 90° | aaag95 PDF 9 = p. 760 | VERIFIED (the source misspells "completly") |
| 37 | "For Specula" notes (speed 1/sin(α/2), symmetric notch ties) | - | Author's application notes, labelled |
| 38 | EE99 DCG journal pages | - | UNVERIFIABLE (preprint pagination only), as stated |

Corrections: rows 29 and 33.
