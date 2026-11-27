# R7-01 DS1 Serio et al. 2024: contents and quadruped-set values

Round 7 library research, question DS1 (f) and (h). Access date for the files read here: **2026-11-24**. The data were read from the owner's unzipped Dryad copy in `Dryad Data/` (README.md, Data_1.csv to Data_5.csv, Data_6.zip). `__MACOSX` was ignored.

## Source

- **Paper:** Serio C, Brown RP, Clauss M, Meloro C (2024). "Morphological disparity of mammalian limb bones throughout the Cenozoic: the role of biotic and abiotic factors." *Palaeontology* 67(4): e12720. doi:10.1111/pala.12720. Free to read. Paper licence (Crossref, already confirmed in R7-00): **CC BY 4.0**.
- **Data set:** Dryad, doi:10.5061/dryad.8gtht76sp.

## Licence

**The data package README.md does not state a licence.** It has no licence, copyright, CC, or rights wording at all.

The Dryad record itself is **CC0 1.0** (Creative Commons Zero v1.0 Universal), as already confirmed from the Dryad API (`license` = `https://spdx.org/licenses/CC0-1.0.html`) and the landing page in R7-00. CC0 allows reuse, including commercial use, without restriction. This file does not copy the data set; it reports only the counts and the quadruped-set values asked for.

---

## (f) What the data set holds

Seven files, 26,144,481 bytes (about 26.1 MB) in total. Semicolon-delimited CSV (UTF-8). Column names below are copied exactly, including the double space in `Femur  (mm)` and `Tibia  (mm)`, and the typo `Synonimous` in Data_2.

| File | Size (bytes) | What it holds |
|---|---:|---|
| README.md | 3,700 | Column descriptions and a short methods note. No licence statement. |
| Data_1.csv | 713,505 | Raw measurements, **specimen-level or literature rows** (not one value a species). 3,177 data rows, 420 species. |
| Data_2.csv | 166,118 | One row a species (plus a few clade labels): phylogeny sources, order, first and last appearance, trophic level, substrate (locomotor category), diet, habitat, with references. 435 rows, 432 unique names, of which 420 are species also in Data_1 and 12 are clade labels (Canidae, Felidae, Ursidae and so on) with no measurements. |
| Data_3.csv | 22,226 | Node age calibrations: 93 rows (Node, Species1, Species2, ages, source1, source2). |
| Data_4.csv | 522 | Time-bin intervals for tips: 22 Cenozoic stages, columns `stage`, `max.age`, `min.age`. |
| Data_5.csv | 583 | Time-bin intervals for tips plus nodes: 25 stages (adds Late Cretaceous / Danian-Selandian-Thanetian). Same columns. |
| Data_6.zip | 25,237,827 | Disparity plots from swapped-tree sensitivity runs: 50 TIFF files in a `Simulations/` folder. Not raw measurements. |

### Data_1.csv columns and measurements

Columns, in file order: `Species`, `Locality`, `Cat` (museum catalogue number), `Humerus (mm)`, `Radius (mm)`, `Femur  (mm)`, `Tibia  (mm)`, `Brachial index (R/H)`, `Crural index (T/F)`, `Greatest skull length (mm)`, `Note`, `Source`.

Units for the five length columns are millimetres, as the headers say. The README describes them as humerus, radius, femur and tibia length, and "the greatest skull length". It does not give landmarks beyond those names, and it does not say whether a bone was measured dry or from a published table. Rows are a compilation from the literature and published databases; 3,160 of 3,177 rows have a `Source` citation.

Non-empty numeric cells in Data_1.csv (every value parsed as a float; no unparseable strings):

| Column | Values | Of which, living species (Data_2 last appearance = 0) | Species with at least one value (living) |
|---|---:|---:|---:|
| Humerus (mm) | 774 | 377 | 405 (242 living) |
| Radius (mm) | 785 | 374 | 405 (242 living) |
| Femur  (mm) | 649 | 324 | 403 (241 living) |
| Tibia  (mm) | 647 | 308 | 403 (241 living) |
| Greatest skull length (mm) | 1,870 | 1,523 | 420 (247 living) |
| Brachial index (R/H) | 20 | 9 | 17 (7 living) |
| Crural index (T/F) | 18 | 9 | 16 (7 living) |

