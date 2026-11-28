# R9-B2 A hand and its nails: anatomical hand models from measurements, and nail-plate numbers

Round 9, section B. Access date for every source: **2026-11-28**. PDFs were read with poppler `pdftotext`, with page images where tables or two-column layout needed checking. Europe PMC XML was used where no PDF downloaded; those sources have **no page numbers** and are cited by section. Everything here is in my own words. **Method only:** no code, scan, mesh or skeleton file was taken from any source. Derivations marked **(mine)** are my own.

**Questions this item answers for the lane** (brief B2):
- How are the palm's surface and the finger pads built from the bones and the delivered A9 dimensions?
- What shape does a knuckle take when bent?
- Where do the creases fall, as shares of each segment?
- What are the nail's numbers: length, breadth, and curvature across and along, by finger and sex?

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Albrecht I., Haber J., Seidel H.-P. (2003). "Construction and animation of anatomically based human hand models." *Proc. ACM SIGGRAPH/Eurographics Symposium on Computer Animation (SCA 2003)*, pp. 98-109. doi:10.2312/SCA03/098-109 | Eurographics Digital Library: https://diglib.eg.org/server/api/core/bitstreams/0e124ca1-e211-4cfa-a844-cae0641769a5/content (13 pp., the paper plus a colour plate) | © The Eurographics Association 2003 (printed on each page); free download from EG DL | Yes, all pages. **Printed page = PDF page + 97** (PDF p.8 is printed p.105) |
| S2 | Rhee T., Neumann U., Lewis J.P. (2006). "Human hand modeling from surface anatomy." *Proc. I3D 2006*, pp. 27-34. doi:10.1145/1111411.1111417 | Co-author J.P. Lewis's site: https://scribblethink.org/Work/Pdfs/Rhee06_I3D.pdf (9 pp.) | ACM copyright; author-posted | Yes, all pages (p.5-6 also as page images, because the two-column text interleaved) |
| S3 | Murdan S. (2011). "Transverse fingernail curvature in adults: a quantitative evaluation and the influence of gender, age, and hand size and dominance." *Int. J. Cosmetic Science* 33(6):509-513. doi:10.1111/j.1468-2494.2011.00663.x | UCL Discovery accepted manuscript: https://discovery.ucl.ac.uk/id/eprint/1351489/ (17 pp. manuscript) | Accepted manuscript in an institutional repository; Wiley copyright on the published version. Repository licence not stated on the PDF (**unverified**) | Yes, all 17 pages; Table II also checked as a page image |
| S4 | Jung J.W., Kim K.S., Shin J.H., Kwon Y.J., Hwang J.H., Lee S.Y. (2015). "Fingernail configuration." *Archives of Plastic Surgery* 42(6):753-760. doi:10.5999/aps.2015.42.6.753 | Europe PMC PMC4659990 (XML full text). The journal-site PDF links failed from the box | **CC BY-NC 3.0** | Yes, the text. **The tables are images in the XML and could not be read**, so nail widths and lengths in mm are **not extracted** |

## 1. Albrecht, Haber, Seidel (2003): a reference hand warped to a person (S1)

**The reference model** (S1 PDF p.4, printed p.101):
- **Skin.** A 3,000-triangle mesh from a scan of a plaster cast of one woman's hand.
- **Bones.** 29 bone meshes (hand and forearm) from a public skeleton, scaled to fit the skin.
- **Muscles.** Virtual muscles between skin and bone.
- **Tissue.** A mass-spring network linking skin, bone and muscle.
- **Joints.** A hierarchy with a local frame at each joint centre giving its rotation axes.

**Joints and their freedom** (PDF p.4):
- Finger PIP and DIP and the thumb IP: one degree of freedom each.
- Finger MCP joints: flexion plus abduction, plus a small coupled roll that depends on flexion. They are therefore modelled with three degrees of freedom; the thumb CMC is also given three.
- Carpometacarpal joints of the index and middle fingers are fixed. Ring and little get two small degrees of freedom each.
- Joints between the individual wrist bones are ignored.
- Every degree of freedom has an upper and lower limit taken from the literature.

**How the surface moves** (PDF p.5-7):
- **Pseudo muscles** turn a contraction value into joint rotations, using anatomical data and moments.
- **Geometric muscles**, painted as grids onto the skin, bulge as they contract and push the skin through the springs. This is how muscle bulges appear (Fig. 5).
- The surface under the skin is the mass-spring network: about 1,500 nodes and 4,500 springs, integrated with a Verlet scheme. It ran at 5-10 frames per second on a 2003 PC (PDF p.10).
- The skin mesh keeps its connectivity, so it never tears between poses (PDF p.10).

