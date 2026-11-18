# J20: Restitution extended: rock and ground, speed dependence, small spheres

Fixed after the second check (2026-11-18):
- J20-2: the e = 1.01 result is Beger (2012), 10 mm soda-lime glass beads, not King et al. (2011) steel spheres; King et al. is reported only as "non-monotonic". The e = 0.8 ± 0.06 study is named (Durda et al. 2011). Sources disagree corrected to match.
- J20-3: Rocscience Rt runs 0.20-0.99, not 0.5-0.99.
- J20-4: the golf speed values are cited to the USGA "Long Hitters" article, the 0.83 limit to the USGA conforming-club page; racquetresearch.com backs a tennis-racquet value, not golf.
- Spreads from Rocscience's user-feedback rows added; a duplicate row in the Rocscience table and the Chau, Wong and Lee year noted under Sources disagree.
- Licences stated; a Sources list added at the bottom.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic. Extends round-two J2 (sports balls); J2's "Not found" asked for everyday material pairs, and this file adds ground and rock surfaces plus how e changes with impact speed.

Reminder: e is the ratio of separation to approach speed along the normal; for a drop on a rigid floor, rebound height / drop height = e² (round-two J2).

## J20-1. Rock and debris hitting the ground (read directly, retyped selection)

Rocscience, *RocFall2 Documentation*, "Coefficient of Restitution Table", https://www.rocscience.com/help/rocfall/documentation/slope/materials/reference-tables/rocscience-coefficient-of-restitution-table-in-rocfall. Rn = normal restitution, Rt = tangential (ratio of outgoing to incoming tangential speed). The documentation notes rigid-body Rn values are significantly lower than lumped-mass values and the two are not interchangeable (Rocscience, "Coefficient of Restitution", https://www.rocscience.com/help/rocfall/documentation/slope/materials/coefficient-of-restitution). Each row's original study is named.

| Surface | Rn | Rt | Original study (as listed by Rocscience) |
|---|---|---|---|
| Hard surface paving | 0.37-0.42 | 0.87-0.92 | Pfeiffer and Bowen 1989, *Bull. Assoc. Eng. Geologists* 26(1) 135-146 (Glenwood Canyon, Colorado) |
| Bedrock or boulders, little soil | 0.33-0.37 | 0.83-0.87 | same |
| Talus, little vegetation | 0.30-0.33 | 0.83-0.87 | same |
| Soft soil slope, little vegetation | 0.28-0.32 | 0.80-0.83 | same |
| Vegetated soil slope | 0.28-0.32 | 0.78-0.82 | same |
| Clean hard bedrock | 0.53 | 0.99 | Hoek (as listed) |
| Asphalt | 0.40 | 0.90 | Hoek |
| Limestone face (mean ± sd) | 0.315 ± 0.064 | 0.712 ± 0.116 | Robotham, Wang and Walton 1995, *Trans. IMM Section A* 104, A25-A33 |
| Chalk face | 0.276 ± 0.079 | 0.835 ± 0.087 | same |
| Vegetated limestone pile | 0.251 ± 0.029 | 0.489 ± 0.141 | same |
| Wood platform at 45° (control) | 0.384 ± 0.133 | 0.687 ± 0.130 | Wu 1985, *Transportation Research Record* 1031, 1-5 |
| Soil (3-5 cm granite pieces dropped) | 0.393 | 0.567 | Chau, Wong and Lee 1998, *Int. J. Rock Mech. Min. Sci.* 35(4/5) 662-663, doi:10.1016/S0148-9062(98)00023-0 |
| Shotcrete | 0.453 | 0.737 | same |
| Rock slope | 0.487 | 0.910 | same |
| Bedrock | 0.50 | 0.95 | Giani 1992, *Rock Slope Stability Analysis*, Balkema |
| Debris of uniform pieces | 0.30 | 0.70 | same |
| Pyroclastic fill at a cliff base | 0.10 | 0.20 | Budetta and Santo, *Engineering Geology* 36, 197-210 |
| Detritus fans at a cliff foot | 0.00 | 0.24 | same |

