# Round seven, checked here: the animal half (2026-11-24)

An independent check for the project, by a model that had no part in the research: the delivered
numbers held against the source files packed with them, with what was computed recomputed from
the raw data. The scripts and their outputs named below are in a scratch folder on the
development machine and are not in the repository; the packed sources are not either.

**How much of the delivery I opened.** I read every animal file in full (R7-00, R7-01, R7-02, R7-2A, R7-2A-Q8, R7-2B, R7-2C, R7-2D, R7-2E, R7-2F, R7-5, R7-5b, R7-6). I read in full the verify files of R7-00, R7-01, R7-2A-Q8, R7-2B, R7-2C, R7-2D, R7-2E, R7-5, R7-5b and R7-6, and part of those of R7-02, R7-2A and R7-2F.
- **Recomputed from the raw data files in the pack:**
  - DS1 (Data_1 and Data_2) and DS2, which needed a small reader of my own for the .RDS file.
  - DS3: dat.mat and Megakine.csv.
  - Humphries: supplements S1 to S5 and all 189 kinematic files from the OSF deposit (Open Science Framework).
  - Chapman's elephant spreadsheet, Shine's sea-snake spreadsheet, Marek's neck-count CSV and the Carnivora foot-posture CSV.
  - The crocodylian strides (Hutchinson), the lacertid strides (Druelle) and Dick & Clemente's lizard table.
  - Böhmer Tables S3 and S4, the Ruta vertebral counts and Baliga Data S1.
- **PDFs and saved text:** about 45 sources, each at the cited table or page.
- **Not opened:** the machine, people, plant and terrain files; the W and U rows of R7-3 and R7-4 beyond the animal rows that R7-5 and R7-6 take up; the DS1 TIFF files.

---


**In short.** The delivery is careful. Every table I recomputed from a data file reproduced to the printed precision, apart from the ten points in (a). Of those, only two change something a builder would type: the name of one sea snake and the count of lizard specimens. The verify files that give a verdict for each row are real re-checks. Seven others only summarise their checks by category. Several round-seven rows repeat what C2, C4, C6 and C7 already had (the same papers and the same numbers), and they agree.

## (a) Errors found

| # | File, row | Delivered | What the source says (my value) | Source, place |
|---|---|---|---|---|
| 1 | R7-6 row 1A, and its summary line "30 lizards" | "All 30 rows are in the file"; n = 30 | **27 specimens** in 9 species. The paper says "Table S2 ... (n = 27)" | `Q9_dick_clemente2016_varanus_FrontZool/12983_2016_141_MOESM2_ESM.txt`; `PMC4758084.1.txt` line 300 |
| 2 | R7-6 row 1A, copy-error note | The 810.7 g *V. varius* row "repeats the **second** *V. tristis* row" | It repeats the **first** *V. tristis* row (82.0, 140.0, 222.0, 417.0, 31.9, 32.3, 35.3). The second row is 73.0, 127.0, 200.0, 406.0, 23.8, 22.0, 37.2. Both *tristis* rows weigh 104.5 g. The verify file passed this row | MOESM2 table |
| 3 | R7-6 row 13 (sea snakes) | taxon "*A. eydouxii*", 62 F / 27 M adults | The spreadsheet's "eydouxii" (108 snakes) is the paper's ***Aipysurus mosaicus***. Its Table 1 reads "Aipysurus mosaicus, spine-tailed sea snake, 108, 27, 62", and *A. eydouxii* is not in the paper. R7-2E already calls this sample *A. mosaicus*. The numbers are right; relabel the rows | `A7_shine_PMC10716647.txt` line 75 |
| 4 | R7-6 row 15 | CarnData: 101 Carnivora | **102 rows**: 62 coded 0 (plantigrade), 40 coded 1 (digitigrade) | `CarnData.csv` (`r76_out.txt`) |
| 5 | R7-5 X13 and the README | Humphries deposit: "98 markers" | Confirmed: 150 Hz, 189 files, 23 dogs. But the **marker count varies from 30 to 98** between files. Every Labrador file has 30 to 32 markers; the German shepherds have 50 to 98. **Each "Stand" trial holds only 1 or 2 frames.** The paper had 12 Labradors; the pack holds 11 (LRD 09 and 12 are missing) | `humphries_osf_out.txt` |
| 6 | R7-2D Q20 | Henderson 2010: "up to 13 m span and 544 kg" | Henderson's own model uses an **11.2 m** span (Table 1, p. 771). The "13 m" is Witton & Habib's general sentence about giant pterosaurs. R7-5b has it right | `henderson.txt` lines 208 and 776; `work/s2/witton.txt` |
| 7 | R7-5 X10 (Chapman, Asian elephants) | Males 78 individuals, 3037 ± 469 kg; file has 399 IDs, 178 of them coded 2 | I find **400 IDs (221 coded 1, 179 coded 2)** and **79 adult males** at 3038.5 ± 465.9 kg (78 at 3034.9 ± 467.0 if a height is required on the same row). Females reproduce exactly: 122, 2474.7 ± 281.8. The verify file's "identical" does not hold for the males; the difference is about 3 kg | `chapman_disanto_out.txt`, `chapman_variants_out.txt` |
| 8 | README, key correction 12 | The 0.08 kg glaucous-winged gull is "in DS2's source table" | It is in **Böhmer et al. 2019 Table S4**, not DS2 (Skelevision has no masses) | `bohmer_supp.txt` |
| 9 | R7-5 X12 table, column "Central cervical vertebra" | 12, 12, 17 … 59, no unit | These are the **central vertebra's length in mm**, not a count. The pigeon's 12 mm equals Marek's count of 12 for the pigeon, so a builder could type it in as a count | Böhmer Table S4 |
| 10 | R7-5 X9 (Schopf, elephant skulls) | "Kami, male adult" | Tables 3 and 4 say "♂ ad.", but the specimen list gives Kami's sex as **"unbestimmt"** (undetermined), so it should be flagged. Also, Schopf's "Jumbo" is a 46-year-old female in Vienna, not Larramendi's male Jumbo (R7-02). Keep the two apart | `A39.txt` lines 150–215 and 994 |