The brachial and crural indices are almost empty; the README lists them, but they are filled on only 20 and 18 rows. They can be computed from radius/humerus and tibia/femur where both bones are present on the same row.

`Locality` is filled on 369 rows, `Cat` on 1,615, `Note` on 68. Notes are mostly synonyms, sex (`male` / `female`), or short remarks, not measurements.

**Body mass is not in the file.** There is no mass or weight column in Data_1 or Data_2. Some `Source` titles mention body mass (for example Costeur et al. 2019 on bony labyrinth, skull size and body mass in ruminants), but the mass itself was not copied into any column. No metapodials, phalanges, girths or soft-tissue measures are present.

### Species count and groups

Living versus fossil was taken from Data_2 `Last appearance (LA)`: **LA = 0** treated as living (extant). That rule gives:

- **420 species in Data_1**, of which **247 living** and **173 fossil**.
- All 247 living Data_2 species occur in Data_1. The 15 Data_2 rows with a blank LA are clade labels, not species (and not in Data_1), with Nimravidae repeated.

Living species are only three orders:

| Order | Living species (rows in Data_1) | Fossil species (rows in Data_1) |
|---|---:|---:|
| Artiodactyla | 120 (633 rows) | 59 (353 rows) |
| Carnivora | 115 (1,259 rows) | 53 (480 rows) |
| Perissodactyla | 12 (85 rows) | 41 (270 rows) |
| Creodonta | 0 | 9 (34 rows) |
| Cimolesta | 0 | 4 (15 rows) |
| Condylarthra | 0 | 3 (37 rows) |
| Procreodi | 0 | 2 (4 rows) |
| Acreodi | 0 | 1 (3 rows) |
| Dinocerata | 0 | 1 (4 rows) |

There are **no living or fossil Proboscidea, Lagomorpha or Rodentia** in the file, so elephants, rabbits and rats cannot be in it.

The paper and the Dryad methods text (quoted in R7-00) said **246 living and 177 fossil species**, and **793 / 805 / 668 / 663** humerus, radius, femur and tibia lengths plus **1,883** skull lengths. The file itself has 247 / 173 species and 774 / 785 / 649 / 647 / 1,870 values. See "Sources disagree".

---

## (h) Quadruped-set values

**Asked for:** dog (including breeds if present), grey wolf, domestic cat, lion, tiger, leopard, horse, red or white-tailed deer, cattle, pig, brown or black bear, African or Asian elephant, rabbit, brown rat. For each species held: humerus, radius, femur, tibia and greatest skull length in mm, with n, mean and spread, and body mass if given, naming the file and column.

### How the numbers were made

All figures below are **computed** from the numeric cells in **Data_1.csv**, using the column names above. Each non-empty cell for that species and column is one observation. Sample standard deviation (n-1) is given when n is at least 2, plus the range (minimum to maximum). When n = 1, the single file value is given and there is no SD. Means and SDs are rounded to two decimal places; ranges keep the file's own precision. No body mass is given for any species.

Some rows have a museum catalogue number (`Cat`); some are literature values with no catalogue number and may already be published means or single cited figures. Pleistocene or fossil-site rows are included when the file lists them under a living species name. Both points affect the means; they are flagged under the species notes and in "Sources disagree". Exact duplicate rows were checked for the target species: none of the values below is an exact duplicate of another row's same-bone value.

### Found and missing

