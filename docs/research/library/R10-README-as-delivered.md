# Round 10 reference library: Sections A to F plus the procedural-generation maths

Written 2026-11-28. This covers the round 10 brief (`brief.md`):

- A: angles at rest
- B: small parts
- C: spreads and limits
- D: how measurements move together
- E: children and older adults
- F: animals

It also has one combined maths note on drawing bodies from these numbers and enforcing the boundaries.

Rules followed:
- Each source gives the citation, a free legal copy (or says there is none), and the licence.
- Every number is retyped by hand with its page or table. Spreads are SDs, not CIs; where a source gives only a CI, IQR or SE, the note says so.
- Every note has a second check file (`.verify.md`). Most rows are regex checks against the saved source copy; the rest are labelled rows (mine, by eye, UNVERIFIABLE).
- Method and facts only: no data from SMPL, SCAPE, CAESAR, Basel Face Model, FLAME or AMASS. No data set was downloaded or fed into anything.
- Substitutes are labelled, my own derivations are marked "(mine)", and secondary sources are marked "Secondary". Where nothing measured exists, the note says so in its Gaps section.

## Files

| File | Topic | Check |
|---|---|---|
| library/R10-A-angles-at-rest.md | Ear protrusion and axis; Q angle, tibiofemoral and carrying angles, anteversion; foot progression, hallux valgus, arch index; spine and pelvis by age; hand resting joint angles; face profile angles | .verify.md: 79 VERIFIED rows, 4 UNVERIFIED or labelled |
| library/R10-B-small-parts.md | Iris (WTW) percentiles, Hertel exophthalmometry, fissures; nail plate sizes; breast and male chest landmarks; patella by MRI; muscle thickness by ultrasound; ultrasound fat at 14 sites | .verify.md: 48 VERIFIED rows |
| library/R10-C-spreads-and-limits.md | ROM with SDs (secondary) by sex and age; clavicle and scapula at rest; women's segment parameters (substitute); hip joint centre regressions (Hara); skinfold mean (SD) by sex and age (Campa 2025); hard limits (ANSUR II min and max, Guinness extremes) | .verify.md: 71 VERIFIED rows plus manual rows |
| library/R10-D-correlations.md | Face correlations and PCA (NIOSH panels), ear correlations, hand matrices (Greiner 1991 excerpts), angles with lengths, spine-pelvis-stance correlations | .verify.md: 57 VERIFIED rows (3 by eye on page images) |
| library/R10-E-children-and-older-adults.md | NHANES stature and mass by single year 2-17 and at 60+ (percentiles, approximate SDs (mine)); child ROM (CIs only); child hip joint centre; preschool ultrasound fat; spine and pelvis over 60 | .verify.md: 99 VERIFIED rows plus manual rows |
| library/R10-F-animals.md | Dog withers by breed and sex (1,155 dogs); cat long bones and indices; horse standing joint angles (Morna, n=50) and Konik segments (n=172); cattle head, ear, horn, tail, girth by sex (4 breeds); roe deer by habitat, sex and age (1,335); bunting wing, tail, tarsus, bill | .verify.md: 76 VERIFIED rows plus 7 manual rows |
| library/R10-MATH-procedural-generation.md | MVN draws with Cholesky; conditioning on stature and sex; von Mises against normal for angles; log-normal from mean and SD; truncated normal; Mahalanobis and chi-square bounds against clamps against rejection; nearest correlation matrix (Higham); allometric scaling for animals; a step-by-step recipe (mine) | .verify.md: 49 VERIFIED rows plus 7 manual rows |
| blocked-downloads-ALTERNATIVES.md | What could not be had and what stands in for it | - |
| brief.md | Copy of the round 10 brief | - |

## Coverage map against the brief