**Smaller points:**
- **Labrador stifle angle.** R7-2A explains 31.5° (Results) against 34° (abstract) as "side or rounding". But Table S4, which is in the pack, gives left 34.87° and right 32.50°, and 31.5° matches neither side.
- **DS1 quality problems R7-01 did not flag:**
  - The wolf row from Christiansen 1999 has humerus = radius = 205.700 mm, probably a copy error in the compilation.
  - One of the three horse skulls (524.3 mm) comes from an archaeological dig.
  - One brown-bear skull (401.68 mm) comes from a paper by cave-bear researchers.

  R7-01's own flags all hold: the Pleistocene wolf tibiae with the site and specimen fields swapped, the two leopard rows 120.4 mm apart, the cave-bear papers filed under the brown bear, and the 565.6 mm bear skull.

## (b) Confirmed against the packed source

About **470 numbers recomputed from data files** reproduced, apart from the items in (a). About **260 more numbers read in PDFs or saved text** were found as delivered.

| File, row | Value | Source in the pack | Where | How |
|---|---|---|---|---|
| R7-01 (f) | 3,177 rows; 420 species (247 living, 173 fossil); values per bone 774 / 785 / 649 / 647 / 1,870 (living 377 / 374 / 324 / 308 / 1,523); species per bone 405 / 405 / 403 / 403 / 420; filled-field counts; table by order | Data_1.csv, Data_2.csv | whole files | recomputed |
| R7-01 (h) | All 65 cells (n, mean, SD, range) for wolf, cat, lion, tiger, leopard, horse, red deer, white-tailed deer, cattle, pig, brown bear, black bear and Przewalski's horse | Data_1.csv | per species | recomputed |
| R7-01 notes | Wolf tibiae; leopard rows; bear source rows; skull extremes | Data_1.csv | rows | recomputed |
| R7-00 DS2 | All 56 cells for the 7 passerines. `tarsus` is the tibiotarsus and `metatarsus` the tarsometatarsus, judging by the values | the .RDS file, `filtered_dataset` | — | recomputed |
| R7-00 DS3 | 44 species, 151 clips; all amplitude cells and head:tail ratios | `dat.mat` | — | recomputed |
| R7-5 appendix (Di Santo) | Wavelength, tail-beat frequency, tail amplitude, speed (UsBL) and length for all 44 species; clip range 0.51–1.49, mean 0.93 ± 0.21 BL | Megakine.csv | — | recomputed |
| R7-02, R7-5 X13 (Humphries) | Table S5 (all rows); S3 stance 43.22 / 43.71 %; main-text bone shares, masses and angles | supplements s001–s005; `work/src/dogstand.txt` | — | read |
| R7-2A Q1–Q5: dogs, wolf, cat, horse | Labrador (Thuller), wolf (Ahmed), cat (Borges) and horse (Padilha) | `lab.txt`, `wolfbg_pdf.txt`, `cat_full.txt`, `bsh.txt` | tables | read |
| R7-2A Q1–Q5: cattle, sows, elephant girth | Cattle (Saha, Foksha), sows (Endrésen), Magro (now confirmed, closing R7-2A's "UNVERIFIABLE"), Chapman girth equations | `saha.txt`, `holro.txt`, `sows.txt`, A32, A38 | tables | read |
| R7-2A Q2 | Vertebral counts for all 18 species | `ruta.xlsx` | — | recomputed |
| R7-2A-Q8 | Robilliard Tables 2–4 | `robilliard.txt` (the same values are in C2 §1c) | — | read |
| R7-2B | Varanus Table 1 (Christian & Garland), tortoises (Serbia and Albania), alligator methods | `varanus.txt`, `testudo_*.txt`, `blob03_readkong.txt` | — | read |
| R7-6 1A | 5 quoted specimen rows; scaling slopes | MOESM1, MOESM2 | — | read |
| R7-6 1B | Lacertid fits: *P. muralis* DF % = 70.76 − 15.14 v (R² 0.15), stride frequency = 3.55 + 7.03 v (R² 0.55); *A. boskianus* DF flat (R² 0.00), stride frequency = 3.65 + 4.69 v (R² 0.65) | MOESM2 CSV | — | recomputed |
| R7-6 2 | Crocodylians: Alligatoroidea DF = 0.688 − 0.082 v (R² 0.69, n 36); individual AMA strides; Tables 1 and 2 | MOESM13 `dataset1_all`; paper text | — | recomputed and read. Table 2's 0.42 is the Crocodyloidea *asymmetrical* group, which R7-6's label leaves out |
| R7-6 13 | All 12 species-by-sex rows of the sea-snake table | Shine spreadsheet | — | recomputed. On units: the spreadsheet's README says cm, but the values only make sense in mm, and the paper states no unit. R7-6's inference holds |
| R7-6 16 | Neck counts: pigeon 12, pelican 17, *Anser fabalis* 19 (maximum), minimum 11, median 13; 112 species | `svp2022data_v2.csv` | — | recomputed |
| R7-2C | 13 back-transformed species (log10 of metres); Table S3; swan 23; Romer counting convention | Böhmer docx and text | — | recomputed and read |
| R7-2C, R7-6 5 | Hummingbird downstroke 48 % to 53 %; dove downstroke 61.80 ms, upstroke 40.65 ms, 9.80 Hz | `tob07.txt`; PMC10942624 | — | read |
| R7-5 X15 | Baliga goshawk wing bones | Data S1 | — | read. The same row gives the goshawk 250.9 g, implausibly low: check Baliga's masses before use |
| R7-2D, R7-5 | Castillo-Figueroa Table 1 rows; Gardiner Table 1; ptarmigan; Witton & Habib | A27, A30, `PMC3988792.txt`, `witton.txt` | — | read |
| R7-5b | Henderson Tables 1–3 (544 kg, 11.2 m, 0.751 m³, 724 kg/m³); Cruse Table 3 | the browser-downloaded PDFs (both have a text layer) | — | read |
| R7-2E | Snake vertebral counts (Szyndlar); trout fin positions | `vzatlas_full.txt`, `trout_tr.txt` | — | read |
| R7-2F, R7-6 3 | Tarantula Table 1. The "tarsus" cells sit in the mass column, so the U1 correction is right. Cupiennius Tables 1 and 2; R7-2F's computed duty factors and frequencies; *G. rosea* SDs | PMC6766366, Weihmann text | — | read and recomputed |
| R7-6 4, 6 | Parle cockroach tibia (Table 2); Nauphoeta speeds | PMC4972353, PMC5719566 | — | read |
| R7-5 U5, U6, U8; Q3, Q4, Q9 | Biancardi, Bender, McElroy; Volf rabbits and hares; Schopf skulls (female mean 83.4 cm); Herrel lizards (the "weak OCR" warning stands) | A3, A12, A13, A36, A39, A34 | — | read |

**What the Humphries trials add.** This is my own computation, so use it as `own`, not `source`. Taking the withers marker's height above the lowest paw marker in one standing frame per dog gives:
- German shepherds: **654 ± 32 mm (n 12)**
- Labradors: **549 ± 42 mm (n 10)**, which agrees with Thuller's live 51–55 cm.

Bone shares measured between markers in the same frames come within 1–2 points of Table S5. So the deposit can turn S5's shares into approximate millimetres.

## (c) Not checkable here (source not in the pack, or only an abstract was read)

| File | Rows | Why |
|---|---|---|
| R7-02 | Singh & Kumar 2023 femur (all six rows) | PDF not in the pack |
| R7-02 | Larramendi 2016 elephant bones | not in the pack (C1 already has the same rows) |
| R7-02 | Gurbuz 2015 rabbit skull | not in the pack |
| R7-02 | Islam 2021 rat | not in the pack |
| R7-02 | All 15 PanTHERIA masses | PanTHERIA file not in the pack (the same values are in C1) |
| R7-2A | Amur tiger figures; Jaegger 2002 and Jaeger 2007 joint ranges | forum page or blocked |
| R7-2A-Q8 | Wetzel 1977 cat gallop; 2026 J Neurophysiol cat duty factors | search extracts only |
| R7-2B | Reilly & Elias 1998 duty factor and femoral adduction | abstract only |
| R7-2B | Hagey S1 Table values | files in the pack but binary; I did not parse them |
| R7-2D | Schutt 1997 and Earls 2000 values | abstracts (`abs_*.txt`), not opened by me |
| R7-2D | Hopper 2013 and Studier 1972 membrane values | secondary citations in a thesis |
| R7-5 X5 | Fieler & Jayne 1998, Full & Tu 1991, Kram 1997, Tobalske 1999, Tobalske & Dial 1996 | read online as abstracts, not saved |
| R7-5 X13 | 113 kinetic CSV files | not in the pack |
| R7-6 | Kubo's 880-mammal list | blocked: the `kubo_sd0*.xlsx` files in work/w3b are error pages, not spreadsheets |
| R7-6 | Harvey 2019 and 2022 wing ranges; Dial 1992 and the dove thesis | blocked |

## (d) The verify files

**Real row-by-row re-checks:** R7-00, R7-01 (84 items), R7-02 (49), R7-2A (115), R7-2F (20) and R7-6 (63).

**Summaries by category only, with no row-by-row verdicts:** R7-2A-Q8, R7-2B, R7-2C, R7-2D, R7-2E, R7-5 and R7-5b. The README's claim that every file has a check "of each row" is not true of these seven. They do describe a number-matching script plus hand checks, and each lists the errors it fixed.

**Twelve VERIFIED rows re-checked by me:**
- **Hold (8):**
  - R7-01: wolf tibia; brown-bear skull; the swapped fields on the cave-wolf rows.
  - R7-00: DS2, 56 cells; DS3, 132 cells.
  - R7-02: Table S5.
  - R7-6: Komodo row; scaling slopes; crocodylian fit.
- **Do not fully hold (4):**
  - R7-6, the *V. varius* duplicate: the duplicate is real, but the verify missed the wrong *tristis* row and the "30 rows".
  - R7-6, the Shine rows: checked against the researcher's own computed CSV, not the spreadsheet, so the check is circular. The species name passed.
  - R7-5, Chapman: the males do not reproduce exactly.

## (e) Contradictions with earlier rounds and EXEMPLARIA.md

**Earlier rows that round seven settles:**
- **C7 1.18 (tarantula tarsus, "garbled"):** those cells are masses; no tarsus length exists in the paper. Drop the row; tarsus length is `own`.
- **C7 1.7 (Cruse 1976, unverified):** confirmed. R7-5b adds the middle and hind legs and the 820 mg body weight.
- **C7 3.14 (Biancardi 2011, unverified):** confirmed.
- **C7 line 139 ("no open numbers for Cupiennius"):** round seven gives Weihmann's regressions and stride frequencies.
- **C6 2.13 / 2.14 (Reilly & Elias 1998):** still unverified.
- **C4 (*E. fuscus*):** already corrected in C4; R7-2D agrees.
- **C5 (milk snake 0.22):** C5 was already right. The README's "correction" fixes the researcher's own earlier extract, not C5.
- **EXEMPLARIA's "not delivered" notes:** update them for birds (leg bones, neck counts, downstroke), the chain family (Di Santo's per-species values) and quadrupeds (girths, skulls, some neck and tail lengths).