| Requested | In Data_1? | File name used |
|---|---|---|
| Dog, including breeds | **No.** No *Canis familiaris*, no breed names in Species, Locality, Cat or Note. | - |
| Grey wolf | **Yes**, *Canis lupus* (`Canis_lupus`). Localities and sources are wolf (Minnesota, Algonquin Park, a gray-wolf skull page, and so on), not dog. | Canis_lupus |
| Domestic cat | **Yes**, *Felis catus* (`Felis_catus`). *Felis silvestris* and *F. s. lybica* are also in the file but were not pooled. | Felis_catus |
| Lion | **Yes**, *Panthera leo* (`Panthera_leo`) | Panthera_leo |
| Tiger | **Yes**, *Panthera tigris* (`Panthera_tigris`) | Panthera_tigris |
| Leopard | **Yes**, *Panthera pardus* (`Panthera_pardus`) | Panthera_pardus |
| Horse, with breed | **Yes**, domestic horse as `Equus_ferus_caballus`. **No breed is given** on any row. | Equus_ferus_caballus |
| Red deer or white-tailed deer | **Yes, both**: `Cervus_elaphus` and `Odocoileus_virginianus` | those two names |
| Cattle | **Yes**, `Bos_taurus`. No breed. | Bos_taurus |
| Pig (adult, domestic if distinguished) | **Yes**, `Sus_scrofa` only. The file does not mark domestic versus wild; several skull sources are wild boar. | Sus_scrofa |
| Brown or black bear | **Yes, both**: `Ursus_arctos` and `Ursus_americanus` | those two names |
| African or Asian elephant | **No** | - |
| Rabbit | **No** | - |
| Brown rat | **No** | - |

*Equus przewalskii* is also in the file (related horse, not the requested domestic horse). A short extra table is at the end of this section.

### Values (computed from Data_1.csv)

Q = DS1(h). SD is the sample standard deviation. "n=1, file value" means a single cell, not a computed mean.

