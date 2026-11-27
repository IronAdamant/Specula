# Second check: R8-A3-offsets-round-mitre.md

Independent re-check, 2026-11-27. Claims were re-found by grep in the page-marked text:
- palfrader-held-2015: journal page = PDF + 413, from the "414" head on PDF 1;
- hehu09: journal page = PDF + 325;
- ee99: printed = PDF - 1;
- the SVG 2 text: svg2-painting.txt.

The live W3C /TR/SVG2/ page was re-fetched for the version and licence. The mitre-limit table was recomputed with Python.

| # | Claim | Source, page | Status |
|---|---|---|---|
| 1 | S1 © 2015 CAD Solutions; CAD&A 12(4) 414-424 | palfrader-held PDF 1-2 header | VERIFIED |
| 2 | Interior offset = points at distance exactly r | S1 PDF 1 = p. 414 | VERIFIED |
| 3 | Four-step conventional offsetting | S1 PDF 1 = p. 414 | VERIFIED |
| 4 | Persson; Held "first thorough study" | S1 PDF 1-2 = p. 414-415 | VERIFIED |
| 5 | Procedure of reading the offset from Voronoi cells | - | Author's explanation, labelled in the file |
| 6 | HeHu09 CI, VC, Lemma 3, four dummy points (bounding box) | hehu09 PDF 4 = p. 329 | VERIFIED |
| 7 | Apex splitting; clearance monotone along edges | hehu09 PDF 6 = p. 331 | VERIFIED |
| 8 | Memory "sky-rocket" when approximating arcs | hehu09 PDF 2 = p. 327 | VERIFIED |
| 9 | Mitred offset definition; distance to supporting line | S1 PDF 1 = p. 414; PDF 7 | VERIFIED |
| 10 | Skeleton = traces of vertices of shrinking mitred offsets | S1 PDF 2 = p. 415 | VERIFIED |
| 11 | Bevel via zero-length edge | ee99 PDF 14 = printed 13 | VERIFIED |
| 12 | Far-away mitre tips at acute reflex angles | S1 PDF 3 and 6 = p. 416, 419 | VERIFIED |
| 13 | Clipper and GEOS via winding numbers; differ from skeleton offsets | S1 PDF 8-9 = p. 421-422 | VERIFIED |
| 14 | FO "known to be flawed" [15, 28] | S1 PDF 8 = p. 421 | **FIX applied**: the draft attached "flawed" to CGAL's code too. S1 calls Cacciola's CGAL code "a corrected and improved version" of FO; wording corrected |
| 15 | Double root (multiplicity two) can be missed; linear derivative of squared distance; multi split near zero | S1 PDF 5-6 = p. 418-419 | VERIFIED |
| 16 | Linear axis: hidden zero-length edges; speed ≤ √2, approaching 1; no vertex farther than √2·d; converges to the medial axis (Tănase-Veltkamp) | S1 PDF 3, 6-7 = p. 416, 419-420 | VERIFIED |
| 17 | SVG mitre length = stroke-width / sin(θ/2); limit as multiple of stroke width; initial 4; negative invalid; 0-1 note | svg2-painting.txt §13.5.5 (lines 545-589) | VERIFIED |
| 18 | §13.5.7: mitre if 1/sin(θ/2) ≤ stroke-miterlimit, else bevel | svg2-painting.txt line 930 | VERIFIED |
| 19 | SVG version and licence | live /TR/SVG2/ | **FIX applied**: the draft said "W3C (2018) Candidate Recommendation" and "W3C Document License". The page on the access date is the "Candidate Recommendation Snapshot 06 October 2026" under the "permissive document license"; corrected |
| 20 | Mitre-limit table (√2 → 90°, 2 → 60°, 4 → 28.96°, 10 → 11.48°) | recomputed | VERIFIED (89.99999..., 60.0000..., 28.955, 11.478) |
| 21 | Held 1998 and VRONI 2001 not obtained | Held's links 404; the "PDF" files fetched were HTML error pages (now renamed `*.FAILED-not-a-pdf.html` in raw/round8, not deleted) | VERIFIED |
| 22 | HeHu09 copy legality | Elsevier personal-copy banner on the PDF | WEAK: legality unclear, as the file says |

Corrections: rows 14 and 19.