**Contradictions, and which to prefer:**

| Quantity | Round seven | Earlier | Prefer |
|---|---|---|---|
| Bird cervical vertebrae | Marek 11–19 (that file's maximum, not birds'); Böhmer 10–23; literature up to 26 | EXEMPLARIA bird skeleton: "neck chain (8 to 14)" | If 8 to 14 means vertebrae, it is wrong for long-necked birds: use the counts. If it means the rig's resolution, say so in EXEMPLARIA |
| Walk footfall phase | R7-2A-Q8: idealised LF 0.25 after LH (from a review) | C2: measured dog LF 0.16, RH 0.50, RF 0.67; elephant LF 0.20 | C2's measured phases |
| Monitor-lizard legs | Dick & Clemente: Komodo crus/thigh 1.37 | Christian & Garland: 0.99 | Christian & Garland, whose segment definitions are stated. Don't mix the two |
| Alligator sprawl adduction | Reilly & Blob 2003: as little as 20°; individuals 19–55° | C6 (Grok lead): about 40° | Reilly & Blob 2003 |
| Vampire-bat jump force | Peak 9.5 × body weight (abstract) | Typical 6.51 × body weight (Gardiner citing Schutt) | Both: 9.5 as the peak, 6.51 as typical |
| Henderson's pterosaur span | R7-5b: 11.2 m (R7-2D's 13 m is wrong) | — | 11.2 m; use C4's Witton scaling for the engine |
| Dog humerus share of withers height | Humphries (skin markers): GSD 26 %, LRD 30.5 % | C1 bone regression about 30 %; EXEMPLARIA's example 0.307 (wolf, Gatta) | C1 for bone lengths; Humphries for marker lengths and breed contrast |
| Stick-insect front femur | Cruse 14.7 (n 20) | C7: Theunissen 15.84 (10 females) | Theunissen for females; Cruse for all three legs |
| Horse, pig, brown bear vertebral counts | Ruta: horse 19 thoracic / 5 lumbar, pig 13 thoracic, brown bear 15 / 4 | usual veterinary counts differ (R7-2A says so) | Horse: Ruta's *E. ferus* 18 / 6. Pig and brown bear: `own`, with a range |

