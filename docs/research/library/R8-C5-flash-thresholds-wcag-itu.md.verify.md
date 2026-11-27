# Second check: R8-C5-flash-thresholds-wcag-itu.md

Re-read against raw/round8/wcag22.txt, wcag22-understanding-2.3.1.txt, wcag-tech-G176.txt, wcag-tech-G15.txt, wcag22-errata.txt, itu-bt1702-3.flow.txt and iris-readme.md.

| row | claim | source location | result |
|---|---|---|---|
| 1 | WCAG 2.2 W3C Recommendation 12 Dec 2024; editors; dated URL | wcag22.txt header | VERIFIED |
| 2 | SC 2.3.1 wording and the non-interference note | wcag22.txt SC 2.3.1 | VERIFIED |
| 3 | Definition of flash | glossary "flash" | VERIFIED |
| 4 | Thresholds: three per second, or 0.006 sr / 25% of a 10 degree field | glossary "general flash and red flash thresholds" | VERIFIED |
| 5 | General flash: 10% of max, darker below 0.80 | same | VERIFIED |
| 6 | Red flash undefined in normative text | same | VERIFIED |
| 7 | Exception: checkerboard squares under 0.1 degree | same | VERIFIED |
| 8 | Note 1: 341 x 256 at 1024 x 768; 15-17 in at 22-26 in; 75-85 ppi against 96 | same, Note 1 | VERIFIED |
| 9 | Note 2: transition and flash | same, Note 2 | VERIFIED |
| 10 | Note 3: R/(R+G+B) at least 0.8; more than 0.2 in (u', v'); ISO 9241-391 | same, Note 3 | VERIFIED |
| 11 | Note 4: tools; 3 or fewer flashes passes automatically | same, Note 4 | VERIFIED |
| 12 | Relative luminance formula, 0.04045, 12.92, 2.4 | glossary "relative luminance" | VERIFIED |
| 13 | 0.03928 before May 2021, no practical effect | relative luminance Note 2 | VERIFIED |
| 14 | sRGB assumption; dithering rule | Notes 3-4 | VERIFIED |
| 15 | u', v' formulas and XYZ matrix | Understanding, "Note (new in WCAG 2.2)" | VERIFIED |
| 16 | Y row equals luminance weights to four decimals | my comparison | VERIFIED |
| 17 | 10 degree field from broadcast specs; UK TV origin | Understanding, Intent | VERIFIED |
| 18 | CSS px reading; 87,296 | Understanding, Intent | VERIFIED |
| 19 | G176: 21,824 px² single flash; Formula 2; 0.1745 and 0.1309; 2203 (1080p) and 1566 ppi rules | G176 Description | FIX: draft called 1566 the plain "720p" rule; G176 says 720p widescreen, usually 1365 x 768. Text corrected |
| 20 | My 0.006 sr check (0.00571 and 0.00598) | my arithmetic (python) | VERIFIED |
| 21 | Viewing-distance caveat; largest scale; looping; HDR version | Understanding, Intent and note | VERIFIED |
| 22 | ITU edition, approval 2023-11-22, in force, free download, © ITU | ITU page; PDF 1-2 | VERIFIED |
| 23 | Guideline 1: 160 / 20 cd/m², 1/17 Michelson, red, 25% of screen, more than 3 per second, 360/334 ms, fast cuts, 5 s | ITU PDF 5 | VERIFIED |
| 24 | Note 6 continuity 1/17 | ITU PDF 6 | VERIFIED |
| 25 | Peak 200 / 1000 / 10000 cd/m² | ITU PDF 5-6, 8 | VERIFIED |
| 26 | Table 1; V = (D − 64)/876; 400 gives 20.1, 863 gives 160.4 | ITU PDF 9-10 | VERIFIED |
| 27 | Patterns: more than 5 pairs, 40% / 25%, flow exemption | ITU PDF 5, 8 | VERIFIED |
| 28 | Annex 4: 10-30 Hz, 20 dB; Annex 5: dark room, under 20, 2 m | ITU PDF 11-12 | VERIFIED |
| 29 | Annex 3 "needs further study"; red needs definition; Note 5 analysers | ITU PDF 6, 10 | VERIFIED |
| 30 | WCAG quotes 20 and 160 cd/m² and 1/17 citing BT.1702 | Understanding, Intent | VERIFIED |
| 31 | My mapping 20/200 = 10%, 160/200 = 0.80 | my arithmetic | VERIFIED (inference labelled) |
| 32 | Area comparison 196,608 against 21,824 (about 9x) | my arithmetic | VERIFIED |
| 33 | IRIS behaviour: 0.1, ±20, verdicts, 5 s, 0.5 s pattern, disclaimer | iris-readme.md | VERIFIED |
| 34 | IRIS licence BSD-3; last push 2025-01-14 18:49 UTC (2025-01-15) | GitHub API | VERIFIED |
| 35 | Errata: only editorial (#3756) | wcag22-errata.txt | VERIFIED |
| 36 | ITU printed page = PDF − 2 | ITU PDF 3 shows "1" | VERIFIED |

| 37 | Annex 3 page | ITU flow text: Annex 3 runs only on PDF 10 | FIX: draft said PDF 10-11; corrected to PDF 10 |

Fixes: rows 19 and 37.
