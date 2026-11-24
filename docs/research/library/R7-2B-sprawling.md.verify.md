# Verify: R7-2B-sprawling.md (adversarial second check, 2026-11-24)

**Method:** the same number-presence script against the cached texts (V1 pdftotext, H1 XML plus the S1 Table spreadsheet, J1, V3, V4, C1 and the C2 abstract), followed by hand checks and an independent recomputation of every ratio.

| Check | Result |
|---|---|
| V1 Table 1, 6 species × 9 values | match (re-read from the table text) |
| V1 n (5; komodoensis 2), method (nearest 0.5 mm, preserved specimens) | match |
| H1 S1 Table, 3 geckos × 9 values with brackets; 3 anoles SVL and HTotal | match the spreadsheet |
| H1 spread type | The S1 Table legend says "95% confidence intervals are shown in parentheses". The file says CI, not SD |
| V3 Table 1a, 4 groups × 11 traits | match; definitions quoted from the Methods |
| V4 SCL and mass, with ranges | match |
| C1 methods and Table 1 posture ranges | match |
| All ratios (**computed**) | Recomputed; every number missing from the sources is a computed ratio or a DOI fragment |
| Citations | Crossref checked: V1 30(2): 219; H1 authors (5; Swartz is not an author); J1 202(2): 143-159; V3 32(4): 445-458; V4 authors Duro, Sönmez, Gündemir, Jashari, Szara; C1 206(23): 4327-4340; C2 201(18): 2559-2574 |

**Errors found and fixed:**
1. The gecko rows first had the hind-limb raw values under the forelimb headings (the ratios were right). Columns were swapped back and re-checked.
2. V4 was first described as "measured by hand". The source says the tortoises were "collected by hand"; corrected.
3. C1's Table 1 row was first described as "many strides per animal". The stride count per animal was not copied; reworded.

**Remaining weaknesses:**
- V1 has no SD.
- The anole values are literature compilations with no n.
- *Dipsosaurus* has no segment data.
- The C2 duty factor is an unverified extract and is kept out of the data.
- No skink, iguana, tail length or *Trachemys* data.