## (f) What it means for the builders

`source` means good enough to type in. Thin means one specimen, one species, an abstract or a substitute. `own` means the engine's estimate, with its anchor.

**Quadruped mammals (cat to elephant)**
- **Limb bones:**
  - `source`: C1 first (horse, cat, rabbit, rat, wolf); DS1 (R7-01 table) for wolf, lion, tiger and black bear.
  - Thin: cattle, pig and red deer (one row each), white-tailed deer, DS1 horse (n 2), elephant (2 skeletons).
  - Never use: DS1 leopard limbs; DS1 brown bear without dropping the Ryziewicz and Zapfe rows.
- **Dog bones:** Humphries Table S5 shares of withers height (n 12 a breed) are `source`. Millimetres need my withers heights, which are `own`. Small and giant breeds are `own`.
- **Skull length:**
  - `source`: DS1 wolf (n 37), lion (17), tiger (27), leopard (22), red deer (7), pig (6; wild boar); Volf rabbits; Labrador live (Thuller).
  - Thin: Schopf elephant (4 females), cattle (2), horse (3), cat (1).
- **Girth:**
  - `source`: Labrador, horse, Holstein cattle, sows, Asian elephant (Chapman data).
  - Everything else is `own`, anchored to girth ÷ withers height of the nearest build: dog 1.51–1.55, horse 1.14, cattle 1.46, pig 1.74, elephant 1.45–1.50.