Spread from the same Rocscience table page (its user-feedback rows, as the second checker reports them, 2026-11-17): Takamatsu concrete has Rn 0.48 ± 0.19 and 0.85; weathered rock 0.47 ± 0.30 up to 1.00; Norway blockfields 0.50 ± 0.06. These widen concrete and rock a lot. (second check: needs a look: the report does not say which of these figures are Rn and which Rt beyond the first, so read the rows on the page before using them.)

### Table J20-1a. Rebound height of a dropped rock (computed by me, e² × drop, Rn as e)

| Surface | Rn | Rebound from a 10 m drop |
|---|---|---|
| Clean hard bedrock | 0.53 | 2.8 m |
| Rock slope | 0.487 | 2.4 m |
| Asphalt | 0.40 | 1.6 m |
| Soil | 0.393 | 1.5 m |
| Chalk | 0.276 | 0.76 m |
| Pyroclastic fill | 0.10 | 0.1 m |

## J20-2. How impact speed changes e (read directly)

Güttler, Heißelmann, Blum and Krijt, "Normal Collisions of Spheres: A Literature Survey on Available Experiments", arXiv:1204.0001 (2012), https://arxiv.org/abs/1204.0001:

- Three regimes for a sphere hitting a flat target: below the **sticking velocity** v_s every collision sticks (e = 0); just above it e rises steeply as e = √(1 − (v_s/v_i)²); between v_s and the **plastic yield velocity** v_y, e is roughly constant near its maximum; above v_y plastic deformation lowers e as speed rises (section 1, Fig. 1).
- Dahneke (1973), latex microspheres: critical sticking velocities 0.83 m/s on stainless steel and 0.92 m/s on polished quartz (section 3.1).
- Beger (2012), 10 mm soda-lime glass beads (smooth, ±0.1 µm, and etched, ±5 µm) dropped in a 1.5 m vacuum tube at 0.01-1 m/s: smooth beads gave e = 1.01 at 1 m/s with scatter under 0.01, which the survey says can only be a systematic error; e fell with speed to 0.93 at 0.01 m/s; etched (rough) beads had lower e with the same trend (section 3.2, PDF p. 9).
- King et al. (2011), steel, brass, aluminium and Delrin spheres 6.24-18.76 mm dropped on steel or granite: reported in the survey only as "non-monotonic", with no e values quoted (section 3.2).
- Durda et al. (2011), 1 m granite spheres hung on cranes, 0.1-1.5 m/s, reported e = 0.8 ± 0.06, comparable to its measurement accuracy (same section).

Game use: give each material pair a plateau e, a low-speed cut-off where small objects stop bouncing, and a slow fall-off at high speed; never let e reach 1.

## J20-3. Tangential bounce and spin

Rocscience's Rt values (0.20 to 0.99; 0.20 pyroclastic fill, 0.24 detritus fans, 0.489 vegetated limestone pile at the low end) show that rocks keep most of their sliding speed on hard surfaces but lose half or more on soil and vegetation. Use Rt as the multiplier on the along-surface speed at each bounce for rubble and debris.

## J20-4. Rule-book bounce tests and club faces (added for brief section 2, "bounce")

Governing-body drop tests turn directly into a restitution value: COR = sqrt(rebound height / drop height) (computed by me for each row, ignoring air drag). The rules are quoted from Wikipedia "Coefficient of restitution", https://en.wikipedia.org/wiki/Coefficient_of_restitution (secondary, CC BY-SA 4.0).