| Q | Species (file name) | Column | n | Mean (mm) | Spread | Body mass |
|---|---|---|---:|---:|---|---|
| DS1(h) | Grey wolf (*Canis lupus*, `Canis_lupus`) | Humerus (mm) | 7 | 220.57 | SD 9.29; range 205.7-229.9 | not given |
| DS1(h) | Grey wolf | Radius (mm) | 7 | 223.23 | SD 10.66; range 205.7-234.0 | not given |
| DS1(h) | Grey wolf | Femur  (mm) | 9 | 238.89 | SD 12.58; range 218.2-252.2 | not given |
| DS1(h) | Grey wolf | Tibia  (mm) | 5 | 200.96 | SD 41.16; range 152.9-235.1 | not given |
| DS1(h) | Grey wolf | Greatest skull length (mm) | 37 | 252.11 | SD 23.92; range 198.859-319.168 | not given |
| DS1(h) | Domestic cat (*Felis catus*, `Felis_catus`) | Humerus (mm) | 3 | 97.57 | SD 1.53; range 96.4-99.3 | not given |
| DS1(h) | Domestic cat | Radius (mm) | 3 | 92.83 | SD 1.59; range 91.0-93.8 | not given |
| DS1(h) | Domestic cat | Femur  (mm) | 2 | 106.65 | SD 5.16; range 103.0-110.3 | not given |
| DS1(h) | Domestic cat | Tibia  (mm) | 1 | 111.6 | n=1, file value (no SD) | not given |
| DS1(h) | Domestic cat | Greatest skull length (mm) | 1 | 93.3 | n=1, file value (no SD); source is a skullbase.info page | not given |
| DS1(h) | Lion (*Panthera leo*, `Panthera_leo`) | Humerus (mm) | 6 | 298.57 | SD 31.25; range 264.8-347.0 | not given |
| DS1(h) | Lion | Radius (mm) | 6 | 270.90 | SD 24.32; range 250.0-310.5 | not given |
| DS1(h) | Lion | Femur  (mm) | 6 | 298.33 | SD 31.97; range 262.6-347.1 | not given |
| DS1(h) | Lion | Tibia  (mm) | 2 | 295.40 | SD 9.05; range 289.0-301.8 | not given |
| DS1(h) | Lion | Greatest skull length (mm) | 17 | 327.91 | SD 35.64; range 278.68-379.0 | not given |
| DS1(h) | Tiger (*Panthera tigris*, `Panthera_tigris`) | Humerus (mm) | 8 | 309.43 | SD 23.36; range 267.0-343.0 | not given |
| DS1(h) | Tiger | Radius (mm) | 8 | 260.81 | SD 19.65; range 235.0-290.6 | not given |
| DS1(h) | Tiger | Femur  (mm) | 8 | 308.97 | SD 27.24; range 275.8-364.773 | not given |
| DS1(h) | Tiger | Tibia  (mm) | 2 | 306.30 | SD 3.25; range 304.0-308.598 | not given |
| DS1(h) | Tiger | Greatest skull length (mm) | 27 | 317.55 | SD 29.87; range 250.0-357.0 | not given |
| DS1(h) | Leopard (*Panthera pardus*, `Panthera_pardus`) | Humerus (mm) | 1 | 211.2 | n=1, file value (no SD) | not given |
| DS1(h) | Leopard | Radius (mm) | 1 | 263.6 | n=1, file value (no SD) | not given |
| DS1(h) | Leopard | Femur  (mm) | 2 | 293.20 | SD 85.14; range 233.0-353.4 | not given |
| DS1(h) | Leopard | Tibia  (mm) | 2 | 262.65 | SD 56.07; range 223.0-302.3 | not given |
| DS1(h) | Leopard | Greatest skull length (mm) | 22 | 208.66 | SD 24.53; range 165.04-245.0 | not given |
| DS1(h) | Horse (*Equus ferus caballus*, `Equus_ferus_caballus`); no breed | Humerus (mm) | 2 | 318.25 | SD 22.98; range 302.0-334.5 | not given |
| DS1(h) | Horse | Radius (mm) | 2 | 370.00 | SD 33.94; range 346.0-394.0 | not given |
| DS1(h) | Horse | Femur  (mm) | 2 | 429.75 | SD 23.69; range 413.0-446.5 | not given |
| DS1(h) | Horse | Tibia  (mm) | 2 | 380.00 | SD 33.94; range 356.0-404.0 | not given |
| DS1(h) | Horse | Greatest skull length (mm) | 3 | 546.10 | SD 24.75; range 524.3-573.0 | not given |
| DS1(h) | Red deer (*Cervus elaphus*, `Cervus_elaphus`) | Humerus (mm) | 1 | 285.0 | n=1, file value (no SD) | not given |
| DS1(h) | Red deer | Radius (mm) | 1 | 298.9 | n=1, file value (no SD) | not given |
| DS1(h) | Red deer | Femur  (mm) | 1 | 338.7 | n=1, file value (no SD) | not given |
| DS1(h) | Red deer | Tibia  (mm) | 1 | 384.3 | n=1, file value (no SD) | not given |
| DS1(h) | Red deer | Greatest skull length (mm) | 7 | 331.14 | SD 22.77; range 290.0-357.0 | not given |
| DS1(h) | White-tailed deer (*Odocoileus virginianus*, `Odocoileus_virginianus`) | Humerus (mm) | 2 | 202.25 | SD 18.03; range 189.5-215.0 | not given |
| DS1(h) | White-tailed deer | Radius (mm) | 2 | 216.40 | SD 31.96; range 193.8-239.0 | not given |
| DS1(h) | White-tailed deer | Femur  (mm) | 1 | 230.5 | n=1, file value (no SD) | not given |
| DS1(h) | White-tailed deer | Tibia  (mm) | 1 | 254.5 | n=1, file value (no SD) | not given |
| DS1(h) | White-tailed deer | Greatest skull length (mm) | 5 | 254.50 | SD 30.31; range 209.0-291.0 | not given |
| DS1(h) | Cattle (*Bos taurus*, `Bos_taurus`); no breed | Humerus (mm) | 1 | 262.7 | n=1, file value (no SD) | not given |
| DS1(h) | Cattle | Radius (mm) | 1 | 254.2 | n=1, file value (no SD) | not given |
| DS1(h) | Cattle | Femur  (mm) | 1 | 335.2 | n=1, file value (no SD) | not given |
| DS1(h) | Cattle | Tibia  (mm) | 1 | 312.8 | n=1, file value (no SD) | not given |
| DS1(h) | Cattle | Greatest skull length (mm) | 2 | 512.26 | SD 24.41; range 495.0-529.52 | not given |
| DS1(h) | Pig / wild boar (*Sus scrofa*, `Sus_scrofa`); not labelled domestic | Humerus (mm) | 1 | 213.0 | n=1, file value (no SD) | not given |
| DS1(h) | Pig / wild boar | Radius (mm) | 1 | 161.7 | n=1, file value (no SD) | not given |
| DS1(h) | Pig / wild boar | Femur  (mm) | 1 | 239.3 | n=1, file value (no SD) | not given |
| DS1(h) | Pig / wild boar | Tibia  (mm) | 1 | 212.7 | n=1, file value (no SD) | not given |
| DS1(h) | Pig / wild boar | Greatest skull length (mm) | 6 | 373.57 | SD 46.96; range 309.0-424.0 | not given |
| DS1(h) | Brown bear (*Ursus arctos*, `Ursus_arctos`) | Humerus (mm) | 9 | 343.17 | SD 52.75; range 272.8-444.0 | not given |
| DS1(h) | Brown bear | Radius (mm) | 9 | 293.23 | SD 34.89; range 243.3-348.5 | not given |
| DS1(h) | Brown bear | Femur  (mm) | 5 | 371.62 | SD 39.82; range 315.8-414.0 | not given |
| DS1(h) | Brown bear | Tibia  (mm) | 4 | 275.60 | SD 25.08; range 239.0-295.9 | not given |
| DS1(h) | Brown bear | Greatest skull length (mm) | 9 | 397.38 | SD 69.56; range 339.13-565.6 | not given |
| DS1(h) | American black bear (*Ursus americanus*, `Ursus_americanus`) | Humerus (mm) | 2 | 279.35 | SD 13.65; range 269.7-289.0 | not given |
| DS1(h) | American black bear | Radius (mm) | 2 | 244.20 | SD 31.25; range 222.1-266.3 | not given |
| DS1(h) | American black bear | Femur  (mm) | 3 | 295.87 | SD 56.65; range 232.9-342.7 | not given |
| DS1(h) | American black bear | Tibia  (mm) | 3 | 243.97 | SD 13.94; range 228.0-253.7 | not given |
| DS1(h) | American black bear | Greatest skull length (mm) | 7 | 262.75 | SD 26.00; range 235.83-311.1 | not given |