- **Mass:** wolf, dogs, Asian elephant and cattle are `source`. PanTHERIA figures are thin (compiled, no spread).
- **Vertebral counts:** Ruta is `source` (one literature count per species) except horse, pig and brown bear. Sacral and caudal counts are `own`, anchored to the rabbit's 15–17 caudals (C1).
- **Neck and tail:** dog tail, wolf tail (32.9 % of body length) and horse neck (0.42 of withers) are `source`; the rest are `own` from those ratios.
- **Standing angles:** dog (Table S4) and horse (Padilha) are `source`. Joint ranges are `own`.
- **Gaits:** C2 (dog, elephant, horse) and the Humphries trot stance (43 % of stride) are `source`. Cat, bound and half-bound are `own`, from C2's Froude transitions.
- **Foot posture:** CarnData (102 Carnivora; brown bear plantigrade, wolf and lion digitigrade) is `source`.

**Sprawling reptiles**
- **Limb segments against SVL:**
  - `source`: monitors (Christian & Garland Table 1), geckos (Hagey), tortoises (Djordjević Table 1a).
  - Herrel's phrynosomatids are weak (OCR).
  - Skinks, iguanas and anoles are `own`, from the monitors' and geckos' ratios.
- **Tail length:** thin.
- **Duty factor against speed:** crocodylian fits (R7-6 row 2), lacertid means (the lacertid slopes have R² of 0.00–0.15, so use the means only) and Varanus walk 69 % / run 41 % are `source`.
- **Femoral adduction:** thin; `own` between 20° and 55°.
- **Girth and vertebral counts:** `own`.

