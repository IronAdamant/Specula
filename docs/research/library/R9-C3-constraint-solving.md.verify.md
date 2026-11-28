# Verify: R9-C3-constraint-solving.md

Re-check done 2026-11-28. The checker re-searched the page-marked text of S2 (hoff05) and S3 (hoff06), re-downloaded S3 from Purdue (29,431 bytes, same as the copy read), and checked all three citations on Crossref.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | S1 citation, JCISE 11(2):021001, 2011, Bettig and Hoffmann | Crossref doi 10.1115/1.3593408 | VERIFIED; FIX applied (DOI added) |
| 2 | S1 not obtained | raw/bettig11.FAILED-not-a-pdf.html is a CGVLab HTML page | VERIFIED |
| 3 | S2 citation, CAD&A 2(5):655-663 | Crossref | VERIFIED |
| 4 | S2 green OA at hdl.handle.net/2117/83305 | OpenAlex open_access record | VERIFIED (record); file identity UNVERIFIABLE, as stated |
| 5 | S2 printed = PDF + 653 | p.8 header "661", p.9 header "662" | VERIFIED |
| 6 | S3 citation IJPLM 1(2):143-149 | Crossref (start page 143); S3 footnote p.1 gives pp.143-149 | VERIFIED |
| 7 | S3 URL and size | curl 200, 29,431 bytes | VERIFIED |
| 8 | Under, well, over definitions | S2 p.4; S3 p.1 | VERIFIED |
| 9 | Structural only; inconsistencies from values not covered | S2 p.4 | VERIFIED |
| 10 | α+β=90° example, Fudos and Hoffmann | S2 p.4 text; figure caption on p.5 | VERIFIED; FIX applied (figure page noted) |
| 11 | Vertex weights 2, 3, 1; edge weights 1 (distance), 2 (incidence) | S3 p.3 | VERIFIED |
| 12 | Deficiency 3 free, 0 fixed | S3 p.3 | VERIFIED |
| 13 | Global count can balance while parts are wrong | (mine) | Labelled (mine) |
| 14 | Cluster core, sequential extension, three-cluster merge | S3 p.3 | VERIFIED |
| 15 | Decomposition fails, then the solver announces no solution, which does not mean none exists | S3 p.4 | VERIFIED |
| 16 | Church-Rosser, order not critical | S3 p.5 | VERIFIED |
| 17 | Flow-based minimal dense subgraphs, may lack geometric sense | S2 p.7 | VERIFIED |
| 18 | Latham et al. identify over and under constraint; priorities solve over-constrained | S2 p.7 | VERIFIED |
| 19 | Analysis of equation systems classifies; Serrano selects a solvable subset | S2 p.9 | VERIFIED |
| 20 | Propagation fails on a loop, no guarantee; numerical combination | S2 p.7 | VERIFIED |
| 21 | Property list (sound, complete, competent, persistent, stable, efficient, robust, intensional) | S2 p.5 | VERIFIED |
| 22 | Doubly exponential | S2 p.5; S3 p.2 | VERIFIED |
| 23 | Root selection by order type; independent of order with triangle decomposition | S3 p.4-5 | VERIFIED |
| 24 | Sketch can stop guiding when dimensions change | S3 p.4 and p.5 | VERIFIED |
| 25 | Non-self-intersecting polygon requirement gives NP-hard steps | S3 p.5 | VERIFIED |
| 26 | Dragging can confuse the user; visual step choice needs sophistication | S3 p.5 | VERIFIED |
| 27 | Well-constrained maps to an under-determined equation system | S2 p.8 | VERIFIED |
| 28 | Numerical needs a near start from the sketch; little control over the solution | S2 p.8 | VERIFIED |
| 29 | Earlier draft: "least-squares style methods may fail even when a solution exists" | not found in S2 | FIX applied (replaced by what p.8 says: relaxation is slow; Newton-Raphson is local and fails on consistently over-constrained systems without least squares; homotopy is global, finds all solutions and is slower; Hel-Or soft constraints with a Kalman filter) |
| 30 | Section 7 lane design, including the deletion filter for a minimal infeasible subset | (mine; standard technique, not from these sources) | Labelled (mine) |

Fixes: rows 1, 10, 29.
WEAK: S1 (the brief's named source) is not read. Section 7's claim that the sources give no method for naming the single conflicting constraint covers only S2 and S3.
