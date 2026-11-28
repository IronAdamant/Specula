# R9-B3 verify: second, independent check

Re-check done 2026-11-28 against the downloaded copies: Perfler et al. (`ear_bezier.pdf`, 11 pp.), Reed et al. (`ear_reed25.pdf`, 13 pp.), Kent et al. (fetched text, `kent21.webfetch.txt`) and Engineer, Japatti et al. (Europe PMC XML PMC6018292). Each claim was searched for again in the text. Citations were checked against Crossref.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | Perfler et al. 2025, CBM 188:109817, five authors | Crossref | VERIFIED |
| 2 | Reed, Vallier, Bonifas 2025, IJIE 107:103738 | Crossref; DOI added | VERIFIED |
| 3 | S4 authors | Crossref lists Engineer P.J. first, then Japatti S.R., Reddy B M., Tiwari A.U., Siddegowda C.Y., Hammannavar R.B.; first page 42 | FIX applied: the citation had the wrong first author and omitted two authors |
| 4 | Kent et al. 2021, IJM 39(3):742-746 | Article header in the fetched text | VERIFIED (not checked in Crossref) |
| 5 | 162 parameter dimensions | Perfler PDF p.2 | VERIFIED; the 9×15+17+10 breakdown is labelled (mine) |
| 6 | 15 numbers per curve: 6 location, 8 rotation (two quaternions), 1 scale | PDF p.4 | VERIFIED |
| 7 | 17 shape keys; names; range [−1,1] except 15 and 16 in [0,1] | PDF p.3, Table 1 | VERIFIED; the source's own wording is muddled, now quoted |
| 8 | Default mesh = WiDESPREaD average; flare 28.53°, rotation 24.01° | PDF p.2 | VERIFIED |
| 9 | Curve weights assigned manually | PDF p.4 | VERIFIED |
| 10 | Curves may lie partly outside the deformed mesh | PDF p.4 | VERIFIED |
| 11 | 20 manual fits: 4 ITA + 3 ARI + 3 SOU real ears and 10 extreme WiDESPREaD ears | PDF p.5-6 | VERIFIED (FIX applied: an earlier draft said "twenty real ears") |
| 12 | 1.5 ± 0.2 (abstract) against 1.5 ± 0.4 (results; 0.2 is from ref. [20]); completeness 87 ± 4%, 66 ± 5% | PDF p.1, p.6 | VERIFIED |
| 13 | Largest errors on the back of the ear | PDF p.8 | VERIFIED |
| 14 | Synthetic test: 10,000 ears; 7,200 train, 800 validation, 2,000 test; results say "800 unseen ears", 0.3 ± 0.1 mm | PDF p.9-10 | FIX applied: the first draft treated 800 against 2,000 as a plain contradiction; it is now explained as validation against test set |
| 15 | Rotations uniform within ±60°; only one parameter group; limited to synthetic ears | PDF p.9-10 | VERIFIED |
| 16 | Rendering seconds per sample; meshes microseconds | PDF p.9 | VERIFIED |
| 17 | Licence CC BY 4.0 | Perfler PDF p.1 (read while writing) | VERIFIED during writing |
| 18 | Reed: 331 ears, 224 adults 18-81; convenience sample | Reed PDF p.1-2 | VERIFIED |
| 19 | PC1 lateral position, nearly half; size at PC4; 8 PCs 91%, 14 PCs 95% | PDF p.5 | VERIFIED |
| 20 | Female ear smaller, angled out at top; age enlarges, larger effect in men; BMI affects position and angle | PDF p.5 | VERIFIED |
| 21 | Limits: region of origin unknown; prone/supine; eardrum within about 2 mm; patients | PDF p.8-10 | VERIFIED (FIX applied: the first draft listed limits not stated in the paper, e.g. "small sample") |
| 22 | Reed copy "All rights are reserved" | PDF p.1 footer | VERIFIED |
| 23 | Kent: 56 men, 59 women, 18-30 | Kent Methods | VERIFIED |
| 24 | Definitions of length, width, lobule, concha | Kent Table II | VERIFIED |
| 25 | Table III values (24 cells) | Kent Table III | VERIFIED; the values are in cm despite the mm caption (abstract gives mm values that match ×10) |
| 26 | Indices 54.10, 52.42; conchal 78.01, 76.18 | Kent Table IV | VERIFIED |
| 27 | Spanish summary repeats lobule-length figures as widths | Kent RESUMEN | VERIFIED |
| 28 | Inclination angle given in "mm" | Kent Discussion | VERIFIED |
| 29 | Shares (0.42, 0.43, 0.30, 0.28, ...) | Derived from Table III | Recomputed: 0.424, 0.431, 0.297, 0.276, 0.308, 0.302, 0.595, 0.622, 0.547, 0.521. Labelled (mine) |
| 30 | Engineer/Japatti: 505 adults 18-64; not significant by sex; length grows faster than width | Japatti Discussion and Conclusion | VERIFIED |
| 31 | Japatti licence CC BY-NC-SA 4.0 | Europe PMC XML licence element (read while writing) | VERIFIED during writing |
| 32 | Kent licence | Not checked | UNVERIFIABLE here (SciELO page not opened); marked "not verified" in the file |

Fixes: rows 3, 11, 14 and 21; Reed DOI added (row 2).

WEAK: Kent is one small Turkish student sample with obvious editing errors, so the shares are indicative only; no tragus, helix-rim or antihelix share was found; Reed's posting status is unverified, so it is used only for stated findings.
