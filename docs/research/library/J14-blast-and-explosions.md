# J14: Blast waves, overpressure, shock speed, fragments

Fixed after the second check (2026-11-18):
- J14-1: the scaled distance Z = R / W^(1/3) now cites Filice et al. 2022 (text and Eq. 2-4); the Wikipedia article "Hopkinson-Cranz scaling" it cited does not exist.
- J14-2: Brode at Z = 2 is 156.4 kPa (not 158), so Brode runs 18-25% below Kinney and Graham (not 18-24%).
- Eight rows that rest on Goel et al. 2012 alone (the PDF could not be opened) moved under "Unconfirmed": the ground-burst factor 1.8, Kingery-Bulmash to Z = 40, the Kinney and Graham impulse (Eq. 15), Brode, Henrych, Sadovskiy, Goel's conclusions, and Friedlander with Larcher's b. The impulse column of Table J14-2a is flagged as resting on Eq. 15.
- Added: Filice's Kingery-Bulmash range (Z = 0.2-198.5) under "Sources disagree"; the NIOSH fatality bands and the DTIC eardrum, lung and survival figures from the same Overpressure page; Filice's Z = 2.76 is after conversion to TNT equivalent.
- A "Sources" list added at the end.

Researched 2026-11-17 (all URLs accessed 2026-11-17). Values marked "computed by me" are arithmetic on the cited formulas, never measurements. This file is for drawing and simulating explosions in a game; it is not safety or engineering guidance.

## J14-1. TNT equivalence and scaled distance (read directly)

| Item | Value or formula | Source |
|---|---|---|
| Energy of 1 g of TNT (convention) | 4,184 J, exactly 1 kcal; 1 ton of TNT = 4.184 GJ | NIST SP 811, Appendix B.8, as cited by Wikipedia "TNT equivalent", https://en.wikipedia.org/wiki/TNT_equivalent (secondary) |
| Scaled (Hopkinson-Cranz) distance | Z = R / W^(1/3), R in m, W = TNT-equivalent mass in kg; at the same Z, the same explosive gives the same blast parameters whatever the charge size | Filice, Mynarz and Zinno 2022 (below), text and Eq. 2-4 (there is no Wikipedia article "Hopkinson-Cranz scaling", which this row first cited) |
| Ground (hemispherical) burst | unconfirmed: see "Unconfirmed" below | |
| Kingery-Bulmash range | Filice et al. 2022, Table 3: the simplified Kingery-Bulmash peak-overpressure fit is valid over Z = 0.2-198.5 m/kg^(1/3) (another range is under "Unconfirmed" and "Sources disagree") | Filice et al. 2022 |
| Kingery-Bulmash source | Kingery and Bulmash, *Airblast Parameters from TNT Spherical Air Burst and Hemispherical Surface Burst*, ARBRL-TR-02555, US Army BRL, 1984: parameters given as high-order polynomials in ln Z | as cited by Filice, Mynarz and Zinno 2022, "Experimental and Empirical Study for Prediction of Blast Loads", *Applied Sciences* 12(5) 2691, doi:10.3390/app12052691, https://www.mdpi.com/2076-3417/12/5/2691 |

## J14-2. Peak overpressure, duration and impulse against scaled distance (formulas read directly)

From Goel, Matsagar, Gupta and Marburg 2012, "An Abridged Review of Blast Wave Parameters", *Defence Science Journal* 62(5) 300-306, doi:10.14429/dsj.62.1149, equations as printed:

- **Kinney and Graham** peak overpressure (Eq. 8): P / P0 = 808 [1 + (Z/4.5)²] / √{[1 + (Z/0.048)²] [1 + (Z/0.32)²] [1 + (Z/1.35)²]}, P0 = ambient pressure.
- **Kinney and Graham** positive-phase duration (Eq. 12): t / W^(1/3) = 980 [1 + (Z/0.54)^10] / {[1 + (Z/0.02)³] [1 + (Z/0.74)⁶] √[1 + (Z/6.9)²]} ms.
- The Kinney and Graham impulse, the Brode, Henrych and Sadovskiy formulas, Goel et al.'s conclusions and the Friedlander shape with Larcher's fit are unconfirmed (the second checker could not open Goel et al.'s PDF): see "Unconfirmed" below.

### Table J14-2a. Kinney and Graham values (computed by me; free air, P0 = 101.325 kPa)

| Z (m/kg^(1/3)) | Peak overpressure | Positive duration per kg^(1/3) | Impulse per kg^(1/3) | Shock Mach number | Shock speed |
|---|---|---|---|---|---|
| 0.5 | 4,000 kPa (39.5 atm) | 0.08 ms | 1.28 bar·ms | 5.9 | 2,030 m/s |
| 1 | 1,009 kPa | 0.52 ms | 1.17 bar·ms | 3.09 | 1,060 m/s |
| 2 | 208 kPa | 1.17 ms | 0.86 bar·ms | 1.66 | 570 m/s |
| 3 | 82 kPa | 1.68 ms | 0.63 bar·ms | 1.30 | 447 m/s |
| 5 | 29 kPa | 2.47 ms | 0.39 bar·ms | 1.12 | 383 m/s |
| 7 | 16.6 kPa | 3.0 ms | 0.28 bar·ms | 1.07 | 366 m/s |
| 10 | 10.0 kPa | 3.5 ms | 0.20 bar·ms | 1.04 | 357 m/s |
| 15 | 6.1 kPa | 3.8 ms | 0.13 bar·ms | 1.025 | 352 m/s |
| 20 | 4.4 kPa | 4.0 ms | 0.10 bar·ms | 1.018 | 349 m/s |
| 40 | 2.1 kPa | 4.2 ms | 0.05 bar·ms | 1.009 | 346 m/s |

Shock Mach number from the normal-shock pressure ratio p1/p0 = [2γM² − (γ − 1)] / (γ + 1) with γ = 1.4, NASA Glenn "Normal Shock Wave Equations", https://www.grc.nasa.gov/www/k-12/airplane/normal.html, solved for M by me; shock speed = M × 343 m/s (sound at 20 °C, round-two J9). The impulse column is computed from Kinney and Graham's Eq. 15, which is unconfirmed (see "Unconfirmed" below): do not use that column until the equation is confirmed. Brode's formula (also unconfirmed) gives 826 kPa at Z = 1 and 156.4 kPa at Z = 2 (computed by me; corrected after the second check, which recomputed 156.4 where this file had 158), 18-25% below Kinney and Graham there (Z = 1: 826 against 1,009 kPa, 18%; Z = 2: 156.4 against 208 kPa, 25%).

Real check from Filice et al. 2022 (Tables 11 and 12, 400 g Semtex 10-SE at 2 m, Z = 2.76; the raw Z of 400 g at 2 m is 2.71, so the paper's 2.76 is after conversion to TNT equivalent): Kingery-Bulmash predicted 138.5 kPa (+48.3% against measured) and Kinney-Graham 99.4 kPa (+6.4%), so the measured peak was about 93 kPa (computed by me). At 5 m (Z = 6.89) Kinney-Graham gave 17.0 kPa and Kingery-Bulmash 25.5 kPa. The paper's overall finding: Kingery-Bulmash came out above the measurements in all cases, Kinney-Graham, Henrych and PECH on average below.

## J14-3. What overpressure does (read directly, secondary)

Wikipedia "Overpressure", table "Overpressure ... Effect on buildings and people within", citing CDC NIOSH "Explosions and refuge chambers" (https://www.cdc.gov/niosh/docket/archive/pdfs/niosh-125/125-explosionsandrefugechambers.pdf), https://en.wikipedia.org/wiki/Overpressure:

| Overpressure | Effect (long-duration blast) |
|---|---|
| 1 psi (6.9 kPa) | window glass shatters; light injuries from fragments |
| 2 psi (13.8 kPa) | moderate damage to houses (windows and doors blown out, severe roof damage) |
| 3 psi (20.7 kPa) | residential structures collapse; serious injuries common |
| 5 psi (34.5 kPa) | most buildings collapse except reinforced concrete |
| 10 psi (68.9 kPa) | reinforced concrete buildings severely damaged or demolished |
| 20 psi (138 kPa) | heavily built reinforced concrete severely damaged or demolished |

Same page, quoted from the NIOSH document: eardrum rupture in about 1% of people at 5 psi and about 99% at 45 psi; lung damage threshold about 15 psi; fatalities in about 1% at 35-45 psi and about 99% at 55-65 psi. Quoted from DTIC AD0067365 ("Blast Effects of Bomb Explosives"): a standing person is blown away at about 10 ft/s by a 25 psi peak shock; 50% eardrum rupture at 15 psi; severe lung injury at 70-100 psi; 50:50 survival at about 500 psi (all instantaneous peak pressure, not long duration).

### Table J14-3a. Free-air distance to each threshold (computed by me, Kinney and Graham)

| Threshold | Z | 0.1 kg TNT | 1 kg | 10 kg | 100 kg | 1,000 kg |
|---|---|---|---|---|---|---|
| 1 psi, glass breaks | 13.5 | 6.2 m | 13.4 m | 29 m | 62 m | 135 m |
| 2 psi | 7.9 | 3.7 m | 7.9 m | 17 m | 37 m | 79 m |
| 3 psi | 6.1 | 2.8 m | 6.1 m | 13 m | 28 m | 61 m |
| 5 psi | 4.6 | 2.1 m | 4.6 m | 9.9 m | 21 m | 46 m |
| 10 psi | 3.3 | 1.5 m | 3.3 m | 7.0 m | 15 m | 33 m |
| 20 psi | 2.4 | 1.1 m | 2.4 m | 5.1 m | 11 m | 24 m |

For a charge on the ground, the factor this file gave is unconfirmed (see "Unconfirmed" below).

## J14-4. Fragment throw (read directly, secondary)

Gurney equations, Wikipedia "Gurney equations", https://en.wikipedia.org/wiki/Gurney_equations (table citing Cooper, *Explosives Engineering*):

- Cylinder of explosive mass C inside a metal case of mass M: V / √(2E) = (M/C + 1/2)^(−1/2).
- Sphere: V / √(2E) = (M/C + 3/5)^(−1/2).
- Gurney velocity √(2E), km/s: TNT 2.44 (density 1.63 g/cm³, detonation 6.86 km/s); Composition B 2.70; RDX 2.83; PETN 2.93; HMX 2.97; Tetryl 2.50; Tritonal 2.32.
- Accuracy: within about 10% of experiment or detailed simulation for 0.1 < M/C < 10 (same page, citing Walters). Rule of thumb: √(2E) is close to one third of the detonation velocity.

### Table J14-4a. Initial fragment speed, TNT (computed by me)

| Case-to-charge mass ratio M/C | Cylinder | Sphere |
|---|---|---|
| 0.5 | 2.44 km/s | 2.33 km/s |
| 1 | 1.99 km/s | 1.93 km/s |
| 2 | 1.54 km/s | 1.51 km/s |
| 5 | 1.04 km/s | 1.03 km/s |
| 10 | 0.75 km/s | 0.75 km/s |

Fragments then slow by drag (round-two J4 and J19 here). How far they fly needs the fragment's mass, shape and drag, which were not found as an open table.

## Sources disagree

- Near-field overpressure: at Z = 2.76, Kingery-Bulmash over-predicted a measured 400 g charge by 48% while Kinney-Graham was within 6% (Filice et al. 2022, Tables 11-12). Goel et al. are said to stress the wide variation between methods too (unconfirmed: see below).
- Range of Kingery-Bulmash: Goel et al. 2012 (as this file first read it; unconfirmed) give the charts as valid up to Z = 40 m/kg^(1/3); Filice et al. 2022, Table 3, give the simplified Kingery-Bulmash peak-overpressure fit as valid over Z = 0.2-198.5 m/kg^(1/3).
- Relative effectiveness (RE) of other explosives against TNT: Wikipedia "TNT equivalent" has a table but it is flagged "citation needed", so it is not used here.

## Not found

- An open copy of UFC 3-340-02 or the JRC report (Karlos and Solomos 2013, EUR 26456 EN): both exist publicly, but publications.jrc.ec.europa.eu and DTIC returned HTTP 403 to the box. Read them in a normal browser: https://publications.jrc.ec.europa.eu/repository/bitstream/JRC87200/lbna26456enn.pdf and https://www.wbdg.org/dod/ufc/ufc-3-340-02.
- Mott fragment-size distribution and fragment range tables.
- A sourced RE factor table.

## Unconfirmed (second check, 2026-11-18)

Not to be used until a source that gives the value has been opened. The checker's reason, the same for all eight: these rest on Goel, Matsagar, Gupta and Marburg 2012 alone, whose PDF sits behind a JavaScript bot-check; the journal's article page (https://publicationsdrdo.in/index.php/dsj/article/view/1149) confirms only the title, authors, *Def. Sci. J.* 62(5) 300-306 and the DOI, and the Wayback Machine's copy timed out.

- Ground (hemispherical) burst: multiply the charge by 1.8 to account for ground reflection when using free-air formulas (Goel et al. 2012). With it: for a charge on the ground, use 1.8 W in the formulas, which pushes each distance in Table J14-3a out by a factor of 1.8^(1/3) = 1.22 (computed by me).
- Kingery-Bulmash range: charts valid up to Z = 40 m/kg^(1/3) (Goel et al. 2012, text on Kingery-Bulmash charts). Filice et al. 2022, Table 3, give 0.2-198.5 instead (see "Sources disagree").
- **Kinney and Graham** positive impulse (Eq. 15): I = 0.067 √[1 + (Z/0.23)⁴] / {Z² [1 + (Z/1.55)³]^(1/3)} bar·ms (per kg^(1/3) of charge). The impulse column of Table J14-2a is computed from it.
- **Brode** (Eq. 2-3): P = 6.7/Z³ + 1 bar when P > 10 bar; P = 0.975/Z + 1.455/Z² + 5.85/Z³ − 0.019 bar when 0.1 < P < 10 bar. (The comparison under Table J14-2a is computed from it.)
- **Henrych** (Eq. 4-6): three Z bands, 0.05-0.3, 0.3-1, 1-10 m/kg^(1/3).
- **Sadovskiy** (Eq. 9): P = 0.085 (W^(1/3)/R) + 0.3 (W^(1/3)/R)² + 0.8 (W^(1/3)/R)³ MPa.
- Goel et al. conclude (Conclusions) that Kinney and Graham's equations are the most commonly used because of their close agreement with experiments and are the most suitable for the positive phase, and that there is "a wide variation in results" between the available methods.
- **Friedlander pressure-time shape** (Goel et al. Eq. 1): P(t) = P0 + P_peak (1 − t/t_pos) e^(−b t / t_pos), b = decay parameter; Larcher et al. fit b = 5.2777 Z^(−1.1975) (Eq. 27).

## Sources

- Wikipedia, "TNT equivalent" (citing NIST SP 811, Appendix B.8). https://en.wikipedia.org/wiki/TNT_equivalent . CC BY-SA 4.0. Accessed 2026-11-17.
- Goel, Matsagar, Gupta and Marburg (2012), "An Abridged Review of Blast Wave Parameters", *Defence Science Journal* 62(5) 300-306, doi:10.14429/dsj.62.1149. https://publicationsdrdo.in/index.php/dsj/article/view/1149 . Accessed 2026-11-17 (the PDF could not be opened by the second checker).
- Filice, Mynarz and Zinno (2022), "Experimental and Empirical Study for Prediction of Blast Loads", *Applied Sciences* 12(5) 2691, doi:10.3390/app12052691. https://www.mdpi.com/2076-3417/12/5/2691 . Accessed 2026-11-17. (The second checker read it at https://dspace.vsb.cz/server/api/core/bitstreams/11c04d04-45f1-4c91-8d55-36d3df0da36b/content .)
- Kingery and Bulmash (1984), *Airblast Parameters from TNT Spherical Air Burst and Hemispherical Surface Burst*, ARBRL-TR-02555, US Army BRL (as cited by Filice et al.). No link. Not opened.
- Larcher et al. (as cited by Goel et al.). No link. Not opened.
- NASA Glenn Research Center, "Normal Shock Wave Equations". https://www.grc.nasa.gov/www/k-12/airplane/normal.html . Accessed 2026-11-17.
- Wikipedia, "Overpressure" (quoting CDC NIOSH "Explosions and refuge chambers" and DTIC AD0067365, "Blast Effects of Bomb Explosives"). https://en.wikipedia.org/wiki/Overpressure . CC BY-SA 4.0. Accessed 2026-11-17.
- CDC NIOSH, "Explosions and refuge chambers". https://www.cdc.gov/niosh/docket/archive/pdfs/niosh-125/125-explosionsandrefugechambers.pdf . Accessed 2026-11-17.
- Wikipedia, "Gurney equations" (citing Cooper, *Explosives Engineering*, and Walters). https://en.wikipedia.org/wiki/Gurney_equations . CC BY-SA 4.0. Accessed 2026-11-17.
- Karlos and Solomos (2013), JRC report EUR 26456 EN (named under "Not found"; not opened). https://publications.jrc.ec.europa.eu/repository/bitstream/JRC87200/lbna26456enn.pdf .
- UFC 3-340-02 (named under "Not found"; not opened). https://www.wbdg.org/dod/ufc/ufc-3-340-02 .
- Round-two J9, J4 and this round's J19 of this library (speed of sound; drag).
