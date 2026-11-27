# Second check: R7-01-DS1-serio-values.md

Independent re-check, 2026-11-24. Every count and every quadruped-set value was recomputed from the unzipped Dryad files with new code (pandas and numpy), written separately from the code that built the tables. Sample SD is `numpy.std(..., ddof=1)`. Means and SDs were rounded to two decimal places with ordinary half-to-even formatting and compared to the figures printed in the file. No sign-in was used.

Result: **0 mismatches**, 84 verified items.

| Status | Item | What was re-checked | Result |
|---|---|---|---|
| VERIFIED | README has no licence wording | searched the data README for licence/license/CC0/CC BY/Creative Commons/copyright/public domain/rights | none present, as stated |
| VERIFIED | Seven file sizes and 26,144,481 byte total | stat() on the unzipped files | match |
| VERIFIED | Row counts (Data_1 3,177 / 420 species; Data_2 435 / 432 names; Data_3 93; Data_4 22; Data_5 25) | pandas read of each CSV | match |
| VERIFIED | Data_6.zip is 50 TIFFs in Simulations/ | zipfile namelist | match |
| VERIFIED | Data_1 column names, including double spaces in Femur and Tibia | header row | match |
| VERIFIED | Measurement value and species counts (living and all) for all seven numeric columns | numpy/pandas notna counts joined to Data_2 LA=0 | match |
| VERIFIED | 247 living / 173 fossil / 420 Data_1 species (LA=0 rule) | Data_2 last appearance joined to Data_1 | match |
| VERIFIED | Species and row counts by order, living and fossil | Data_2 Order joined to Data_1 | match |
| VERIFIED | No body-mass column in Data_1 or Data_2 | column names | match |
| VERIFIED | No Proboscidea, Lagomorpha or Rodentia in Data_2 | Order column | match |
| VERIFIED | Found/missing species list (no dog, elephant, rabbit, rat; all named quadruped taxa present) | species strings and a field search for breed/dog words | match |
| VERIFIED | 65 measurement cells parsed from the two value tables (12 requested species plus Przewalski's horse) | markdown parse | match |
| VERIFIED | Canis_lupus Humerus (mm) n=7 mean=220.57 SD=9.29 range=205.7-229.9 | numpy mean and np.std(ddof=1) on 7 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Canis_lupus Radius (mm) n=7 mean=223.23 SD=10.66 range=205.7-234 | numpy mean and np.std(ddof=1) on 7 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Canis_lupus Femur  (mm) n=9 mean=238.89 SD=12.58 range=218.2-252.2 | numpy mean and np.std(ddof=1) on 9 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Canis_lupus Tibia  (mm) n=5 mean=200.96 SD=41.16 range=152.9-235.1 | numpy mean and np.std(ddof=1) on 5 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Canis_lupus Greatest skull length (mm) n=37 mean=252.11 SD=23.92 range=198.859-319.168 | numpy mean and np.std(ddof=1) on 37 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Felis_catus Humerus (mm) n=3 mean=97.57 SD=1.53 range=96.4-99.3 | numpy mean and np.std(ddof=1) on 3 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Felis_catus Radius (mm) n=3 mean=92.83 SD=1.59 range=91-93.8 | numpy mean and np.std(ddof=1) on 3 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Felis_catus Femur  (mm) n=2 mean=106.65 SD=5.16 range=103-110.3 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Felis_catus Tibia  (mm) n=1 value 111.6 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Felis_catus Greatest skull length (mm) n=1 value 93.3 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Panthera_leo Humerus (mm) n=6 mean=298.57 SD=31.25 range=264.8-347 | numpy mean and np.std(ddof=1) on 6 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_leo Radius (mm) n=6 mean=270.90 SD=24.32 range=250-310.5 | numpy mean and np.std(ddof=1) on 6 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_leo Femur  (mm) n=6 mean=298.33 SD=31.97 range=262.6-347.1 | numpy mean and np.std(ddof=1) on 6 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_leo Tibia  (mm) n=2 mean=295.40 SD=9.05 range=289-301.8 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_leo Greatest skull length (mm) n=17 mean=327.91 SD=35.64 range=278.68-379 | numpy mean and np.std(ddof=1) on 17 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_tigris Humerus (mm) n=8 mean=309.43 SD=23.36 range=267-343 | numpy mean and np.std(ddof=1) on 8 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_tigris Radius (mm) n=8 mean=260.81 SD=19.65 range=235-290.6 | numpy mean and np.std(ddof=1) on 8 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_tigris Femur  (mm) n=8 mean=308.97 SD=27.24 range=275.8-364.773 | numpy mean and np.std(ddof=1) on 8 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_tigris Tibia  (mm) n=2 mean=306.30 SD=3.25 range=304-308.598 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_tigris Greatest skull length (mm) n=27 mean=317.55 SD=29.87 range=250-357 | numpy mean and np.std(ddof=1) on 27 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_pardus Humerus (mm) n=1 value 211.2 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Panthera_pardus Radius (mm) n=1 value 263.6 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Panthera_pardus Femur  (mm) n=2 mean=293.20 SD=85.14 range=233-353.4 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_pardus Tibia  (mm) n=2 mean=262.65 SD=56.07 range=223-302.3 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Panthera_pardus Greatest skull length (mm) n=22 mean=208.66 SD=24.53 range=165.04-245 | numpy mean and np.std(ddof=1) on 22 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Equus_ferus_caballus Humerus (mm) n=2 mean=318.25 SD=22.98 range=302-334.5 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Equus_ferus_caballus Radius (mm) n=2 mean=370.00 SD=33.94 range=346-394 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Equus_ferus_caballus Femur  (mm) n=2 mean=429.75 SD=23.69 range=413-446.5 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Equus_ferus_caballus Tibia  (mm) n=2 mean=380.00 SD=33.94 range=356-404 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Equus_ferus_caballus Greatest skull length (mm) n=3 mean=546.10 SD=24.75 range=524.3-573 | numpy mean and np.std(ddof=1) on 3 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Cervus_elaphus Humerus (mm) n=1 value 285.0 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Cervus_elaphus Radius (mm) n=1 value 298.9 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Cervus_elaphus Femur  (mm) n=1 value 338.7 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Cervus_elaphus Tibia  (mm) n=1 value 384.3 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Cervus_elaphus Greatest skull length (mm) n=7 mean=331.14 SD=22.77 range=290-357 | numpy mean and np.std(ddof=1) on 7 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Odocoileus_virginianus Humerus (mm) n=2 mean=202.25 SD=18.03 range=189.5-215 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Odocoileus_virginianus Radius (mm) n=2 mean=216.40 SD=31.96 range=193.8-239 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Odocoileus_virginianus Femur  (mm) n=1 value 230.5 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Odocoileus_virginianus Tibia  (mm) n=1 value 254.5 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Odocoileus_virginianus Greatest skull length (mm) n=5 mean=254.50 SD=30.31 range=209-291 | numpy mean and np.std(ddof=1) on 5 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Bos_taurus Humerus (mm) n=1 value 262.7 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Bos_taurus Radius (mm) n=1 value 254.2 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Bos_taurus Femur  (mm) n=1 value 335.2 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Bos_taurus Tibia  (mm) n=1 value 312.8 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Bos_taurus Greatest skull length (mm) n=2 mean=512.26 SD=24.41 range=495-529.52 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Sus_scrofa Humerus (mm) n=1 value 213.0 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Sus_scrofa Radius (mm) n=1 value 161.7 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Sus_scrofa Femur  (mm) n=1 value 239.3 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Sus_scrofa Tibia  (mm) n=1 value 212.7 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Sus_scrofa Greatest skull length (mm) n=6 mean=373.57 SD=46.96 range=309-424 | numpy mean and np.std(ddof=1) on 6 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_arctos Humerus (mm) n=9 mean=343.17 SD=52.75 range=272.8-444 | numpy mean and np.std(ddof=1) on 9 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_arctos Radius (mm) n=9 mean=293.23 SD=34.89 range=243.3-348.5 | numpy mean and np.std(ddof=1) on 9 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_arctos Femur  (mm) n=5 mean=371.62 SD=39.82 range=315.8-414 | numpy mean and np.std(ddof=1) on 5 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_arctos Tibia  (mm) n=4 mean=275.60 SD=25.08 range=239-295.9 | numpy mean and np.std(ddof=1) on 4 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_arctos Greatest skull length (mm) n=9 mean=397.38 SD=69.56 range=339.13-565.6 | numpy mean and np.std(ddof=1) on 9 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_americanus Humerus (mm) n=2 mean=279.35 SD=13.65 range=269.7-289 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_americanus Radius (mm) n=2 mean=244.20 SD=31.25 range=222.1-266.3 | numpy mean and np.std(ddof=1) on 2 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_americanus Femur  (mm) n=3 mean=295.87 SD=56.65 range=232.9-342.7 | numpy mean and np.std(ddof=1) on 3 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_americanus Tibia  (mm) n=3 mean=243.97 SD=13.94 range=228-253.7 | numpy mean and np.std(ddof=1) on 3 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Ursus_americanus Greatest skull length (mm) n=7 mean=262.75 SD=26.00 range=235.83-311.1 | numpy mean and np.std(ddof=1) on 7 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Equus_przewalskii Humerus (mm) n=4 mean=259.85 SD=10.92 range=245-271 | numpy mean and np.std(ddof=1) on 4 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Equus_przewalskii Radius (mm) n=4 mean=306.02 SD=8.78 range=293-312.1 | numpy mean and np.std(ddof=1) on 4 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Equus_przewalskii Femur  (mm) n=1 value 326.0 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Equus_przewalskii Tibia  (mm) n=1 value 309.0 | recomputed from Data_1.csv with numpy | match |
| VERIFIED | Equus_przewalskii Greatest skull length (mm) n=9 mean=525.22 SD=21.12 range=485-547 | numpy mean and np.std(ddof=1) on 9 cells in Data_1.csv, rounded to 2 d.p. | match |
| VERIFIED | Wolf tibia note: excluding 152.9 and 159.3 gives n=3, mean 230.87, SD 4.90, range 225.5-235.1 (computed) | numpy on the three remaining Data_1 tibia cells | match |
| VERIFIED | Leopard femur values differ by 120.4 mm | 353.4 minus 233.0 from the two Data_1 cells | match |
| VERIFIED | Brown bear skulls: max 565.6, next 416.3, min 339.13 mm | nine Data_1 skull cells sorted with numpy | match |
| VERIFIED | No exact duplicate rows among the quadruped-set species (and Przewalski's horse) | pandas duplicated() on Data_1 subset | match |
| VERIFIED | Grotta Mora Cavorso wolf tibiae: Cat holds the site, Locality holds GMC 2823 and GMC 2744, values 152.9 and 159.3 mm | two Data_1 rows | match |
| VERIFIED | Non-empty Locality 369, Cat 1,615, Note 68, Source 3,160 of 3,177 | string counts | match |
| VERIFIED | Data_2 column typo Synonimous is in the file, as stated | header | match |

No errors found. The printed means, SDs, ranges, n=1 file values, file inventory, species found/missing list, and the three sensitivity figures in the notes (wolf tibia subset, leopard femur difference, brown bear skull extremes) all match a fresh computation from Data_1.csv and Data_2.csv.