**From a photograph to a person's hand** (PDF p.8-9):
1. **Feature points.** 26 points that need no anatomical training to find (count by type, PDF p.8):
   - 5 fingertips;
   - 4 finger DIP joints and the thumb IP;
   - 4 PIP joints;
   - 5 MCP joints;
   - 4 interdigital skin points;
   - 2 wrist sides;
   - the head of the ulna.
   A subset of **16** gave "reasonable results" (PDF p.8).
2. **Scale.** The user clicks two marks on a ruler in the photograph to set the scale (PDF p.9).
3. **Make the 2D targets 3D.** Rotate them into the plane fitted to the 3D source points, aligning the two sets' largest diameters. Each target then takes the **height above that plane of its source point**, scaled by the ratio of the two sets' diameters. This keeps the hand from being flattened and gives it a thickness proportional to its overall size (PDF p.9).
4. **Warp everything with one radial-basis function.** The RBF interpolates source to target points, using a biharmonic kernel chosen because it minimises bending energy (the authors cite Bookstein's thin-plate splines). The same function moves:
   - every skin and bone vertex;
   - the joint centres;
   - the muscle-grid control points.
   Muscles are then re-fitted between the new skin and bone, so the new hand can be animated at once (PDF p.9).
5. **Tested on new hands.** The female reference was warped to a man's and to a child's hand. Picking the points took about a minute (PDF p.10).

**Limits and failure cases, as stated:**
1. **Thickness is not measured.** It comes from the reference, scaled by overall size (PDF p.9).
2. **Animation settings do not carry over.** After warping, the muscle contraction values must be re-tuned, because new proportions change torques, inertias and so the resulting angles (PDF p.10).
3. **Choosing contraction values for a wanted pose "is not always a straightforward process."** An optimiser was still being written (PDF p.9-10).
4. **The muscle model is simplified:**
   - geometric muscles do not move the bones (that would need tendons);
   - gravity is left out;
   - there is no collision detection between parts of the hand (PDF p.10).
5. **No texture from the photograph yet.** The authors say parameterising the skin for texture is not trivial (PDF p.10).

## 2. Rhee, Neumann, Lewis (2006): joints and creases from surface anatomy (S2)

**Goal** (S2 PDF p.1-3). A person-specific hand from **one palm photograph**, with no manual input:
1. Find the creases.
2. Infer the joints from them.
3. Warp a generic hand with RBFs.

**Surface anatomy used** (PDF p.3, Fig. 3):
- **Palm creases** (three main ones):
  - the **proximal palmar** crease starts at the hand's edge near the index metacarpal head and runs across the hollow of the palm;
  - the **distal palmar** crease starts at the little-finger metacarpal head and runs through the hollow, passing the ring and middle metacarpal heads;
  - the **thenar** crease lies between the proximal palmar crease and the distal wrist crease.
- **Finger creases.** Each finger has three transverse creases (DIP, PIP and palmar digital). The thumb has two.
- **The distal palm area** is bounded by a line from the radial end of the proximal palmar crease to the ulnar end of the distal palmar crease, and by the palmar digital creases. **The finger MCP joints lie inside this area.**

**Why creases mark joints** (PDF p.3):
- Palm creases are skin flexion folds tied to the fascia beneath.
- Most lie **near the centre of curvature of the head of the bone proximal to the joint**. Mechanical joint centres are defined in the anatomy literature as that same centre of curvature.
- The authors cite radiographic work (Bugbee and Botte 1993) for the crease-to-bone relation. **That paper was not read here.**

**Joint rules** (PDF p.5-6, checked on the page images):
- **DIP and PIP of the fingers, IP and MCP of the thumb.** Where the crease line crosses the finger's medial axis. If two creases are found near one joint, the joint is put at the **lower** one.
- **Finger MCP.** Take the segment Si on each finger's axis between its palmar digital crease and a line H. H joins the start of the distal palmar crease to the start of the proximal palmar crease. The MCP joint goes at **about one third of Si, measured from H**.
- **Wrist.** Midway between the two points of highest contour curvature on the two sides of the wrist (Wa, Wb).
- **Thumb CMC.** Hard to place from a 2D image. It is put where the thumb metacarpal axis (parallel to a line fitted to the contour) meets the radius axis.
- The generic hand has **21 joints** with: one degree of freedom at the DIP, PIP and thumb IP and MCP; two at the finger MCPs; three at the thumb CMC; three at the wrist (PDF p.5).

**Skin** (PDF p.6-7):
- The hand outline is split into **30 curve segments** whose ends sit at creases, tips and valleys.
- These are matched by arc length to the photograph's outline.
- The matches, plus the 21 joints, drive an RBF warp of the generic mesh. The test mesh had 16,423 vertices (PDF p.7).

**Limits and failure cases, as stated** (PDF p.7-8):
1. **Thickness is guessed.** One palm view cannot measure thickness; depth is set proportional to hand length, which "may result in an erroneous depth of the joints".
2. **The thumb CMC cannot be guaranteed** from a 2D image.
3. **Atypical palms fail.** Hands with missing or vague main creases, corns, calluses or large scars cannot be handled.
4. **Lighting and thresholds matter.** Crease extraction needs several thresholds; the defaults worked only with good lighting and no strong shadows.
5. **Small errors can build up** along the contour matching, though the results were acceptable in the tests (PDF p.6).
6. **Out of scope:** skin deformation and texture blending (palm and back textures were blended by hand).

## 3. What the two models give the lane, and what neither gives

| Brief asks | What S1 and S2 give | Gap |
|---|---|---|
| Palm surface and finger pads built from bones and measured dimensions | **Neither builds the surface from the bones.** Both warp one hand-made or scanned reference hand to new joint and feature positions with a smooth RBF (biharmonic, bending-minimising). Thickness comes from the reference, scaled by overall size (S1 p.9; S2 p.8) | No source here gives pad thickness or palm depth as numbers. A9 lengths and breadths can drive the warp's feature points; depth stays a reference shape **(mine)** |
| The knuckle's shape when bent | S1: muscle bulge through mass-spring skin, and the skin stays intact. S2: joint centres at the centre of curvature of the proximal bone's head, beneath the crease | **No stated rule for dorsal knuckle shape at flexion.** The implicit-skinning contact and bulge methods in R9-A3 are the closest |
| Crease positions as shares of the segment | **Rules, not shares.** DIP and PIP lie under their creases; finger MCPs lie at about 1/3 of the way from line H toward the palmar digital crease (S2 p.6) | No open source in this round gives creases as percentages of phalanx length. Bugbee and Botte 1993 (radiographs) is the likely source; it is closed and was not read |
| Nails | Neither model includes nails | Section 4 |

## 4. Nail-plate numbers (S3, S4)

**How curvature was measured** (S3 PDF p.3-4; S4 Methods):
- The transverse curvature of the nail plate is given as the **radius of the circle that best matches the plate's curve**, using a set of radius gauges.
- The gauge is placed across the **highest part** of the plate. In S3 that is midway along the nail, leaving out the free edge.
- **A flatter nail has a larger radius.** The plate is not a true circular arc, so the method is an estimate (S3 PDF p.4).
- One examiner made every measurement in both studies (S3 PDF p.4; S4 Methods). S3's gauges ran to about 17.5 mm; S4's to 15.0 mm.

**S3, England.**
- **Sample** (S3 PDF p.3-4, Table I p.10): 92 men and 90 women aged 21-90. Mean hand length 19.3 cm (men) and 17.5 cm (women), right hand. Breadth 8.5 and 7.6 cm.
- **Transverse radius, dextral participants, right / left hand, mm, mean ± SD** (S3 Table II, PDF p.11, read from the page image):

| Digit | Men R | Men L | Women R | Women L |
|---|---|---|---|---|
| Thumb | 13.2 ± 2.0 | 12.8 ± 2.1 | 10.4 ± 1.8 | 9.8 ± 1.8 |
| Index | 11.2 ± 2.1 | 10.4 ± 2.4 | 8.4 ± 1.9 | 7.8 ± 1.6 |
| Middle | 10.7 ± 2.2 | 10.1 ± 2.0 | 8.0 ± 1.7 | 7.7 ± 1.4 |
| Ring | 8.7 ± 2.1 | 8.1 ± 1.5 | 6.5 ± 1.5 | 6.3 ± 1.2 |
| Little | 6.8 ± 1.2 | 6.6 ± 1.2 | 5.3 ± 0.9 | 5.1 ± 1.0 |

Left-handed participants (10 men, 8 women) are also in Table II but are not repeated here.

- **By age, hands pooled** (S3 Table IV, PDF p.13). Men aged 21-39 / 40-59 / 60+:
  - thumb 12.2 / 13.3 / 13.3;
  - index 9.9 / 10.7 / 11.6;
  - middle 9.5 / 10.5 / 11.2;
  - ring 7.5 / 8.4 / 9.3;
  - little 6.1 / 6.7 / 7.3.

  Women in the same age groups:
  - thumb 9.7 / 10.1 / 11.0;
  - index 7.3 / 8.1 / 9.3;
  - middle 7.3 / 7.9 / 8.8;
  - ring 6.0 / 6.3 / 7.2;
  - little 4.9 / 5.2 / 5.6.

  Hand lengths and breadths were similar across the age groups (PDF p.6, p.13).
- **Findings** (S3 PDF p.5-7):
  - Order from flattest to most curved: **thumb > index = middle > ring > little**. Index and middle did not differ. This order follows the breadth of the distal finger joint (Table III, from another study).
  - **Sex** explains 26-33% of the variance. Men's nails are flatter.
  - After allowing for sex, **age, hand breadth and dominance** explain a further 10-18%. Nails are flatter on the dominant hand, in older people and in wider hands.
  - **Hand length has no effect** (p > 0.2).
  - The dominance effect was significant in right-handers only (paired t-test p < 0.01), not in left-handers.

**S4, Korea.**
- **Sample** (S4 Methods): 300 adults, 30 of each sex in each decade from the 20s to the 60s.
- **What was measured:**
  - nail **width** (widest distance between the lowest points in the lateral nail grooves);
  - nail **length** (fingertip to the groove at the proximal nail fold and eponychium; defined so that trimming does not matter);
  - DIP joint width, hand length and hand breadth;
  - transverse radius.
- **Transverse radius, mm, thumb to little** (S4 Discussion):
  - men 11.4 ± 1.3, 8.9 ± 1.2, 8.8 ± 1.2, 7.3 ± 1.0, 5.8 ± 0.9;
  - women 9.6 ± 0.8, 7.4 ± 0.7, 7.3 ± 1.0, 6.8 ± 0.8, 5.2 ± 0.7.
- **Nail width/length ratio, thumb to little** (S4 Discussion):
  - men 1.04, 0.94, 0.95, 0.95, 0.93 (SD 0.06-0.08);
  - women 0.91 on every finger (SD 0.06-0.08).
  - The ratio was **not** affected by age or handedness.
- **Findings** (S4 Results, Discussion):
  - Same digit order as S3, with index and middle similar.
  - Men's nails are flatter and men's nail widths, lengths and ratios are larger.
  - Height did **not** correlate with the ratio or with curvature.
  - The breadth/length ratio of the hand correlated weakly with radius (all fingers in women; left fingers only in men).
  - Radius grew with age in both sexes (p < 0.001), and dominant-hand nails were flatter. The dominance difference was significant on every finger in left-handed men, right-handed men and left-handed women; in right-handed women it was significant on all fingers except the little finger (S4 Results).
- **Measurement:** radius gauges from 1.0 to 15.0 mm, placed on the uppermost part of the plate; one author measured everything (S4 Methods).
- **Absolute nail widths and lengths in mm are in S4's tables, which are images in the open XML. They were not extracted here (missing).**

**Disagreements between S3 and S4, both given:**
1. **Size of the radii.** S4's radii are smaller on every finger. S4 links this to its smaller hand breadth and DIP joint width (S4 Discussion).
2. **Left-handers.** S3 found the dominance effect significant in right-handers but **not** in left-handers (S3 PDF p.6). S4 found it significant in left-handed men and women too (S4 Results). S3's left-handed group was 18 people.
3. **Wording.** S4's abstract says the thumb has the "highest value of transverse fingernail curvature". Its results make clear this means the largest *radius*, i.e. the flattest nail. Read the radius, not the word "curvature".

**Longitudinal curvature.** S3 says several methods exist for longitudinal overcurvature (citing clinical papers) but gives **no normal values** (PDF p.3). **No open normal values for longitudinal curvature by finger and sex were found in this round (missing).**

**Shape numbers for a nail generator, derived (mine):**
- The plate's cross-section can be an arc of radius R across a width W. Its rise (sagitta) is h = R − √(R² − (W/2)²).
- **Illustrative only, with an assumed width:** for a man's index (R ≈ 11 mm, S3) and a width of 12 mm (assumed, not sourced), h ≈ 1.7 mm. **(mine)**
- Bounds for a generator: radius by digit and sex from the two tables; nail width ≈ length (S4 ratios about 0.9-1.04); age and dominance shift the radius upward by roughly 1-2 mm across adult life (S3 Table IV). **(mine)**