Limb values for cattle, pig, red deer, and the single leopard humerus and radius, each come from one literature row (often Christiansen 1999, *Journal of Zoology* 247: 333-348, or, for the smaller leopard femur and tibia, a row sourced only as "Fossilwork"). Treat those as single cited figures, not sample means.

### Species notes (read before using the means)

- **Grey wolf tibia:** two of the five values (152.9 and 159.3 mm) are much smaller than the other three (225.5, 232.0, 235.1 mm). They come from Salari et al. 2017, Pleistocene *Canis lupus* at Grotta Mora Cavorso, Italy. In those two rows the `Cat` field holds the site name and `Locality` holds the specimen codes (GMC 2823, GMC 2744), so the fields look swapped. A further femur (226.3 mm, Piculjan 2012) is also Upper Pleistocene (Hijenska Pecina, Croatia). Including the two short tibiae pulls the mean down and inflates the SD. If those two are set aside, the other three tibiae give n=3, mean 230.87 mm, SD 4.90 mm, range 225.5-235.1 (computed). Skull n=37 is the strongest wolf sample in the file.
- **Leopard limbs should not be used as a species mean.** One row (Christiansen 1999) gives humerus 211.2, radius 263.6, femur 353.4, tibia 302.3 mm. Radius longer than humerus, and a 353.4 mm femur, sit in lion or tiger territory, not with the other leopard row (Fossilwork: femur 233.0, tibia 223.0 mm) or with the leopard skulls (mean 208.66 mm, n=22). The femur and tibia "means" are the midpoints of two incompatible figures (SD 85.14 and 56.07 mm). Report the two rows separately, or drop the Christiansen limb row, until the original paper is checked.
- **Brown bear rows may mix in other bears.** Three humerus and radius rows are cited to Ryziewicz 1957, a paper on cave bear (*Ursus spelaeus*). Two further limb rows are cited to Zapfe 1946 on early Pleistocene bears from Hundsheim. One skull value is 565.6 mm (Green et al. 2012), far above the other eight (339.13 to 416.3 mm; the next highest, 416.3 mm, is from the same Green et al. paper). That single cell drives much of the skull SD (69.56 mm). Another skull row (339.13 mm, Santos et al. 2017) is cited to a paper on the *Ursus deningeri*-*U. spelaeus* lineage, so even the lower end of the skull sample may not be modern brown bear. The table above uses every row the file labels `Ursus_arctos`. A user who wants only rows whose sources are not obviously about other fossil bears should recompute after reading those papers.
- **Pig skulls** in the file are largely wild boar (Choudhary et al. 2017, "Indian wild pig"; Genov et al. 1991, Bulgarian wild boar). The single limb row (Christiansen 1999) does not say domestic or wild.
- **Horse:** no breed. Two of the three skull values (541.0 and 573.0 mm) come from the same Mammalian Species account (Bennett and Hoffmann 1999) with no catalogue numbers; they may be published figures rather than two measured specimens.
- **Domestic cat** limb and skull samples are small (tibia and skull are n=1). The skull value is from http://skullbase.info/skulls/mammals/domestic_cat.php, not a journal table.