| Item | Rule (as quoted by Wikipedia "Coefficient of restitution") | COR | Source cited there |
|---|---|---|---|
| Basketball | rebound 1,035-1,085 mm from a 1,800 mm drop | 0.758-0.776 | FIBA official basketball rules documents, https://www.fiba.basketball/documents |
| Table tennis ball | rebound 24-26 cm from 30.5 cm onto a steel block | 0.887-0.923 | International Table Tennis Federation rule as given in the article; the reference attached is the ITF "Tennis Tech" page, so the citation does not match the claim |
| Golf driver face (ball on club) | USGA upper limit 0.83; tested values 0.845 at 90 mph down to 0.797 at 130 mph | 0.797-0.845 | 0.83 limit: USGA conforming-club page (web.archive.org/web/20210616221005/https://www.usga.org/ConformingGolfClub/conforming_golf_club.asp); 0.845 at 90 mph and 0.797 at 130 mph: USGA, "Do Long Hitters Get An Unfair Advantage?" (https://www.usga.org/content/usga/home-page/articles/2011/04/do-long-hitters-get-an-unfair-advantage-2147496940.html). (racquetresearch.com/coeffici.htm, cited beside them, backs a tennis-racquet 0.85 quote, not golf.) |

Speed dependence shows in the golf row: the same pair loses restitution as impact speed rises, matching J20-2.

## Sources disagree

- Bedrock: Rn 0.33-0.37 (Pfeiffer and Bowen), 0.50 (Giani), 0.53 (Hoek). Field Rn depends on block shape, slope angle and whether the model is lumped-mass or rigid-body.
- Smooth glass beads, e = 1.01 (Beger 2012, flagged as systematic error by the survey) against physical limit e ≤ 1.
- Rocscience's table lists "Uncovered limestone blast pile" with exactly the same values as "Limestone face" (0.315/0.064/0.712/0.116), which looks like a duplicate in the source.
- Chau, Wong and Lee: Rocscience lists the study as 1996; Crossref gives *Int. J. Rock Mech. Min. Sci.* 35(4-5):662-663, June 1998, so this file's 1998 is right.

## Not found

- J20-4 table tennis: the ITTF handbook itself was not opened; the Wikipedia citation points to an ITF (lawn tennis) page.

- Baseball and golf restitution limits from the governing-body documents: values were seen only in search summaries (baseball about 0.546 at 85 ft/s on ash; USGA driver limit 0.830), not read from MLB or USGA PDFs, so they are not used here.
- An open primary table for everyday pairs such as wood on wood, steel on concrete, or glass on tile.
- Restitution against temperature (cold rubber).

## Sources

All accessed 2026-11-17.

- Rocscience, *RocFall2 Documentation*, "Coefficient of Restitution Table", https://www.rocscience.com/help/rocfall/documentation/slope/materials/reference-tables/rocscience-coefficient-of-restitution-table-in-rocfall
- Rocscience, *RocFall2 Documentation*, "Coefficient of Restitution", https://www.rocscience.com/help/rocfall/documentation/slope/materials/coefficient-of-restitution
- Original studies as listed by Rocscience: Pfeiffer and Bowen 1989, *Bull. Assoc. Eng. Geologists* 26(1) 135-146; Hoek; Robotham, Wang and Walton 1995, *Trans. IMM Section A* 104, A25-A33; Wu 1985, *Transportation Research Record* 1031, 1-5; Chau, Wong and Lee 1998, *Int. J. Rock Mech. Min. Sci.* 35(4-5) 662-663, doi:10.1016/S0148-9062(98)00023-0; Giani 1992, *Rock Slope Stability Analysis*, Balkema; Budetta and Santo, *Engineering Geology* 36, 197-210
- Güttler, Heißelmann, Blum and Krijt, "Normal Collisions of Spheres: A Literature Survey on Available Experiments", arXiv:1204.0001 (2012), https://arxiv.org/abs/1204.0001 (reports Dahneke 1973, Beger 2012, King et al. 2011 and Durda et al. 2011)
- Wikipedia, "Coefficient of restitution" (CC BY-SA 4.0), https://en.wikipedia.org/wiki/Coefficient_of_restitution
- FIBA official basketball rules documents, https://www.fiba.basketball/documents (as cited by Wikipedia)
- USGA conforming-club page, web.archive.org/web/20210616221005/https://www.usga.org/ConformingGolfClub/conforming_golf_club.asp (as cited by Wikipedia)
- USGA, "Do Long Hitters Get An Unfair Advantage?", https://www.usga.org/content/usga/home-page/articles/2011/04/do-long-hitters-get-an-unfair-advantage-2147496940.html (as cited by Wikipedia; named by the second check, 2026-11-17)
- racquetresearch.com/coeffici.htm (backs a tennis-racquet value, not the golf row)
- ITF "Tennis Tech" page, the reference Wikipedia attaches to the table-tennis rule (no URL recorded in this file; the citation does not match the claim)
