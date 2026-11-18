# J29: Bullet drag (G1 and G7), ballistic coefficients, and penetration tests

Fixed after the second check (2026-11-18): the claim that Hornady's BCs are measured by Doppler radar is removed (the page does not say it); the smallest BC drop across Hornady's page is 0.68% (30 cal 250 gr A-Tip Match), not 1.6%; the Roma Plastilina backing is NIJ section 4.2.5.3, not section 7; the FM 3-06.11 Table 7-6 cell for .50 ball against 10 in reinforced concrete is pulled from the table and moved under Unconfirmed. Spreads added from the checker: the J40 maker BC, the NIJ conditioned velocities, Hornady's two Mach 2.0 speeds and its hidden 338 cal row; and a note that the gelatin recipe sentence carries no reference.

Researched 2026-11-17 to 2026-11-18 (all URLs accessed in that window). Round-four gap 2. Game physics only. Extends round-three J25 (cartridge loads, recoil, vacuum drop). Values marked "computed by me" are arithmetic on the cited numbers.

## J29-1. How the drag functions are used (one line)

Retardation of a real bullet = (standard projectile's drag at the same Mach) ÷ BC, where BC = sectional density ÷ form factor; a G7 BC must be used with the G7 table and a G1 BC with the G1 table (Wikipedia "Ballistic coefficient", text CC BY-SA 4.0, https://en.wikipedia.org/wiki/Ballistic_coefficient; the G1 standard is a flat-base 2-calibre-ogive projectile, the G7 a long 7.5° boat-tail with a 10-calibre secant ogive, per the same article's G-model list and its figures).

## J29-2. G1 and G7 drag coefficient against Mach (selection)

Source: JBM Ballistics, "More Drag Functions", files mcg1.txt (79 rows, Mach 0 to 5) and mcg7.txt (84 rows, Mach 0 to 5), https://jbmballistics.com/downloads/mcg1.txt and https://jbmballistics.com/downloads/mcg7.txt; the JBM downloads page (https://jbmballistics.com/downloads.html) says the G functions were obtained from the US Army Ballistic Research Laboratory (BRL). **Secondary** (BRL original not opened). The downloads page states the files are "free for public use". Selected rows retyped; rows marked * fall between table points and were interpolated linearly by me.

| Mach | G1 Cd | G7 Cd | G1 ÷ G7 (computed by me) |
|---|---|---|---|
| 0.00 | 0.2629 | 0.1198 | 2.19 |
| 0.50 | 0.2032 | 0.1194 | 1.70 |
| 0.70 | 0.2165 | 0.1202 | 1.80 |
| 0.80 | 0.2546 | 0.1242 | 2.05 |
| 0.85 | 0.2901 | 0.1306 | 2.22 |
| 0.90 | 0.3415 | 0.1464 | 2.33 |
| 0.95 | 0.4084 | 0.2054 | 1.99 |
| 1.00 | 0.4805 | 0.3803 | 1.26 |
| 1.05 | 0.5427 | 0.4043 | 1.34 |
| 1.10 | 0.5883 | 0.4014 | 1.47 |
| 1.20 | 0.6393 | 0.3884 | 1.65 |
| 1.30 | 0.6589 | 0.3732 | 1.77 |
| 1.50 | 0.6573 | 0.3440 | 1.91 |
| 1.75 | 0.6280 | 0.3160 | 1.99 |
| 2.00 | 0.5934 | 0.2980 | 1.99 |
| 2.50 | 0.5397 | 0.2697 | 2.00 |
| 3.00 | 0.5133 | 0.2424 | 2.12 |
| 4.00 | 0.5006 | 0.1935 | 2.59 |
| 5.00 | 0.4988 | 0.1618 | 3.08 |

Peaks (read from the files): G1 maximum 0.6625 at Mach 1.40; G7 maximum 0.4043 at Mach 1.05. The transonic rise starts near Mach 0.8 in both. Full files are linked, not copied.

## J29-3. Ballistic coefficients of modern rifle bullets (maker)

Source: Hornady Manufacturing, "Ballistic Coefficient" page, https://www.hornady.com/bc (read 2026-11-17). Hornady states the BCs are corrected back to ICAO standard atmosphere (sea level, 59 °F, 29.92 inHg, 0% humidity), and gives each at Mach 2.25 (2,512 ft/s), 2.0 and 1.75. Units lb/in². Selection of rows. Correction (second check, 2026-11-18): the page does not say how the BCs were measured; the word "Doppler" is not on it. The same page lists the Mach 2.0 speed as both 2,233 and 2,232 fps. Its markup also holds a second, hidden row for the 338 cal 300 gr A-Tip with different Mach 2.0 and 1.75 values (0.859/0.818 against 0.860/0.850); that row is not used here.

