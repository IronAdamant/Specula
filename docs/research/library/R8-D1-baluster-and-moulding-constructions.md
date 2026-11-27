# R8-D1. Baluster proportions and moulding profiles built from arcs

Round 8, section D, item 1. Prepared 2026-11-27. Private notes in my own words; both sources are public domain, so nothing here raises a copying question, but I still paraphrase. **(mine)** marks my own reading of a plate or my own arithmetic.

## Sources

| Tag | Role | Citation | Free copy | Licence |
|---|---|---|---|---|
| L1 | Listed | James Gibbs, *Rules for Drawing the Several Parts of Architecture, in a More Exact and Easy Manner than has been heretofore practised, by which all Fractions, in dividing the principal Members and their Parts, are avoided*. London: printed by W. Bowyer for the author, 1732. 42 pp. text + 64 plates. | Internet Archive scan of the 1732 first edition: https://archive.org/details/bim_eighteenth-century_rules-for-drawing-the-se_gibbs-james-architect_1732 (177 PDF pages). Raw: raw/round8/gibbs1732.pdf, gibbs1732_djvu.txt, gibbs1732.flow.txt; plate crops in raw/round8/img/gibbs/. Other free scans: 1738 (archive.org id rules-for-drawing-the-several-parts-of-architecture, marked Public Domain Mark 1.0) and 1753 (Getty, gri_33125011578867). | Public domain (1732; author died 1754). The IA item states no licence. |
| S1 | Substitute and second opinion for "any open measured account of the classical baluster's parts", and for moulding names | William R. Ware, *The American Vignola, Part I: The Five Orders*. Scranton, PA: International Textbook Co.; copyright 1902; this printing 1920. | https://archive.org/details/americanvignola01wareuoft (University of Toronto scan, 94 PDF pages). Raw: ware-vignola1.pdf, ware-vignola1.djvu.txt, ware-vignola1.flow.txt | Public domain (US, published 1902; author died 1915). IA marks it NOT_IN_COPYRIGHT. |

**Page mapping.**
- **L1:** printed text page n = PDF n + 7 (PDF 16 is printed "(9)", PDF 27 is "(20)", PDF 48 is "(41)"). Plate N is at PDF 50 + 2(N − 1); plates have blank backs. Checked for Plates XXIV (PDF 96), XXV (PDF 98) and LXII (PDF 172).
- **S1:** printed n = PDF n + 10. "Moldings" is p. 3 = PDF 13; Balustrades is pp. 38-39 = PDF 48-49.

**OCR warning.** The IA OCR of L1 drops the small engraved fractions (¼, ⅓, ⅙). Every fraction below was read from the page image, not the OCR.

## D1-1. Module convention (L1 p. 2 = PDF 9)

L1 notes that most authors take the column's diameter or semidiameter as the module and split it into minutes (12, 18, 24, 30 or 60 parts; Palladio uses 60 minutes for the diameter). Gibbs avoids minutes altogether. Each feature's height is split into a few equal parts, and those parts are split again, so no fractions arise. **For Specula this means proportions should be stored as small-integer divisions of each feature's own height, not as module minutes (mine).**

## D1-2. Baluster (L1 p. 41 = PDF 48, Plate LXII = PDF 172; S1 pp. 38-39 = PDF 48-49)

### L1, Gibbs: five balusters on one plate

**Text (PDF 48):**
- The lowest (stoutest) baluster is 8 parts high. Its breadth is divided into 4: 2 go to the solid core and 2 to the swelling members.
- Balusters stand half their own thickness apart.
- Dotted cross lines on the plate mark the centres of the swelling and hollow curves.
- The balustrade above it on the left is 9 parts high and the one on the right is 10, each subdivided as drawn.
- The two uppermost are slenderer, 12 parts, with their mouldings subdivided. Gibbs says these suit wood or cast metal; the others suit stone.
- The caps and bases of balustrades take the same proportions as ordinary pedestals.

