# Second check: R9-A2-muscle-and-fat-shapes.md

Independent re-check, 2026-11-28.
- **Page cites.** Each was re-found by grep in freshly page-marked text of scheepers96.pdf (OCR text, 334 pp.), wilhelms97sg.pdf, plos13fat.pdf and jpa07fat.pdf.
- **Page offset.** The Scheepers printed-page offset was re-derived from the running heads (PDF p.141 shows "116", PDF p.143 shows "118", so printed = PDF - 25).
- **Tables.** The fat tables were re-read from the layout text. The "6" for "±" was checked against S3's footnote ("Median 6 interquartile range").
- **Licences.** The PLOS licence was re-checked in Crossref (CC BY 4.0). The OhioLINK rights text was re-read from the record page.

| row | claim | source location | result |
|---|---|---|---|
| 1 | Scheepers 1997 SIGGRAPH pp. 163-172; Wilhelms 1997 pp. 173-180; PLoS ONE 8(8) e72002; JPA 26(4) 513-516 | Crossref | VERIFIED |
| 2 | S1 has no legal free copy; mirror not used | ACM bot page; search | VERIFIED as a search result: WEAK (absence not provable) |
| 3 | S1b is free on OhioLINK; rights "research and educational purposes... All Rights Reserved" | osu2.html record page | VERIFIED |
| 4 | UCSC TR link unreachable from the box today | curl timeouts (3 attempts) | VERIFIED; the TR content is UNVERIFIABLE today |
| 5 | Fusiform = ellipsoid keeping volume and height/width ratio | scheepers96 p.142-144 | VERIFIED |
| 6 | Parallel belly = elliptic cylinder with caps; triangular belly | scheepers96 p.143-145 (Fig. 32) | VERIFIED |
| 7 | Tension parameter; k = 1.6 in the code fragment | scheepers96 p.151-152, p.155 | VERIFIED |
| 8 | Multi-belly; general bicubic tube; volume by three truncated cones | scheepers96 p.156-165 | VERIFIED |
| 9 | Tendons fixed in length; generalised cylinders | scheepers96 p.141, p.167-169 | VERIFIED |
| 10 | Cubic fall-off with radius R; summed field | scheepers96 p.199-200 | VERIFIED |
| 11 | Swept elliptic patch skin; control points pushed until F = 0; flexion-side pinning; R tests | scheepers96 p.213-217 | VERIFIED |
| 12 | Table 4 superficial muscles, incl. deltoid, pectoralis major, gluteus maximus, gastrocnemius | scheepers96 p.86-88 | VERIFIED |
| 13 | Fat thicker on trunk than limbs; sex differences; major and minor pads | scheepers96 p.91 | VERIFIED |
| 14 | Table 5 fat pads and sex notes (paraphrased) | scheepers96 p.96 | VERIFIED |
| 15 | Lack of fat, not muscularity, gives definition; high-waisted female outline | scheepers96 p.95 | VERIFIED |
| 16 | Failure cases F1-F7 | scheepers96 p.150, 159-160, 164, 202, 204, 216, 226, 235-236 | VERIFIED |
| 17 | Wilhelms: 2 origins + 2 insertions; 7 sections / 8 slices; 8 points per slice; no tendons | wilhelms97sg p.2-3 | VERIFIED |
| 18 | Pivot; Table 1 parameters; pectoralis Fig. 4A | wilhelms97sg p.3-4 | VERIFIED |
| 19 | Width and thickness scaled by rest/present; paper claims exact volume between parallel slices | wilhelms97sg p.4 | VERIFIED (quote paraphrased). The inconsistency argument is labelled (mine) and flagged unresolved |
| 20 | Voxel skin: 225 grid, 200 density, SD 3.2, threshold 35, about 75,000 vertices | wilhelms97sg p.4, p.7 | VERIFIED |
| 21 | Tri-affine anchors, Newton inverse, bumps if a single slice frame is used | wilhelms97sg p.4-5 | VERIFIED |
| 22 | Spring model: area-based stiffness, Ca = 0.10, 0.1 compression factor, 90% rest length, sphere collision | wilhelms97sg p.5-6 | VERIFIED |
| 23 | About 30 iterations; 5-10 often acceptable | wilhelms97sg p.7 | VERIFIED |
| 24 | Failure cases F8-F12 | wilhelms97sg p.4, p.6-7 | VERIFIED |
| 25 | 15 sites, right side, standing; CT r = 0.99; CV 1.9-12.2% | plos13fat p.5 (draft said p.8) | **FIX** applied |
| 26 | S3 tables: medians and IQRs for male and female controls | plos13fat p.2-3 | VERIFIED (all 30 values re-read) |
| 27 | S3 cut-offs: men neck 2.3 / trunk 15.5; women upper back 3.3 / arms 15.9 | plos13fat p.1 | VERIFIED |
| 28 | S4 means and SDs at all 15 sites, by sex; p < 0.001 throughout | jpa07fat p.2 Table 1 | VERIFIED |
| 29 | S4 predictors: 87.1% (men) and 78.5% (women) | jpa07fat p.1-3 | VERIFIED |
| 30 | S4's earlier Estonian study found different predictors | jpa07fat p.3 | VERIFIED. The draft wrongly took this sentence as describing S4's own sample: **FIX** applied (S4 recruitment now marked unverified) |
| 31 | No open source gives a continuous fat-thickness map | search in this round | WEAK (search-based) |
| 32 | Muscles-that-matter mapping and lane notes | synthesis | Labelled (mine) |

Fixes: row 25 (S3 p.8 changed to p.5 in four places); row 30 (misread of S4 p.3 corrected, with the disagreement added).
WEAK: rows 2 and 31 (absence based on searches).
