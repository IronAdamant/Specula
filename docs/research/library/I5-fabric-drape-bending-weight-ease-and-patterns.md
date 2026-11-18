# I5 Fabric bending stiffness, drape coefficient, fabric weight, garment ease and open flat patterns

Fixed after the second check (2026-11-18): Mosleh's fabric table (Table 2) is in section 3.2, not "section 5" (the paper has no section 5); the three public-domain drafting manuals, listed as leads and never opened, are moved under Unconfirmed. Added from the checker: Mosleh's own citation of a study where insulation falls past a 1 cm ease (a spread against the 15 mm convection onset, under Sources disagree); each Petrak Kd is a mean of five specimens; the 18 cm disc is stated for the simulation model; the Commons warning on the 1898 pattern's measurements; the MoMu year not being in the Commons description; and that the four papers carry no access date in the delivery.

Round 4, gap 8. Values were retyped from tables in open-access papers read through Europe PMC fullTextXML, and from Wikipedia raw wikitext (CC BY-SA 4.0). Unit conversions are "computed by me".

## 1. Nine apparel fabrics: KES and FAST bending, shear, extension, plus the Cusick drape coefficient

Source: Petrak S, Mahnic Naglic M, Rogale D, Gersak J (2021) "Analysis of Polygonal Computer Model Parameters and Influence on Fabric Drape Simulation". Materials 14:6259. doi:10.3390/ma14216259. PMC8584912. Licence CC BY 4.0 (read from the article).

Drape test (text): Cusick drape meter; 30 cm diameter circular specimens on an 18 cm support disc. (Second check, 2026-11-18: the 30 cm real specimen is stated directly; the 18 cm disc is stated for the simulation model, "a solid disk model with a diameter of 18 cm, as found in the" real device.) Kd = drape coefficient (higher = stiffer, less drape).

Fabrics (Table 1) and drape coefficient (Table 3):

| Code | Composition | Weave | Ends / picks per cm | Mass (g/m^2) | Kd |
|---|---|---|---|---|---|
| F1 | 100% polyester | Twill 3/1 | 112.2 / 41.2 | 135.2 | 0.222 |
| F2 | 100% polyester | Plain | 72.8 / 36.3 | 83.7 | 0.227 |
| F3 | 63% PES, 23% viscose, 4% elastane (as printed) | Twill 2/1 | 35.7 / 28.2 | 286.1 | 0.285 |
| F4 | 100% wool | Plain | 38.9 / 35.9 | 161.8 | 0.319 |
| F5 | 75% wool, 15% PES, 10% elastane | Twill 2/1 | 23.7 / 24.1 | 178.7 | 0.348 |
| F6 | 100% polyester | Plain | 19.6 / 18.0 | 159.8 | 0.403 |
| F7 | 97% PES, 3% elastane | Plain | 23.2 / 20.8 | 178.1 | 0.442 |
| F8 | 65% cotton, 31% PES, 4% elastane | Twill 2/1 | 42.6 / 18.1 | 253.4 | 0.548 |
| F9 | 65% PES, 35% viscose | Plain | 42.7 / 31.1 | 366.8 | 0.647 |

Mechanical properties (Table 2). Suffix 1 = warp, 2 = weft. KES: EM = extension (%); B = bending rigidity, units as printed "cNcm" (KES B is normally cN cm^2/cm); G = shear rigidity, units as printed; T0 = thickness (mm). FAST: E100 = extension at 100 gf/cm (%); B in micro-N m; G in N/m; ST = surface thickness (mm).