**Birds**
- **Leg bones:** DS2 (passerines) is `source`; for the raven use its medians. Böhmer's non-passerines are thin (one specimen each).
- **Wing bones:** Baliga Data S1 and C3 are `source`; check Baliga's masses first.
- **Neck:** Marek and Böhmer counts are `source` (the counting convention is unchecked). Neck lengths are thin. Per-vertebra lengths are `own`, from the central vertebra's length and the count.
- **Downstroke share:** thin (hummingbird 48–53 %, dove at take-off 60.3 %); `own` between 50 % and 60 %.
- **Girth:** `own`, from mass through Henderson's bird bulk density of 806 kg/m³.
- **Wing-fold angles:** `own`.

**Bats and winged creatures**
- **Digits against forearm:** Castillo-Figueroa Table 1 is `source`; skip *A. lituratus*'s digit III. Per-bone metacarpal and phalanx lengths are `own`.
- **Launch:** Gardiner and the ptarmigan are `source`.
- **Membrane thickness:** thin; `own` within 25–250 µm.
- **Membrane attachment and calcar:** `own`.
- **Giant flyers:** Henderson's tables, Witton & Habib's 200–250 kg and C4's Witton scaling (m = 0.551 × span^2.516) are `source` as models. A flyer above 250 kg has no source: `own`, and say so.
- **Body densities:** Henderson's regional densities are `source` as a model's assumptions.

**Snakes, eels and fish**
- **Body wave:** Di Santo's per-species values are `source` (amplitudes are peak to peak).
- **Fish depth and width along the body:** in the pack but not used by the delivery. `A9_disanto_zenodo/resultsMeansSE.csv` has depth and width at 10, 25, 50, 75, 90 and 100 % of length per species. I checked the columns, not the values.
- **Snake vertebral counts:** thin (pythons and boas only); colubrids, vipers and elapids are `own`, anchored to caudal shares of 0.13–0.24.
- **Sea-snake proportions:** Shine is `source` for sea snakes only, after relabelling *A. mosaicus*. Land snakes, eels and girth along a snake are `own`.
- **Fin positions:** thin (brown trout only).
- **Snake speed:** thin (milk snake 0.11 BL/s).

**Insects, spiders and crabs**
- **Leg segments:** Cruse's stick insect (n 20, all legs) and the Silva-Pereyra tarantula are `source`. The ant is a model and the cockroach is hind tibia only, so both are thin. No source has a tarsus length: `own`.
- **Gait against speed:** Cupiennius, Grammostola and the tarantulas are `source`; cockroaches are thin.
- **Crabs:** nothing in round seven; entirely `own`.

## (g) What I did not get to

- The Hagey files (binary).
- Reilly & Blob Table 1; Hu 2009; Hopper; the Schutt and Earls abstracts; Hellickson.
- The values in Di Santo's `resultsMeansSE.csv`.
- The animal U rows of R7-4 / R7-4b and the R7-3 extinct-animal (W) rows.
- Licences, which I took as stated.

## Scripts and outputs (`C:/lv/research7/check-animals/`)

| Script | What it recomputes | Output |
|---|---|---|
| `ds1_check.py` | DS1 counts and quadruped-set values | `ds1_out.txt` |
| `rds_ds2.py` | DS2 passerines (minimal .RDS reader) | `rds_ds2_out.txt` |
| `ds3_ampenv.py` | DS3 amplitude envelopes | `ds3_ampenv_out.txt` |
| `chapman_disanto.py` | Chapman elephants; Di Santo per-species table | `chapman_disanto_out.txt` |
| `chapman_variants.py` | selection rules tried for the male elephant row | `chapman_variants_out.txt` |
| `r76_check.py` | Shine, Marek, CarnData, crocodylian and lacertid fits | `r76_out.txt` |
| `humphries_osf.py` | dogs, trials, markers, withers heights, segment shares | `humphries_osf_out.txt` |
| `docx_text.py` | text of the Humphries and Böhmer supplements | `humphries_supp.txt`, `bohmer_supp.txt` |
| pdftotext | text of the Henderson and Cruse PDFs | `henderson.txt`, `cruse.txt` |
