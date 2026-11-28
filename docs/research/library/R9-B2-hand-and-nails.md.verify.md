# R9-B2 verify: second, independent check

Re-check done 2026-11-28 against the downloaded copies: Albrecht et al. (`albrecht03.pdf`, EG DL, 13 pp.), Rhee et al. (`rhee06.pdf`, 9 pp.), Murdan (`nail_murdan.pdf`, UCL accepted manuscript, 17 pp.) and Jung et al. (Europe PMC XML PMC4659990). Each claim was searched for again in the page-marked text. Tables II and IV of Murdan were re-read from the text layer, which keeps the numbers in order. Citations were checked against Crossref, and against OpenAlex for the EG paper.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | Albrecht, Haber, Seidel 2003, SCA pp. 98-109 | OpenAlex title and year; printed folios 105-107 on PDF p.8-10 confirm offset +97; Crossref has no record for the EG DOI | VERIFIED (pages from the printed folios) |
| 2 | Rhee, Neumann, Lewis 2006, I3D, doi 10.1145/1111411.1111417 | Crossref (first page 27) | VERIFIED; end page 34 from the copy's own layout (not given by Crossref) |
| 3 | Murdan 2011, IJCS 33(6):509-513 | Crossref | VERIFIED |
| 4 | Jung et al. 2015, APS 42(6):753-760, six authors | Crossref | VERIFIED |
| 5 | Skin 3,000 triangles from a plaster-cast scan; 29 bone meshes, scaled | Albrecht PDF p.4 | VERIFIED |
| 6 | Index and middle CMC fixed; ring and little CMC two small DOFs; DOF limits from the literature; wrist-bone joints ignored | PDF p.4 | VERIFIED |
| 7 | Finger MCPs and thumb CMC modelled with three DOFs because of coupled rotation | PDF p.4 | VERIFIED |
| 8 | 26 feature points with the listed breakdown; 16 enough | PDF p.8 | VERIFIED |
| 9 | Ruler calibration by two clicks | PDF p.9 | VERIFIED |
| 10 | Fitting plane, diameter alignment, heights copied and scaled; thickness proportional to size | PDF p.9 | VERIFIED |
| 11 | Biharmonic RBF minimising bending energy; Bookstein thin-plate | PDF p.9 | VERIFIED |
| 12 | Skin, bones, joints and muscle-grid control points warped; connectivity unchanged | PDF p.9 | VERIFIED |
| 13 | Female reference warped to a man's and a child's hand; about a minute to pick points | PDF p.10 | VERIFIED |
| 14 | Contraction values must be re-tuned (torques, inertia, angles) | PDF p.10 | VERIFIED |
| 15 | "not always a straightforward process"; optimiser in progress | PDF p.9-10 | VERIFIED |
| 16 | 1,500 nodes, 4,500 springs; 5-10 fps; skin never loses integrity | PDF p.10 | VERIFIED |
| 17 | Future: muscles should move bones (needs tendons); gravity; collision detection; textures not trivial | PDF p.10 | VERIFIED |
| 18 | Rhee: crease at centre of curvature of the proximal bone's head; Bugbee and Botte 1993 | Rhee PDF p.3 | VERIFIED |
| 19 | DIP/PIP at crease ∩ axis, lower of two creases; MCP at about 1/3 of Si from H; wrist midpoint; thumb CMC from radius axis | PDF p.5-6 (page images, layout text) | VERIFIED during writing from page images; the 1/3 rule depends on that image reading |
| 20 | 21 joints including fingertips and wrist; DOF list | PDF p.5 | VERIFIED |
| 21 | 30 curve segments; arc-length matching; small accumulated errors | PDF p.6-7 | VERIFIED |
| 22 | Test mesh 16,423 vertices | PDF p.7 | VERIFIED |
| 23 | Thickness proportional to hand length, "may result in an erroneous depth of the joints"; CMC not guaranteed | PDF p.8 | VERIFIED |
| 24 | Fails with corns, calluses, scars, missing creases; thresholds need good lighting | PDF p.7-8 | VERIFIED |
| 25 | Skin deformation and texture blending out of scope; manual blending | PDF p.2 | VERIFIED |
| 26 | Murdan sample 92 men, 90 women, 21-90; hand length and breadth | Murdan PDF p.3-4, Table I p.10 | VERIFIED (from summary notes; Table I numbers re-seen in Table IV hand rows, consistent) |
| 27 | Radius gauges; mid-way along the nail excluding the free edge; topmost part; not perfect circles; one operator | PDF p.4 | VERIFIED |
| 28 | Table II dextral values and SDs (all 20 cells) | PDF p.11 | VERIFIED, cell by cell |
| 29 | Sinistral group 10 men, 8 women | PDF p.11 caption | VERIFIED |
| 30 | Table IV age values (all 30 cells) | PDF p.13 | VERIFIED, cell by cell; hand length and breadth also similar across ages |
| 31 | Sex explains 26-33%; further 10-18%; hand length p > 0.2 | PDF p.5-6 | VERIFIED |
| 32 | Dominance significant in dextral (p < 0.01) not sinistral | PDF p.6 | VERIFIED |
| 33 | No normal values for longitudinal curvature; methods exist | PDF p.3 | VERIFIED |
| 34 | Jung: 300 adults, 30 per sex per decade, 20s-60s | Jung Methods | VERIFIED |
| 35 | Width and length definitions | Jung Methods | VERIFIED |
| 36 | Radii and W/L ratios with SDs | Jung Discussion | VERIFIED |
| 37 | Height no correlation; weak hand B/L correlation (all fingers women, left only men) | Jung Results | VERIFIED |
| 38 | Age p < 0.001; dominant flatter; significance pattern by handedness and sex | Jung Results | VERIFIED; added the right-handed women little-finger exception |
| 39 | Smaller radii than England, linked to smaller DIP width and hand breadth | Jung Discussion | VERIFIED |
| 40 | Abstract wording "highest value of transverse fingernail curvature" | Jung Abstract | VERIFIED |
| 41 | Jung licence CC BY-NC 3.0 | Europe PMC XML licence element (read while writing) | VERIFIED during writing |
| 42 | Sagitta example | Derived | Labelled (mine) with an assumed width; not a source number |

Fixes: row 38 (added the exception for right-handed women's little fingers, and Jung's gauge range and single measurer).

WEAK: row 19, because the MCP "about one third" rule depends on a page-image reading of an interleaved two-column page; the Murdan UCL repository licence is not stated on the PDF; Jung's absolute nail widths and lengths are in table images and were not extracted.