| Bullet | G1 at M2.25 / M2.0 / M1.75 | G7 at M2.25 / M2.0 / M1.75 |
|---|---|---|
| 22 cal 73 gr ELD Match | 0.398 / 0.389 / 0.382 | 0.200 / 0.195 / 0.192 |
| 22 cal 88 gr ELD Match | 0.545 / 0.534 / 0.528 | 0.274 / 0.268 / 0.266 |
| 6.5 mm 140 gr ELD Match | 0.646 / 0.637 / 0.616 | 0.326 / 0.320 / 0.310 |
| 30 cal 168 gr ELD Match | 0.523 / 0.516 / 0.498 | 0.263 / 0.259 / 0.251 |
| 30 cal 208 gr ELD Match | 0.690 / 0.683 / 0.669 | 0.348 / 0.343 / 0.337 |
| 30 cal 178 gr ELD-X | 0.552 / 0.543 / 0.538 | 0.278 / 0.273 / 0.271 |
| 338 cal 285 gr ELD Match | 0.829 / 0.814 / 0.796 | 0.417 / 0.409 / 0.400 |
| 375 cal 390 gr A-Tip Match | 0.987 / 0.974 / 0.971 | 0.497 / 0.489 / 0.488 |

Patterns (computed by me): in the rows above G1 BC ÷ G7 BC is 1.98 to 1.99; across the page BC falls between 0.68% (30 cal 250 gr A-Tip Match, G1 0.878 to 0.872) and 8.6% (6.5 mm 147 gr ELD Match) from Mach 2.25 to Mach 1.75 (smallest drop corrected by the second check, 2026-11-18, recomputed over every bullet on the page; the 375 cal 390 gr drops 1.62% and the 7 mm 175 gr ELD-X 1.60%).

Doppler example of BC varying along the path (Wikipedia "Ballistic coefficient", CC BY-SA 4.0, table for a Lost River J40 .50 calibre 773 gr solid, source not named in the table): G1 BC 1.040 at 500 m, rising to 1.068 at 1,100 to 1,300 m, falling to 1.032 at 2,000 m. The same paragraph gives the maker's assigned BC of 1.062 lb/in² (746.7 kg/m²), a spread against the 1.032 to 1.068 measured (second check, 2026-11-18).

## J29-4. Body armour test threats (a fixed "this stops that" ladder)

Source: National Institute of Justice, *Ballistic Resistance of Body Armor*, NIJ Standard-0101.06, July 2008, https://www.ojp.gov/pdffiles1/nij/223054.pdf, section 2 and Table 4 (US Government work). Velocities are for new armour; tolerance ±9.1 m/s on all.

| Type | Test bullets (mass, velocity) | Max backface deformation in clay |
|---|---|---|
| IIA | 9 mm FMJ RN 8.0 g at 373 m/s; .40 S&W FMJ 11.7 g at 352 m/s | 44 mm |
| II | 9 mm FMJ RN 8.0 g at 398 m/s; .357 Magnum JSP 10.2 g at 436 m/s | 44 mm |
| IIIA | .357 SIG FMJ FN 8.1 g at 448 m/s; .44 Magnum SJHP 15.6 g at 436 m/s | 44 mm |
| III (rifle) | 7.62 mm M80 FMJ 9.6 g at 847 m/s | 44 mm (Table 4) |
| IV (AP rifle) | .30 cal M2 AP 10.8 g at 878 m/s | 44 mm (Table 4) |

Conditioned (aged) soft armour is tested 18 to 28 m/s slower (e.g. IIA 9 mm at 355 m/s). All conditioned velocities (second check, 2026-11-18): IIA 355 and 325, II 379 and 408, IIIA 430 and 408 m/s. Backing is Roma Plastilina No. 1 oil-based clay (section 4.2.5.3, "Backing Material", under 4.2.5 Armor Backing Material; corrected by the second check from "section 7", which is the test methods).

## J29-5. Penetration into building materials (US Army field manual)

Source: US Army, FM 3-06.11 *Combined Arms Operations in Urban Terrain* (2002), Chapter 7, Tables 7-2 to 7-6, read as HTML at https://www.globalsecurity.org/military/library/policy/army/fm/3-06-11/ch7.htm (US Government work; **secondary host**, the official PDF not opened). Centimetres computed by me.

**Single 7.62 mm ball round (Table 7-3), penetration in inches (cm):**

| Range (m) | Pine board | Dry loose sand | Cinder block | Concrete |
|---|---|---|---|---|
| 25 | 13 (33) | 5 (13) | 8 (20) | 2 (5) |
| 100 | 18 (46) | 4.5 (11) | 10 (25) | 2 (5) |
| 200 | 41 (104) | 7 (18) | 8 (20) | 2 (5) |

**Rounds needed to get through walls (selection):**