| Code | KES EM-1 / EM-2 | KES B-1 / B-2 | KES G | KES T0 | FAST E100-1 / -2 | FAST B-1 / B-2 | FAST G | FAST ST |
|---|---|---|---|---|---|---|---|---|
| F1 | 2.75 / 6.92 | 0.0195 / 0.0061 | 0.41 | 0.405 | 1.1 / 3.9 | 4.5 / 1.9 | 23 | 0.060 |
| F2 | 3.90 / 31.28 | 0.0085 / 0.0033 | 0.46 | 0.356 | 1.6 / 18.0 | 2.1 / 1.1 | 16 | 0.080 |
| F3 | 21.54 / 22.05 | 0.0297 / 0.0240 | 0.61 | 0.900 | 11.9 / 11.7 | 7.7 / 6.3 | 27 | 0.174 |
| F4 | 3.02 / 9.58 | 0.0324 / 0.0161 | 0.73 | 0.439 | 1.4 / 4.9 | 7.6 / 3.9 | 33 | 0.082 |
| F5 | 9.05 / 11.49 | 0.0214 / 0.0200 | 0.50 | 0.539 | 4.9 / 5.9 | 5.7 / 5.3 | 24 | 0.086 |
| F6 | 12.41 / 9.63 | 0.0362 / 0.3980 | 0.49 | 0.600 | 6.5 / 5.1 | 9.1 / 9.3 | 24 | 0.096 |
| F7 | 7.02 / 7.75 | 0.0532 / 0.4440 | 0.69 | 0.510 | 3.2 / 3.4 | 11.9 / 9.8 | 38 | 0.072 |
| F8 | 3.48 / 21.15 | 0.0743 / 0.0370 | 1.26 | 0.980 | 1.6 / 10.6 | 32.1 / 10.3 | 60 | 0.225 |
| F9 | 3.97 / 6.27 | 0.3354 / 0.0989 | 1.15 | 2.420 | 1.5 / 2.8 | 94.3 / 23.4 | 67 | 0.960 |

Spread: across these nine fabrics, Kd runs 0.222 to 0.647 and KES warp B runs 0.0085 to 0.3354. The paper gives one value per fabric (no repeat SD); each value is the mean of five specimens ("the measurement for each fabric was performed on five samples, and the results were expressed as the mean", second check, 2026-11-18).

## 2. One cotton/PES/elastane twill: KES-FB and the CLO3D values derived from it

Source: Mahnic Naglic M, Petrak S, Tomljenovic A (2025) "Analysis of Woven Fabric Mechanical Properties in the Context of Sustainable Clothing Development Process". Polymers 17:2013. doi:10.3390/polym17152013. PMC12349057. Licence CC BY 4.0 (read from the article).

Fabric M1 (Table 1): 65% cotton, 33% PES, 2% elastane; twill; 40 ends/cm warp and 20 threads/cm weft (as laid out in the XML table); 226.58 g/m^2; 0.52 mm.

KES-FB (Table 2): EMT-1 (warp elongation at 490.35 cN) 1.730%; EMT-2 (weft) 11.180%; B-1 (warp bending stiffness) 0.356 cN cm; B-2 (weft) 0.093; G (shear stiffness) 0.010 (units as printed). CLO3D inputs derived from these: stretch warp 482,012 and weft 441,042 g/s^2; bend warp 3560 and weft 930 g mm^2/s^2 (the table labels both "Bend Warp"); shear 100,000 g/s^2.

## 3. Nylon fabrics by weight: drape ratio (cylinder method)

Source: Shim E, Koo DS, An JS, Yoon HJ (2022) "Comparison of the Silhouette of Virtual Clothes by Fabric Characteristics of Nylon Fabric for the Utilization of Virtual Clothes". ACS Omega 7:47156. doi:10.1021/acsomega.2c06302. PMC9773953. Licence CC BY-NC-ND 4.0 (read from the article): facts only.

Drape test (text): "KS K ISO 9073-0:2008" as written; 25 cm circular samples on an acrylic cylinder 7.5 cm in diameter and 30 cm tall, photographed from the top and side. Bending stiffness from bending length per KS K ISO 9073-7.

Table 6, weight per unit area (CLO kit vs KES-FB) and drape ratio; g/m^2 = mg/cm^2 x 10, computed by me:

| Class | Weight, CLO (mg/cm^2) | Weight, KES (mg/cm^2) | approx g/m^2 (computed by me) | Drape ratio | Nodes |
|---|---|---|---|---|---|
| Light | 3.74 | 3.65 / 3.77 / 3.81 | about 37 | 0.23 / 0.28 / 0.32 | 4 / 4 / 3 |
| Medium | 7.02 / 7.07 / 7.17 | 7.11 / 7.42 / 7.21 | about 70 to 74 | 0.35 / 0.24 / 0.33 | 4 / 5 / 4 |
| Heavy | 14.44 / 14.44 / 14.85 | 14.42 / 14.99 / 14.31 | about 143 to 150 | 0.20 / 0.25 / 0.21 | 6 / 6 / 5 |

These rows are per sample within each class (three samples each).

## 4. Typical fabric weights (Wikipedia, CC BY-SA 4.0)