| Brief asks | Answer, in short | Where |
|---|---|---|
| A1 ear on the head | Concho-mastoid angle 45.0 (10.4) and 31.1 (7.1) deg, helix-mastoid distance at two levels, long-axis tilt 25 and 27 deg (range, no SD), left-right correlations. Helix distance at the top and lobe, and ear height against the brow: none found | A, A1 |
| A2 limbs' alignment | Q angle, tibiofemoral angle, femoral anteversion, pelvic tilt by sex with SD and range (Nguyen 2009); carrying angle (one radiographic study; Saudi values UNVERIFIED). Radiographic HKA: closed source only | A, A2 |
| A3 foot | Foot progression walking, tibial torsion, hallux valgus (ROAD cohort), arch height index (women). Standing foot angle and per-toe length shares: none found | A, A3 |
| A4 trunk and neck | Thoracic kyphosis, lumbar lordosis and pelvic tilt by sex and age band; craniovertebral angle. Male shoulder slope and rib slope: none found | A, A4 |
| A5 hand at rest | Resting MCP, PIP and DIP angles per digit with SDs. Thumb abduction and rotation, finger spread: none found | A, A5 |
| A6 face | Nasofrontal, nasolabial, mentolabial and nasal angles by sex. Gonial angle: hosts would not serve the PDF (UNVERIFIED values only) | A, A6 |
| B1 eye | Iris (WTW) percentiles by sex and age; Hertel values; one new adult fissure row (round two B9 has the rest). Fissure SDs by age band: not found | B, B1 |
| B2 nail | Nail width and length by finger and sex from search summary (tables are images, UNVERIFIED) | B, B2 |
| B3 breast and chest | Clinical landmarks for young women (n=54) and men (n=150). Base width, projection and size bands: none open | B, B3 |
| B4 knee | Patella size by sex (two MRI studies). Knee breadth and girth at the kneecap: not found | B, B4 |
| B5 muscle thickness | Abdominal wall, deltoid, gluteus medius and thigh by ultrasound with SDs. Pectoralis, trapezius, latissimus and gluteus maximus: not found | B, B5 |
| B6 fat map | 14-site ultrasound (figure readings, no SDs). No continuous open map | B, B6 |
| C1 ROM with SDs | Secondary SDs from the CDC data set (Soucie 2011 primary closed); child CIs in E | C, C1; E, E3 |
| C2 clavicle and scapula | Yoshida 2022 (79 people, CT; medians and IQR), Roy 2007 (15 people, SDs) | C, C2 |
| C3 women's segment masses | No open source meets the bar of more than 15 women with SDs; labelled substitute (9 women) | C, C3 |
| C4 joint centres in women | Hip joint centre regressions by sex and age (Hara 2016). Knee and ankle in women: none found | C, C4 |
| C5 skinfolds | Mean (SD) at 7 ISAK sites by sex and 5-year age band, 2,507 adults | C, C5 |
| C6 hard limits | ANSUR II min and max; Guinness extremes for stature and mass | C, C6 |
| D1 face together | NIOSH panel correlations, PCA loadings and a published ellipse bound; face dimensions by sex with min and max | D, D1 |
| D2 ear together | Length and width r = 0.660; axis and antihelix r = 0.417. Protrusion correlations: none | D, D2 |
| D3 hand together | Greiner 1991 matrices exist (pp. 218-269); excerpts and PCA retyped | D, D3 |
| D4 angles with lengths | Q angle regressed on tibiofemoral angle, anteversion and the femur-to-tibia length ratio by sex (the length ratio is not significant; 7 alignment measures explain only 18-19%). Other angle-length pairs: none found | D, D4 |
| D5 spine, pelvis, stance | Pelvic incidence, lumbar lordosis, kyphosis and SVA correlations (Wakayama and others). Some figure values UNVERIFIED | D, D5 |
| E children and older adults | Stature and mass by single year 2-17 and at 60+; child ROM (CIs), child hip joint centre, preschool fat, spine over 60. Child skinfolds and segment lengths (NHANES Tables 24-33): not reached | E |
| F animals | Within-breed spreads for dog withers, cat bones, horse segments and standing joint angles, cattle head, ear, horn and tail, roe deer frame, bird wing, tail and tarsus. Ear carriage, tail carriage, dog and cat standing angles: none measured found | F |
| Maths and boundaries (owner's request) | MVN with Cholesky and conditionals, von Mises, log-normal, truncated normal, Mahalanobis and chi-square against clamp against rejection, Higham repair, allometry, with worked numbers (mine) | MATH |

## Not found or not reachable (summary; details in each note's Gaps)

- Ear: helix-to-head distance at the top and lobe; protrusion correlations.
- Limbs: an open radiographic HKA source.
- Foot: standing foot angle; per-toe length shares.
- Trunk: male shoulder slope; rib slope.
- Hand: thumb abduction and rotation at rest.
- Face: gonial angle (only UNVERIFIED values).
- Breast: base width and projection.
- Knee: breadth and girth at the kneecap.
- Muscles: several muscles.
- Fat: an MRI fat map with SDs.
- Segments: women's segment parameters with more than 15 women (C3); knee and ankle joint centres in women.
- Children: skinfolds and segment lengths from NHANES Tables 24-33 (host blocks curl; the fetch tool truncates the report); child ROM SDs; ROM over 70.
- Animals: dog and cat standing angles; ear and tail carriage angles for any species; dog per-breed segment SDs; cat outer measures; deer other than roe deer; any bird beyond one passerine.
