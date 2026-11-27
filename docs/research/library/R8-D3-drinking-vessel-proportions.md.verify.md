# Second check: R8-D3-drinking-vessel-proportions.md

Re-read against met-drinking-vessels.csv (recomputed), parpinello18.flow.txt, wp-Wine_glass.wiki, attwood12.txt and the ISO abstract page.

| row | claim | source location | result |
|---|---|---|---|
| 1 | Met API data CC0 | metmuseum.github.io text ("Creative Commons Zero") | VERIFIED |
| 2 | Query method; 200 hits per term; exact objectName; Overall H and D | met_ratios.py; met.log (tumbler 8, beaker 63, goblet 37, wine glass 12, mug 9, cup 0) | VERIFIED |
| 3 | Medians, IQRs, ranges, glass-only medians in the D3-1 table | recomputed from the CSV with Python statistics.quantiles | VERIFIED |
| 4 | Silver tumbler ca. 1691 is the 0.58 minimum | CSV objectID 9788 | VERIFIED |
| 5 | Sample biased to European decorative arts; wine glasses mostly Modern and Contemporary | department counts from the CSV | VERIFIED |
| 6 | Parpinello citation, CC BY 4.0 | Crossref licence; PDF p. 7 licence line | VERIFIED |
| 7 | Table 1 values (six glasses) | S2 PDF 3 | VERIFIED |
| 8 | Measured following Hirson et al. 2012; ISO glass by Bormioli | S2 PDF 2 (Section 2.3) | FIX: page was given as PDF 3 in the check; the text is on PDF 2 (the main file cites only Table 1, PDF 3, which is correct) |
| 9 | H/max D and H/opening columns | my arithmetic | VERIFIED |
| 10 | "Height" is bowl height | inference from 94.5 against 100 nominal | WEAK: S2 does not define the datum in the text read |
| 11 | ISO dimensions 46/100/55/65/9/65 mm; 210-225 mL; 50 mL pour; 9% lead | Wikipedia §ISO wine tasting glass | VERIFIED against Wikipedia; WEAK as to the standard itself (not read) |
| 12 | ISO abstract wording; 3 pages; confirmed 2022 | iso.org/standard/9002 | VERIFIED |
| 13 | ISO ratios 1.54, 2.38, 35%, 0.71 | my arithmetic | VERIFIED |
| 14 | "9 mm foot diameter" is really the stem | inference; vendor pages agree (not cited) | WEAK |
| 15 | Attwood: n = 159; 12 fl oz; 60% slower from straight when full; not half-full; not soft drinks; curved midpoint misjudged | attwood12.txt Abstract and Methods | VERIFIED |
| 16 | Attwood licence CC BY | attwood12.xml licence block | VERIFIED |
| 17 | MPRG and POTS not read; failures kept | raw/round8 *.FAILED-* files | VERIFIED |
| 18 | Summary table | rows 3, 9, 13 | VERIFIED |

Fixes during the check:
- **Removed** a sentence that credited a "warning" to the unread MPRG Guide (it came from a search snippet only).
- **Replaced** "ISO abstract and preview" with the iso.org abstract actually read.

WEAK: rows 10, 11 (standard), 14.
