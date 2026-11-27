# Second check: R8-A6-capping-holes-fist-cdt-flips.md

Independent re-check, 2026-11-27. Claims were re-found by grep in the page-marked text. Page offsets were re-derived from running heads: delnotes printed = PDF - 4; cdtj1 printed = PDF - 4; cgi98 and Triangle cited by PDF page. Bibliographic data was checked against Crossref (CGI'98) and the FIST project page HTML. Eberly p. 13 was checked on a rendered page image.

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | FIST 2001 not obtained: the project page's links are commented out; algo01.ps.gz returns 404; Wayback refused TLS from the box | fist-project.html; curl logs (algo01.log: all 20 timed Wayback retries (last at about 22:23) ended without a gzip file; the one body saved was the cosy server's 404 page, kept as raw/round8/algo01.FAILED-not-a-psgz.html) | **FIX applied**: the draft said the page "links" the preprint; the link is inside an HTML comment |
| 2 | CGI'98 citation: pp. 633-643, doi:10.1109/CGI.1998.694322, Hannover June 1998 | Crossref API; researchr listing | **FIX applied**: pages were unverified in the draft; now confirmed and DOI added |
| 3 | Convex/reflex/tangential, diagonal, cone definitions | cgi98 PDF 2 | VERIFIED |
| 4 | CE1 and CE2 (Facts 2.1, 2.2), both necessary and sufficient; CE2 uses the closure of the triangle | cgi98 PDF 2 | VERIFIED |
| 5 | O(k)/O(r) per test; O(n²) CE1, O(n·r) CE2 | cgi98 PDF 2-3 | VERIFIED |
| 6 | BV-tree/grid; register near-boundary items in both cells; CE2+grid fastest; deleting reflex vertices helps with self-overlap | cgi98 PDF 3-4 | VERIFIED |
| 7 | Lexicographic sort, duplicates discarded, shared index; one sidedness primitive; index-ordered evaluation with sign flip; user epsilon; "consistent throughout" | cgi98 PDF 3 | VERIFIED |
| 8 | Exact arithmetic insufficient; symbolic perturbation can aggravate | cgi98 PDF 4 | VERIFIED |
| 9 | Signed area for orientation; figure-eight larger lobe; outer loop = max \|area\|; local tests failed; no guarantee | cgi98 PDF 4 (§4.1) | VERIFIED |
| 10 | Sequential/random/sorted modes; fan for convex polygon; "nicest" ear priority queue | cgi98 PDF 4 (§3.2) | VERIFIED |
| 11 | 180° vs 0°/360°; walk CW and CCW; "lengthy case analysis" | cgi98 PDF 5 (§4.2) | VERIFIED |
| 12 | Degenerate ears clipped on the spot; v_{i-1} on edge v_{i+1} v_{i+2} | cgi98 PDF 5 (§4.3) | VERIFIED |
| 13 | Fig. 1a/1b rules for v_k on diagonal / on ear edges | cgi98 PDF 5 | VERIFIED |
| 14 | A_ik / A_ki rule: reject only if both positive; misclassification when A_ki = 0 argued harmless | cgi98 PDF 5 | VERIFIED |
| 15 | Mapping to the engine's three faults | - | Author's inference, labelled |
| 16 | Recovery: re-classify; diagonal with 3 checks incl. winding number 1; Fig. 2a v5 v9 twisted; self-intersection two-triangle fix and the 3D gap reason; desperate mode | cgi98 PDF 5-6 | VERIFIED |
| 17 | "degrade gracefully"; FIST "extended to handle multiply-connected polygons" | cgi98 PDF 2 | VERIFIED |
| 18 | Eberly pseudosimple polygon; bridge | eberly PDF 6-8 | VERIFIED |
| 19 | Six-step visibility algorithm; minimum angle; one-sided edge rule in step 2 | eberly PDF 10 | VERIFIED |
| 20 | Spoor (2021) distance suggestion; Lalish (2023) counterexample coordinates; distances √50 and √40 | eberly PDF 10-11 | VERIFIED (coordinates re-read) |
| 21 | Multiple holes by max x; bridge edge pair rule | eberly PDF 11 | VERIFIED |
| 22 | Preconditions; Shalev 2019 touching holes under floats; rational arithmetic option; Binder 2023 sorting claim; "perhaps my analysis is incorrect" | eberly PDF 12 | VERIFIED |
| 23 | Typo in Eberly eq. (4) and "0 ≤ j ≤ 13" | eberly PDF 13 (image) | VERIFIED as printed; that it is a typo is the author's reading, labelled |
| 24 | Eberly CC BY 4.0; created 2002, modified 3 Aug 2024 | eberly PDF 1 | VERIFIED |
| 25 | Triangle: constrained edges not flipped | triangle96 PDF 3 | VERIFIED |
| 26 | Triangle: conforming (midpoint + Lawson) vs CDT default; segment intersections detected; hole "triangle-eating virus" stopped by segments; footnote on oriented curves | triangle96 PDF 4 | VERIFIED |
| 27 | Removing triangles before refinement avoids over-refinement | triangle96 PDF 5 | VERIFIED |
| 28 | CDT definition via visibility | delnotes §2.8, Definition 36, PDF 44 | **FIX applied**: draft cited "§2.10"; the CDT section is §2.8 (printed p. 36-42) |
| 29 | Constrained Delaunay Lemma (Theorem 13), Theorem 14 (modified flips, O(n²), proof omitted as a straightforward extension), Corollary 15, Theorem 16 (maximises min angle among triangulations of the PLC) | delnotes PDF 45-46 (printed 41-42) | VERIFIED |
| 30 | L3: 2D CDT maximises min angle; Theorem 13 Delaunay Lemma; Theorem 32 CDT Theorem; weakly ridge-protected | cdtj1 PDF 6, 34, 54, 11 | VERIFIED |
| 31 | Lemma 4 | delnotes PDF 36 (printed 32) | **FIX applied**: the draft's wording ("an edge that cannot be flipped is locally Delaunay") was the contrapositive, not the source's statement; now as stated (either locally Delaunay, or flippable and the new edge is locally Delaunay) |
| 32 | Lemma 1 lifting | delnotes PDF 33 (printed 29) | **FIX applied**: the draft paraphrased Lemma 1 as the edge criterion; Lemma 1 is the circle-lift statement, used in the Theorem 5 proof for edges |
| 33 | Theorem 3 Delaunay Lemma | delnotes PDF 34 (printed 30) | VERIFIED |
| 34 | Theorem 5 and proof: lifting, gluing tetrahedra, edge never reappears, ≤ n(n - 1)/2 | delnotes PDF 37 (printed 33) | VERIFIED |
| 35 | Queue implementation, O(n + k), k ∈ Θ(n²), fast for small perturbations | delnotes PDF 37-38 | VERIFIED |
| 36 | Lemma 8 (min angle up, max circumcircle down) | delnotes PDF 38 (printed 34) | VERIFIED |
| 37 | Lawson [73] introduced the flip algorithm and max-min angle; [73] = Lawson 1977, Mathematical Software III, 161-194 | delnotes PDF 39 and bibliography | VERIFIED |
| 38 | Strict test needed; inexact in-circle may cycle | - | Author's inference, labelled; supported indirectly by R8-A5 sources |
| 39 | FIST 2001 hole-bridging method itself | - | UNVERIFIABLE (not read) |

Corrections: rows 1, 2, 28, 31, 32.