Source: Wikipedia, "Units of textile measurement" (raw wikitext read 2026-11-17), CC BY-SA 4.0. https://en.wikipedia.org/wiki/Units_of_textile_measurement
- "Typically, cheap T-shirt fabric is approximately 150 g/m2" (cited there to Gandhi 2019, p. 309, and a retailer page; secondary).
- Silk momme ranges: habutai 5 to 16 mm; chiffon 6 to 8 mm (12 to 16 double); crepe de chine 12 to 16; gauze 3 to 5; silk noil 35 to 40; organza 4 to 6; charmeuse 12 to 30. The momme is defined there as the weight in pounds of 45 in by 100 yd.
- Computed by me from that definition: 1 momme = 453.592 g / (1.143 m x 91.44 m) = 4.34 g/m^2. So charmeuse at 12 to 30 mm is about 52 to 130 g/m^2, and chiffon at 6 to 8 mm about 26 to 35 g/m^2.

Fabric used in a garment thermal study (Mosleh et al. 2021, section 3.2, Table 2; corrected by the second check, 2026-11-18, from "section 5"): cotton, 126 g/m^2, 0.312 mm, warp 585.6 and weft 284 per 10 cm, 0.155 clo.

## 5. Garment ease

Wikipedia, "Ease (sewing)" (raw wikitext read 2026-11-17), CC BY-SA 4.0. The article carries an "original research" banner (Dec 2014). https://en.wikipedia.org/wiki/Ease_(sewing)
- Wearing ease, citing English, S. (2016) Sewing Patterns: 200 Questions and Answers, pp. 68-9 (SECONDARY): 2 1/2 in at the bust, 1 in at the waist, 3 in at the hip (6.4, 2.5 and 7.6 cm, computed by me).
- Ready-to-wear ease, citing King, K. D. (2018) Smart Fitting Solutions (SECONDARY): 2 in bust, 1 in waist, 2 in hips (5.1, 2.5 and 5.1 cm, computed by me).
- Ease categories, citing Vogue Sewing (2006) p. 119: close-fitting, fitted, semi-fitted, loose-fitting and very loose-fitting (the article gives no amounts for these).

Air-gap ease in five shirt sizes. Source: Mosleh S, Abtew MA, Bruniaux P, Tartare G, Loghin E-C, Dulgheriu I (2021) "Modeling and Simulation of Human Body Heat Transfer System Based on Air Space Values in 3D Clothing Model". Materials 14:6675. doi:10.3390/ma14216675. PMC8587069. Licence CC BY 4.0 (read from the article).
- Table 1, mean air gap (ease allowance): garment A 0.5 cm; B 0.8; C 1.15; D 1.55; E 1.95 cm.
- Text: natural convection starts once the air gap is thicker than 15 mm.

## 6. Open flat patterns (links and images)