**Read off Plate LXII, bottom row (mine; the plate's own scale; image crops pl62_bottom_profile.png and pl62_bottom_elev.png):**

| Part | Height band (of 8) | Width |
|---|---|---|
| Plinth (square block) | 0 to 1 | 2 parts, the full baluster breadth |
| Base mouldings: torus, scotia/cove with fillets, astragal ring | about 1 to 2.4 | (not measured) |
| Belly (greatest swell) | widest at about 3.2 | the full breadth, 2 parts |
| Belly-to-neck curve | about 2.4 up to 5 | (shape below) |
| Neck (narrowest) | about 5 to 6 | the "solid", 1 part = ½ the breadth |
| Collar: astragal and fillet rings | about 6 to 6.7 | (not measured) |
| Small ovolo under the abacus | about 6.7 to 7 | (not measured) |
| Abacus (square block) | 7 to 8 | 2 parts |

- **Overall:** horizontal and vertical plate scales have the same unit (about 37.5 px per part at my crop scale). So this baluster is 8 high by 2 wide, **height = 4 x breadth**. Gaps are 1 part (half the thickness), so balusters stand **3 parts = ⅜ of the height apart, centre to centre**.
- **Belly-to-neck curve:** drawn as two arcs. One centre is level with division 5 and well outside the baluster; its radius reaches down to the top of the base mouldings. The other centre is near the axis at about height 3.2.

### S1, Ware: a different rule

**Proportions (p. 39 = PDF 49):**
- **Cap:** one quarter of the baluster's height, including the astragal. It has three roughly equal members, as in a Tuscan or Doric capital: a plain abacus; an echinus and fillet; and a necking.
- **Base:** also one quarter of the height. It resembles the Attic base, with the scotia usually the main member.
- **Shaft ("sleeve"):** between cap and base, profiled as a *quirked cyma reversa*.
  - The belly (greatest diameter) sits at about one-fourth of the shaft's height, i.e. about one-third of the baluster's height.
  - The belly width is one-third of the baluster height. So is the plinth width (exactly) and the abacus width (almost).
  - The neck is less than half as wide.
  - The inflection point of the cyma reversa is half way between cap and base.
- **Lighter variants** are used with the Ionic and Corinthian orders. A beak moulding often replaces the cyma reversa. A "double baluster" is two small balusters set base to base. Square balusters also occur.

**Spacing and rails (pp. 38-39 = PDF 48-49):**
- Balusters are about **half their height apart on centres**.
- The rails are sometimes ⅙ and ²⁄₆ of the space between them, like a continuous pedestal, but are often much heavier, up to ⅓ and ½.
- A balustrade is for leaning on, so 3-4 ft high is normal. It is scaled to the human figure, not to the order, though St Peter's balustrade is about 8 ft.
- No more than about a dozen balusters should run without an uncut block (a solid baluster-sized block placed under a joint in the upper rail).

### Where L1 and S1 disagree (both given, as the brief asks)

| Quantity | Gibbs (L1, stone baluster) | Ware (S1) |
|---|---|---|
| Breadth / height | ¼ (8 high, 2 wide) | ⅓ |
| Centre spacing / height | ⅜ (gap = ½ breadth) | about ½ |
| Neck / belly | ½ | less than ½ |
| Belly height / total | about 0.4 (mine, from plate) | about ⅓ |
| Cap + base share | plinth ⅛ + base mouldings, abacus ⅛ + collar (mine) | ¼ + ¼ |

Gibbs also gives slenderer 9-, 10- and 12-part variants; Ware gives one proportion and says "lighter" for Ionic and Corinthian. **(mine)** Ware's ⅓ and Gibbs's ¼ bracket the stout stone baluster. A shape library could expose breadth/height in that range, with neck/belly ≤ ½.

## D1-3. Moulding profiles

### Names and shapes (S1 p. 3 = PDF 13; table p. 4 = PDF 14)

- **Convex:**
  - Ovolo (quarter round).
  - Torus (half round).
  - Three-quarter round.
  - Bead, astragal or reed (a small torus).
  - Thumb moulding (an elliptical torus).
- **Concave:**
  - Cavetto (quarter hollow).
  - Congé (a cavetto tangent to a plane).
  - Half hollow.
  - Scotia (three-quarter hollow). S1 notes "scotia" is often used loosely for any hollow.
- **Double curvature (cyma):**
  - **Cyma recta:** tangents at top and bottom are horizontal, as if cut from a horizontal wavy line.
  - **Cyma reversa:** tangents at top and bottom are vertical.
  - S1 warns that some writers call a cyma recta turned upside down "reversa", and that this causes confusion.
  - The proportion of concave to convex part varies.
  - Cymatium means a small cyma, or a small crowning moulding of any shape.
  - A beak moulding is a convex and a concave meeting at an angle instead of tangentially.
- **What the curves are:** S1 says the curves may be arcs of circles, ellipses, parabolas, hyperbolas or other curves. The sentence breaks off at the page turn in this scan (PDF 14), so I could not read its ending.

### Constructions from circular arcs (L1)

**Cyma ("ogee"), erect or reversed (L1 p. 19-20 = PDF 26-27, Plate XXIV = PDF 96):**
1. Draw the diagonal of the square formed by the member's height and projection, from fillet ("list") to fillet.
2. Halve that diagonal.
3. From the middle and from each end of the diagonal, strike intersecting arcs. The two intersection points are the centres of the two arcs of the ogee.
4. Gibbs says this rule serves for every curved member of this kind, erect or reversed.

**(mine) Geometry of this construction:**
- Each half of the cyma is a circular arc through one end of the diagonal and its midpoint. Its centre lies on the perpendicular bisector of that half-diagonal. If the striking radius equals the half-diagonal, the centre makes an equilateral triangle with the two points, so each arc turns through 60 degrees.
- The two arcs meet at the midpoint. Their centres are symmetric about the midpoint and lie on opposite sides of the diagonal, so the arcs share a tangent there and the curve is C1-continuous at the inflection.
- Plate XXIV draws the construction arcs crossing on both sides of the diagonal, which is consistent with this reading. Gibbs does not state the striking radius in words; the equal-radius reading is mine.
- L1 also gives pedestal caps a projection-to-height ratio of 2 : 3 (p. 20 = PDF 27).

**Ovolo (Tuscan capital, L1 p. 9 = PDF 16, Plate VI):**
- The capital is ½ diameter high and projects ¼ of the diameter at the top of the shaft. Its height is split into three equal parts: collarino, ovolo, abacus. The projection is split into six.
- The ovolo is made *less than a quarter circle*.
- **Finding the centre:** strike arcs from the ovolo's two end points so that they intersect; join the intersections (the dotted line on the plate, i.e. the perpendicular bisector of the chord). The centre is where that line meets the ovolo's upper line.

**Astragal and the cavetto joining fillet to shaft (L1 p. 9 = PDF 16):**
- The astragal under the capital belongs to the column and measures ⅙ of the semidiameter. Of its height, ⅓ goes to the fillet ("listel"). It projects as much as it is high.
- To join the fillet to the shaft: divide the fillet's projection into 5 parts and use **6 such parts as the radius** of the cavetto. **(mine)** So the cavetto is flatter than a quarter circle: a radius of 1.2 x the projection, tangent to the shaft.

**Torus (L1 p. 20 = PDF 27, Plate XXV = PDF 98):** column bases are ½ diameter high and project ⅓ of their height. Their members are mostly semicircular, so a torus is a half circle on its height.

**Scotia from two centres (L1 p. 20 = PDF 27, Plate XXV, Ionic base):**
1. Lay out the members and the centres of the upper and lower torus.
2. Drop a perpendicular from the centre of the upper torus. Divide the part of it inside the scotia's height into 7.
3. The top 3 parts are the radius of the smaller circle CDE, centred at C.
4. From C draw the outer semicircle AFB.
5. Draw the line from A through the centre of the smaller circle, meeting it at E.
6. With centre A, draw the arc BE. The profile B-E-D is the scotia.
7. Gibbs says the rule serves for the bases of every order.

**(mine)** The two arcs share a tangent at E, because A, the small centre and E are collinear. So the scotia is C1 at E, with radii in the ratio set by the 3/7 split.

## Known problems

- **L1 OCR** drops fractions, as noted above.
- **L1's construction text is terse.** "Mark the intersections from the middle and extremes of the diagonal" leaves the striking radius unstated; my equal-radius reading rests on the plate.
- **L1's plate lettering for the scotia** (A, B, C, D, E, F) is small at 1400 px. I matched the text to the Ionic base drawing but did not resolve each letter. The construction is checked as stated in the text only.
- **S1 and L1 disagree on baluster proportions** (table above). Neither cites measured historical examples; both are pattern-book rules.
- **S1's moulding-curve sentence** is cut off in this scan.

## Not found

- An open *measured survey* of real classical balusters (archaeological measurements), as opposed to pattern-book rules: not found in this pass.