| Target | 5.56 mm, 25-100 m (Table 7-2) | 7.62 mm NATO ball, 25 m (Table 7-5) | .50 ball, 35 m (Table 7-6) |
|---|---|---|---|
| 8 in reinforced concrete | 35 to first penetration, 250 for a loophole | 100 for a 7 in hole | pulled, see Unconfirmed |
| 9 in double brick | 70 to first penetration, 120 for a loophole | 45 (penetration only) | |
| 16 in log wall | 1 to 3 (penetration only) | 1 | 1 |
| 24 in double sandbag wall | 220 (penetration only) | 110 | 5 |
| 3/8 in mild steel door | 1 | 1 | |
| 1 in armour plate | | | 1 |

Text facts (same chapter): 7.62 mm ball at 50 m cannot reliably penetrate one layer of well-packed sandbags, can at 200 m; .50 ball and AP penetrate 14 in of sand or 28 in of packed earth at 200 m; a layer of books 18 to 24 in thick stops 5.56 mm rounds.

## J29-6. Gelatin: the test medium and the depth criterion

- Medium (Wikipedia "Ballistic gelatin", CC BY-SA 4.0, citing the INS National Firearms Unit protocol, Firearms Tactical Institute 1998): 10% gelatin (250 bloom type A, 1 part to 9 parts water by mass) chilled to 4 °C; calibrated by a .177 in steel BB at 183 ± 3 m/s which must penetrate 8.3 to 9.5 cm. The 4 °C figure is marked "citation needed" in the article. Older NATO formula: 20% at 10 °C. Second check (2026-11-18): the 10% recipe sentence carries no reference in the article; the INS citation supports only the BB calibration.
- Depth criterion: "While penetration up to 18 inches is preferable, a handgun bullet MUST reliably penetrate 12 inches of soft body tissue at a minimum"; a side shot through an arm needs at least 10-12 inches to reach the heart; about 7 inches from the front to reach the major vessels of the abdomen in a slender adult. Patrick U.W., *Handgun Wounding Factors and Effectiveness*, FBI Academy Firearms Training Unit, 14 July 1989 (US Government work). **Secondary:** read from an HTML transcription, https://gunnerynetwork.com/reference_material/hwfe.html; the FBI original was not opened. In centimetres (computed by me): 30 to 46 cm.

## Sources disagree

- No direct disagreement between the sources opened. Note for modelling: FM 3-06.11 Table 7-3 shows 7.62 mm ball going deeper into pine at 200 m (41 in) than at 25 m (13 in), and the chapter says penetration is best at 600 m, so a simple energy model will be wrong at short range.
- Wikipedia's gelatin temperature (4 °C) is uncited in the article.

## Not found

- BRL original G1/G7 reports (only the JBM retyping was opened).
- Military ball BCs (M80, M855) and pistol bullet BCs from a maker page opened this round.
- Spin drift and wind drift tables from a primary source (compute from J29-2 instead).
- Penetration of steel plate by thickness and velocity for small arms beyond the FM's single rows; shotgun pattern spread; fragment speeds.
- Gelatin penetration results for named cartridges from a published test opened this round.

## Unconfirmed (second check, 2026-11-18)

- FM 3-06.11 Table 7-6, .50 ball at 35 m against reinforced concrete. Delivered as "10 in thick: 50 for a 7 in hole". The checker found the host's HTML table stacks the cells (thickness 10 and 18 in; hole 12, 24 and 7 in; rounds 50, 100 and 140). Read in that order, 10 in gives a 12 in hole for 50 rounds (and 24 in for 100), and 18 in gives a 7 in hole for 140 rounds; pairing 50 rounds with a 7 in hole is not supported. The official FM PDF has not been opened. **Not to be used until a source that gives it (the official field manual PDF) has been opened and the cells re-read in order.**

## Sources

All accessed 2026-11-17 to 2026-11-18 (as the header says).

- Wikipedia, "Ballistic coefficient" and "Ballistic gelatin", text CC BY-SA 4.0, https://en.wikipedia.org/wiki/Ballistic_coefficient, https://en.wikipedia.org/wiki/Ballistic_gelatin
- JBM Ballistics, drag function files and downloads page, https://jbmballistics.com/downloads/mcg1.txt, https://jbmballistics.com/downloads/mcg7.txt, https://jbmballistics.com/downloads.html
- Hornady, "Ballistic Coefficient", https://www.hornady.com/bc
- NIJ Standard-0101.06 (2008), https://www.ojp.gov/pdffiles1/nij/223054.pdf
- US Army FM 3-06.11 (2002), Chapter 7, via https://www.globalsecurity.org/military/library/policy/army/fm/3-06-11/ch7.htm
- Patrick (1989), FBI, via https://gunnerynetwork.com/reference_material/hwfe.html