- The Cutter's Practical Guide, 1898, Part 1, Plate 1 "The Shirt System" (front, yoke, back, sleeve, cuff, collar). Public domain per the Commons file page. Image at images/cutters-practical-guide-1898-shirt-system.png in the delivery (the pictures are not in this library). https://commons.wikimedia.org/wiki/File:Cutter%27s_Practical_Guide,_1898_Edition,_Part_1_-_Plate_1.svg (second check, 2026-11-18: the file is a redraw tagged PD-self by Isarra, and its Commons description warns "Measurement numbers aren't guaranteed to be accurate").
- MoMu Antwerp, bodice of ensemble ST2240AB, 2023 Pattern-a-thon. CC BY-SA 4.0. (Second check, 2026-11-18: Commons says "Made during a MoMu Pattern-a-thon"; the year 2023 is not in the description the checker read.) Image at images/momu-ST2240AB-bodice-patterns.png in the delivery (not in this library). https://commons.wikimedia.org/wiki/File:Patterns_for_the_bodice_of_an_ensemble_(ST2240AB)_-_MoMu_Antwerp_Study_Collection.svg
- Underpants model "KALSON" (4 parts, sizes M, L, XL), by Liberumia, CC BY-SA 4.0 (licence read via the Commons API; not downloaded). https://commons.wikimedia.org/wiki/File:Underpants_model_KALSON.svg
- FreeSewing (parametric made-to-measure patterns): the code licence is MIT ("Copyright (c) 2022 Joost De Cock", read from the project's LICENSE file at raw.githubusercontent.com/freesewing/freesewing/develop/LICENSE). I did not check the licence of the generated pattern output or the documentation (the freesewing.dev licence page returned 404). https://freesewing.org
- Three drafting manuals listed in the delivery as public domain: under Unconfirmed.

## Sources disagree

- Petrak et al. Table 2, KES B-2 (weft bending) for F6 = 0.3980 and F7 = 0.4440. These are about 10 times their warp values and the other fabrics' weft values. The same fabrics' FAST weft bending (9.3 and 9.8 micro-N m) is close to their warp (9.1 and 11.9). These two KES entries are probably misplaced decimals (0.0398 and 0.0444); my reading, not stated by the authors.
- Drape methods differ: Petrak uses Cusick (30 cm specimen on an 18 cm disc, coefficient Kd); Shim uses a 25 cm specimen on a 7.5 cm cylinder (a "drape ratio"). The numbers are not interchangeable.
- Ease: English (2016) gives 3 in hip wearing ease; King (2018) gives 2 in for ready-to-wear (both via Wikipedia).
- Air gap and insulation: Mosleh et al. simulate natural convection starting once the air gap passes 15 mm, but their introduction cites another study (their ref [26]) in which shirt insulation began to fall above a 1 cm ease allowance or a 6000 cm³ air gap (second check, 2026-11-18).

## Not found

- Full KES-FB block (2HB hysteresis, 2HG5, MIU, MMD, SMD, LT, WT, RT) for a range of fabrics from an opened open source.
- Drape coefficients with SD from repeat tests.
- An open, measured table of garment ease by garment type and fit category (only the Wikipedia-relayed bust, waist and hip figures and the Mosleh air gaps above).
- Denim, knit jersey and leather KES values from an opened source (a sheep garment leather drape paper exists, PMC8398511, not opened).

## Unconfirmed (second check, 2026-11-18)

- Three drafting manuals listed in the delivery as public-domain scans on Commons: "Manual of apparel drafting and sewing" (IA manualofappareld00kunz); "Margaret J. Blair's system of sewing and garment drafting" (IA margaretjblairss00blai); "A complete course in dressmaking, Vol. 8, Draping and Pattern Making" (IA completecoursein08cono). Neither the researcher nor the checker opened them, so their licence is not checked. Leads only: **not to be used until a source that gives their licence (their own pages) has been opened.**

## Sources

The four papers carry no access date in the delivery; the second checker read them through Europe PMC on 2026-11-18. The Europe PMC URLs were made from the PMC numbers the note gives. The Wikipedia raw wikitext was read 2026-11-17; the Commons and FreeSewing pages carry no date in the delivery.

1. Petrak S et al. 2021, Materials 14:6259, PMC8584912, CC BY 4.0 (Tables 1, 2, 3; methods text). https://europepmc.org/article/PMC/PMC8584912 ; https://doi.org/10.3390/ma14216259
2. Mahnic Naglic M et al. 2025, Polymers 17:2013, PMC12349057, CC BY 4.0 (Tables 1, 2). https://europepmc.org/article/PMC/PMC12349057 ; https://doi.org/10.3390/polym17152013
3. Shim E et al. 2022, ACS Omega 7:47156, PMC9773953, CC BY-NC-ND 4.0 (Table 6; methods text). https://europepmc.org/article/PMC/PMC9773953 ; https://doi.org/10.1021/acsomega.2c06302
4. Mosleh S et al. 2021, Materials 14:6675, PMC8587069, CC BY 4.0 (Tables 1, 2, sections 3.2 and introduction; text). https://europepmc.org/article/PMC/PMC8587069 ; https://doi.org/10.3390/ma14216675
5. Wikipedia, Units of textile measurement, CC BY-SA 4.0, read 2026-11-17. https://en.wikipedia.org/wiki/Units_of_textile_measurement
6. Wikipedia, Ease (sewing), CC BY-SA 4.0, read 2026-11-17. https://en.wikipedia.org/wiki/Ease_(sewing)
7. Wikimedia Commons file pages for the patterns (licences as listed in section 6): https://commons.wikimedia.org/wiki/File:Cutter%27s_Practical_Guide,_1898_Edition,_Part_1_-_Plate_1.svg ; https://commons.wikimedia.org/wiki/File:Patterns_for_the_bodice_of_an_ensemble_(ST2240AB)_-_MoMu_Antwerp_Study_Collection.svg ; https://commons.wikimedia.org/wiki/File:Underpants_model_KALSON.svg
8. FreeSewing LICENSE (MIT). https://raw.githubusercontent.com/freesewing/freesewing/develop/LICENSE ; project site https://freesewing.org
