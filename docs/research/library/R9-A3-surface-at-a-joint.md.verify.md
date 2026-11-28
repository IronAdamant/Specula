# Second check: R9-A3-surface-at-a-joint.md

Independent re-check, 2026-11-28.
- **Page cites.** Each was re-found by grep in freshly page-marked text of kavan08.pdf (author copy, 23 pp.), vaillant13.pdf (HAL, 12 pp. including the HAL cover) and vaillant14rv.pdf (author copy, 11 pp.).
- **Bibliography.** Checked with Crossref, OpenAlex and the HAL API, plus the HAL cover sheet.
- **Derivations.** The (mine) algebra was re-derived by hand. For two unit vectors at angle θ, the length of their mean is cos(θ/2). For weights w and 1-w, the squared length is w² + (1-w)² + 2w(1-w)cos θ, which is smallest at w = 0.5.

| row | claim | source location | result |
|---|---|---|---|
| 1 | Kavan TOG 27(4), Article 105 | Crossref (27(4), pp. 1-23); S3's bibliography cites "105:1-105:23" | VERIFIED |
| 2 | Vaillant 2013 TOG 32(4), Article 125; HAL Authorization licence | HAL cover (vaillant13 p.1); HAL API | VERIFIED |
| 3 | Vaillant 2014 TOG 33(6), pp. 1-11 | HAL API, OpenAlex | VERIFIED. The draft gave "Article 189", which could not be confirmed: **FIX** (removed; marked not confirmed) |
| 4 | HAL and IRIT copies of S3 truncated; author copy read | files on disk (.FAILED-truncated); pdfinfo errors | VERIFIED |
| 5 | Averaged points lie inside the arc | kavan08 p.2 | VERIFIED |
| 6 | Candy wrapper: 180° twist with 0.5/0.5 weights collapses to the joint; changing frames does not cure it | kavan08 p.3-4 | VERIFIED |
| 7 | cos(θ/2) closed form and weight formula | own derivation | VERIFIED by re-derivation (labelled mine) |
| 8 | LBS volume loss, elbow collapse | vaillant13 p.2, p.3, p.5, p.8 | VERIFIED |
| 9 | LBS handles scale and shear at no extra cost | kavan08 p.15 | VERIFIED |
| 10 | DQS is coordinate-invariant (bi-invariant) | kavan08 p.9-10 | VERIFIED |
| 11 | Bounds of 8.15° and 15.1% against ScLERP | kavan08 p.11 (draft said p.10-11; the values are on p.11) | VERIFIED (range acceptable) |
| 12 | DQS slightly slower than LBS; over 2x faster than log-matrix or spherical | kavan08 p.14, p.1 | VERIFIED |
| 13 | Non-rigid joints need two phases; about twice LBS's instruction count | kavan08 p.12-13, p.15 | VERIFIED |
| 14 | Antipodality; the less-than-180° assumption | kavan08 p.12 | VERIFIED |
| 15 | Flip from 179° to 181° | kavan08 p.15-16 | VERIFIED |
| 16 | DQS overgrowth at knee and elbow (S2 Figs. 10, 12) | vaillant13 p.8-9 | VERIFIED |
| 17 | Heat-diffusion weights; partition | vaillant13 p.4 | VERIFIED |
| 18 | HRBF, closure points, [0,1] mapping with 0.5 surface, compact support r | vaillant13 p.4-5 | VERIFIED |
| 19 | Three field requirements: smooth, closed, compact | vaillant13 p.4 | VERIFIED |
| 20 | Union (max) avoids elbow collapse; gradient operators; modified blend | vaillant13 p.5-6 | VERIFIED |
| 21 | σ = 0.35; 55° stop; relaxation after two projections; Laplacian at contacts | vaillant13 p.7 | VERIFIED |
| 22 | 32³ and 128³ grids; 20-70 bones; 10-35 MB; >60 and >12 fps; 170,000 vertices | vaillant13 p.8 | VERIFIED |
| 23 | Limits I1-I6 | vaillant13 p.9-10 (Fig. 14 on p.9; text on p.10) | VERIFIED |
| 24 | S3 three stages; ARAP with prescribed rotations; tangent-plane constraint | vaillant14rv p.6-7 | VERIFIED |
| 25 | Contact operators via biharmonic grids; earlier stopping unstable | vaillant14rv p.2-6 | VERIFIED |
| 26 | About 150°; belly button; 30-300 fps; biceps primitive | vaillant14rv p.1, p.7-8 | VERIFIED |
| 27 | Limits T1-T4 | vaillant14rv p.8-9 | VERIFIED |
| 28 | SMPL artefacts and pose blend-shape counts in the comparison table | R9-A1 S4 p.2, p.6 | VERIFIED (cross-file) |
| 29 | "DQS bulge not discussed in S1" | kavan08 full-text grep for "bulg" (only muscle-bulging context, p.1, p.3, p.12) | VERIFIED |
| 30 | Lane notes and expected looks at 90° | synthesis | Labelled (mine) |

Fixes: row 3 (unconfirmed article number removed).
WEAK: none beyond the labelled (mine) notes. The S3 author copy and the HAL file were not compared byte for byte (stated in the file).