### Also in the file, not requested: Przewalski's horse (`Equus_przewalskii`)

| Column | n | Mean (mm) | Spread | Body mass |
|---|---:|---:|---|---|
| Humerus (mm) | 4 | 259.85 | SD 10.92; range 245.0-271.0 | not given |
| Radius (mm) | 4 | 306.02 | SD 8.78; range 293.0-312.1 | not given |
| Femur  (mm) | 1 | 326.0 | n=1, file value (no SD) | not given |
| Tibia  (mm) | 1 | 309.0 | n=1, file value (no SD) | not given |
| Greatest skull length (mm) | 9 | 525.22 | SD 21.12; range 485.0-547.0 | not given |

---

## Not found

- Licence statement in the data README (the README is silent; the Dryad record is CC0 1.0, confirmed earlier).
- Domestic dog and all dog breeds.
- African elephant, Asian elephant, rabbit, brown rat (and their orders).
- Body mass, for any species.
- Horse breed, cattle breed, and a domestic-versus-wild label for *Sus scrofa*.
- Landmarks or a protocol beyond the column names in the README.

## Sources disagree

- **Paper and Dryad methods text versus the file:** the methods text (as recorded in R7-00) gave 246 living and 177 fossil species, and 793 humerus, 805 radius, 668 femur, 663 tibia and 1,883 skull values. Data_1.csv plus Data_2 last-appearance = 0 gives **247 living and 173 fossil species**, and **774 / 785 / 649 / 647 / 1,870** values. The direction is the same; the counts are not. Possible causes include a later Dryad version, a different living/fossil rule, or subspecies names counted separately in the file (for example `Bison_bison` and `Bison_bison_americanus`, or `Felis_silvestris` and `Felis_silvestris_lybica`).
- **Leopard limb row (Christiansen 1999) versus the Fossilwork leopard row, and versus the leopard skulls:** see the species note above. The two femur values differ by 120.4 mm.
- **Brown bear source titles versus the species label:** Ryziewicz 1957 and Zapfe 1946 are papers on cave bear or early Pleistocene bears, filed here under `Ursus_arctos`. The 565.6 mm skull is an outlier in the same species.
- **Grey wolf tibiae:** Pleistocene cave values (152.9, 159.3 mm) versus the other three (225.5-235.1 mm).
- **README age-column labels:** the README says `max.age` is "minimum stage age" and `min.age` is "maximum stage age". The column names and the description contradict each other; this file does not use those columns.

## Surprises worth flagging

1. The living sample is only even-toed ungulates, carnivorans and odd-toed ungulates. That is why elephant, rabbit and rat are absent, and why a dog separate from the wolf is absent too.
2. Several requested "species means" are one literature cell, not a series of specimens.
3. The leopard Christiansen 1999 limb row, the brown bear cave-bear source titles, the 565.6 mm brown bear skull, and the two very short Pleistocene wolf tibiae are the main data-quality problems for the quadruped set.
4. Brachial and crural indices are listed in the README but are almost unused in Data_1.csv.
