# C1: Limb and body proportions of a working set of mammals

Research for item C1 of `LIBRARY_RESEARCH_BRIEF.md`. Compiled 2026-11-16. All links accessed 2026-11-16.

**How this was made.** The brief asked for live passes with the grok CLI. Three passes (carnivores, ungulates, small mammals plus allometry) were launched, and all three failed at once with `Not signed in` (`grok login --device-code` needs the owner). So no number below comes from a grok pass. Every row was read by hand from the source named in its row, using WebSearch, WebFetch and the source PDFs or full text.

- **Verified**: I read the number in that source, in the table or page given.
- **Verified (secondary)**: I read the number in a later open source that quotes the original. The original itself was not opened.
- **Unverified**: the number came from a search-engine summary and I could not open a source that confirms it. Do not use these rows until they are checked.

Search summaries made at least two errors that were caught. One labelled the horse *scapula* value as humerus. One gave "leopard humerus 570 mm", which is really the whole forelimb length of 57 cm. Treat any unchecked summary with suspicion.

## Pass 2 changes (2026-11-16)

- **Retried the two Dryad datasets.**
  - The Dryad API (`/api/v2/files/{id}/download` and `/api/v2/datasets/{doi}/download`) now answers 401 "must have current bearer token", which means a logged-in API account. The web file stream answers 403.
  - Serio et al. 2024 (dryad.8gtht76sp) has no mirror on Zenodo or figshare, and Wiley's supplement page refused the fetch. **Still not obtained.** Only its metadata (methods, licence CC0) was read.
  - The felid and canid forelimb set (Meachen-Samuels & Van Valkenburgh) **was obtained from its official Zenodo mirror** (record 4969945, CC0).
- **Found and used a second CC0 dataset**, Samuels, Meachen & Sakai 2013 (Zenodo mirror of the Dryad deposit, record 4964743). It has bone-by-bone lengths, scapula to metatarsal III, for wolf, African wildcat, lion and both bears.
- **Added §1.13** (new rows for wolf, wildcat, lion, tiger, leopard, brown bear, black bear), sources S22–S23, one new "Sources disagree" item (lion femur vs tibia) and updates to "Not found".
- **Still missing**: dog, deer and pig bone-by-bone lengths; neck and tail lengths; most skull lengths and widths. Neither CC0 file has neck, tail or skull fields.
- Raw dataset files were read in `/workspace/c1work/dryad/` (scratch, not a repo) and **deleted after use**.

Notes on units and terms:
- GL = greatest length, after von den Driesch (1976).
- SH = shoulder or withers height.
- FLL / HLL = forelimb / hindlimb length.
- SD = standard deviation. SE = standard error. s.e.m. = standard error of the mean.
- "computed here" means I derived the value from the source's own numbers; the method is given.

---

## 1. Species tables

### 1.1 Domestic dog (*Canis familiaris*)

| Quantity | Value | Units | Population | Spread | Source, location | Status |
|---|---|---|---|---|---|---|
| SH from humerus GL | SH = 3.43 × GL − 26.54 | mm | British prehistoric/early historic dogs (Harcourt's reference sample; n not given in secondary) | no spread given (in secondary) | Harcourt 1974, quoted in Crockford 1997/2017, ch. 9 p. 88 [S10] | Verified (secondary) |
| SH from radius GL | SH = 3.18 × GL + 19.51 | mm | as above | no spread given | [S10] p. 88 | Verified (secondary) |
| SH from femur GL | SH = 3.14 × GL − 12.96 | mm | as above | no spread given | [S10] p. 88 | Verified (secondary) |
| SH from tibia GL | SH = 2.92 × GL + 9.41 | mm | as above | no spread given | [S10] p. 88 | Verified (secondary) |
| SH from humerus+radius | SH = 1.65 × (H+R) − 4.32 | mm | as above | no spread given | [S10] p. 88 | Verified (secondary) |
| SH from femur+tibia | SH = 1.52 × (F+T) − 2.47 | mm | as above | no spread given | [S10] p. 88 | Verified (secondary) |
| SH from metacarpal III | SH(cm) = 0.83 × GL(mm) − 2.03 | cm, mm | Clark 1995, modern dogs (quoted) | no spread given | Clark 1995, quoted in [S10] p. 88 | Verified (secondary) |
| SH from metatarsal III | SH(cm) = 0.77 × GL(mm) − 2.26 | cm, mm | as above | no spread given | [S10] p. 88 | Verified (secondary) |
| Foot posture | digitigrade | — | Canidae | — | Kubo et al. 2019 [S20], Methods (Canidae scored from video or photos; the species table is in the SI, not opened) | Verified (category scheme); species row Unverified |

No open source found with adult dog **bone lengths by breed with SD**. See "Not found".

### 1.2 Grey wolf (*Canis lupus*)

| Quantity | Value | Units | Population | Spread | Source, location | Status |
|---|---|---|---|---|---|---|
| Humerus GL | mean 193.5 | mm | extant Apennine wolf (*C. l. italicus*), PNALM collection, Italy; n not stated in table | range 177.0–203.0 | Gatta et al. 2016, Table 3 [S9] | Verified |
| Radius GL | mean 187.6 | mm | as above | range 176.0–200.5 | [S9] Table 3 | Verified |
| Femur GLC (length from caput) | mean 208.5 | mm | as above | range 198.0–216.0 | [S9] Table 3 | Verified |
| Tibia GL | mean 211.0 | mm | as above | range 196.0–227.0 | [S9] Table 3 | Verified |
| Skull basal length (basion–prosthion), measured | 197.0, 210.0, 198.0, 201.0, 197.0 (mean 200.6, computed here) | mm | 5 PNALM skulls | range 197–210 | [S9] Table 2 | Verified |
| Withers height | mean 62.9 | cm | PNALM wolves, estimated from bones (Harcourt method) | range 57.9–67.2 | [S9] p. 948 text | Verified |
| Body length (estimated) | mean 146 | cm | PNALM wolves | range 135–155 | [S9] p. 948 | Verified |
| Live Apennine wolves (quoted) | body length 109–148; withers 49–73 | cm | wolves that died 1974–1990 (Ciucci & Boitani 2003, quoted) | range only | [S9] p. 948 | Verified (secondary) |
| Whole hindlimb length (passively flexed, hip to toe tip) | 65.7 | cm | *C. lupus*, Minnesota, n = 7, mean body mass 30 773 g | no spread given (species mean) | Kilbourne & Hoffman 2013, File S1 Table SA1 [S1] | Verified |
| Whole forelimb length (incl. scapula) | 73.2 | cm | as above | no spread given | [S1] File S1 Table SA2 | Verified |
| Body mass (species value) | 31 756.51 | g | literature compilation | no spread given | PanTHERIA, Jones et al. 2009 [S19] | Verified |
| Head–body length (species value) | 1055.00 | mm | literature compilation | no spread given | [S19] | Verified |

### 1.3 Domestic cat (*Felis catus*)

| Quantity | Value | Units | Population | Spread | Source, location | Status |
|---|---|---|---|---|---|---|
| Shoulder height (stick-measured) | F 24.68; M 27.09; all 25.98 | cm | live adult cats, Türkiye/Poland, 17 F, 20 M | SD F 2.4, M 2.21, all 2.57 | Poradowski et al. 2024, Table 1 [S8] | Verified |
| Humerus GL (radiograph) | F 9.68; M 10.39; all 10.06 | cm | as above; F n=17, M n=20 | SD 0.59 / 0.62 / 0.70 | [S8] Table 2 | Verified |
| Radius GL | F 9.32; M 10.07; all 9.74 | cm | F n=16, M n=20 | SD 0.57 / 0.55 / 0.67 | [S8] Table 2 | Verified |
| Ulna GL | F 10.94; M 11.81; all 11.43 | cm | F n=16, M n=20 | SD 0.61 / 0.56 / 0.72 | [S8] Table 2 | Verified |
| Femur GL | F 10.22; M 11.27; all 10.81 | cm | F n=16, M n=20 | SD 0.62 / 0.66 / 0.83 | [S8] Table 3 | Verified |
| Tibia GL | F 11.05; M 11.65; all 11.38 | cm | F n=16, M n=20 | SD 0.61 / 0.55 / 0.65 | [S8] Table 3 | Verified |
| Os coxae GL | F 8.06; M 8.73; all 8.43 | cm | F n=16, M n=20 | SD 0.41 / 0.35 / 0.50 | [S8] Table 3 | Verified |
| SH ÷ GL factor (all cats) | humerus 2.59, radius 2.68, ulna 2.28, femur 2.42, tibia 2.29 | ratio | as above | SD 0.24, 0.22, 0.18, 0.25, 0.22 | [S8] Tables 2–3 | Verified |
| Bone ÷ SH (= 1/factor, computed here) | humerus 0.386, radius 0.373, ulna 0.439, femur 0.413, tibia 0.437 | ratio | as above | from the factors | computed from [S8] | Verified (derived) |
| Segment % of forelimb (skin markers, video) | humerus 36.8, radius 34.2, metacarpal 14.6, phalanges 14.1 | % FLL | 4 domestic cats, mean mass 3.7 kg | ± s.e.m. 1.0 / 0.3 / 0.3 / 0.7 | Day & Jayne 2007, Table 3 [S7] | Verified |
| Segment % of hindlimb | femur 34.7, tibia 32.4, metatarsal 22.1, phalanges 10.5 | % HLL | as above | ± s.e.m. 1.0 / 1.3 / 0.3 / 0.3 | [S7] Table 3 | Verified |
| FLL / HLL / intergirdle distance | 24 / 33 / 32 | cm | as above | ± s.e.m. 1 each; ranges 22–26 / 30–35 / 26–44 | [S7] Table 1 | Verified |
| Body mass (species value) | 2884.80 | g | compilation | no spread given | [S19] | Verified |
| Foot posture | digitigrade | — | Felidae | — | [S20] Methods | Category Verified; species row Unverified |

Note: Day & Jayne took lengths from 2-D video of skin markers on live animals. These are not bone lengths. They are segment lengths between joint centres.

### 1.4 Big cats: lion, tiger, leopard (*Panthera leo, P. tigris, P. pardus*)

All rows are from Day & Jayne 2007 [S7]: live zoo or sanctuary animals, 2-D video, means ± s.e.m. (ranges in brackets).

| Quantity | Lion (n=3) | Tiger (n=5) | Leopard (n=1) | Units | Location | Status |
|---|---|---|---|---|---|---|
| Body mass | 167 ± 2 (165–170) | 169 ± 6 (155–192) | 39 | kg | Table 1 | Verified |
| Intergirdle distance (shoulder–hip joint) | 90 ± 1 (87–93) | 85 ± 1 (75–92) | 74 | cm | Table 1 | Verified |
| Forelimb length (sum of segments) | 69 ± 1 (63–76) | 68 ± 1 (62–74) | 57 | cm | Table 1 | Verified |
| Hindlimb length | 82 ± 1 (71–86) | 81 ± 1 (72–92) | 76 | cm | Table 1 | Verified |
| HLL/FLL | 1.19 | 1.19 | 1.33 | ratio | Table 1 | Verified |
| Humerus, % FLL | 37.4 ± 0.5 | 36.4 ± 0.5 | 34.4 | % | Table 3 | Verified |
| Radius, % FLL | 34.9 ± 1.1 | 33.6 ± 1.6 | 37.6 | % | Table 3 | Verified |
| Metacarpal, % FLL | 15.0 ± 0.4 | 16.0 ± 1.3 | 16.0 | % | Table 3 | Verified |
| Fore phalanges, % FLL | 12.8 ± 0.6 | 13.8 ± 0.8 | 12.0 | % | Table 3 | Verified |
| Femur, % HLL | 36.4 ± 0.9 | 34.2 ± 0.8 | 31.5 | % | Table 3 | Verified |
| Tibia, % HLL | 37.3 ± 0.6 | 38.2 ± 0.5 | 37.7 | % | Table 3 | Verified |
| Metatarsal, % HLL | 16.7 ± 0.4 | 17.3 ± 0.4 | 19.4 | % | Table 3 | Verified |
| Hind phalanges, % HLL | 9.7 ± 0.3 | 9.9 ± 0.7 | 11.4 | % | Table 3 | Verified |
| Head–body length (PanTHERIA species value) | 1835.16 | 1824.79 | 1377.71 | mm | [S19], no spread | Verified |
| Body mass (PanTHERIA) | 158 623.93 | 161 914.66 | 52 399.99 | g | [S19], no spread | Verified |

Unverified leads (not opened):
- Royal Bengal tiger scapula, humerus, radius and ulna gross morphometry: Bangladesh J. Vet. Anim. Sci. 10(1), https://doi.org/10.60015/bjvas/v10i1a4.
- A search summary gave lion and tiger humerus and femur articular lengths (e.g. tiger humerus 335.5 mm, femur 390.2 mm), credited to "a 2016 osteometry study" behind a ResearchGate login. **Unverified; do not use.**

### 1.5 Horse (*Equus caballus*)

| Quantity | Value | Units | Population | Spread | Source, location | Status |
|---|---|---|---|---|---|---|
| Scapula length (E) | 34.88 | cm | 194 live adult horses (>2.5 y), mixed breeds, sexes and ages, Uruguay; tape between palpable bony landmarks, left side | SD 4.33; range 26–52 | Möller 2017, Tabla 1, p. 43 [S11] | Verified |
| Humerus (H) | 30.85 | cm | as above | SD 2.81; range 26–40 | [S11] Tabla 1 | Verified |
| Radius (R) | 37.24 | cm | as above | SD 3.59; range 32–49 | [S11] Tabla 1 | Verified |
| Metacarpal (MC) | 31.03 | cm | as above | SD 1.57; range 28–38 | [S11] Tabla 1 | Verified |
| Femur (F) | 42.16 | cm | as above | SD 3.54; range 33–54 | [S11] Tabla 1 | Verified |
| Tibia (T) | 46.22 | cm | as above | SD 4.50; range 35–55 | [S11] Tabla 1 | Verified |
| Metatarsal (MT) | 37.74 | cm | as above | SD 2.51; range 31–46 | [S11] Tabla 1 | Verified |
| Criollo breed subset | E 33.86, H 30.29, R 35.97, MC 30.56, F 41.27, T 45.46, MT 36.90 | cm | n = 147 Criollo | SD 2.76, 1.94, 2.30, 1.09, 3.01, 4.49, 1.67 | [S11] Tabla 1 | Verified |
| Withers height ÷ bone GL (May 1985 factors) | humerus 4.624, radius 4.111, metacarpal 6.102, femur 3.501, tibia 3.947, metatarsal 5.239 | ratio (SH mm / GL mm) | re-analysis of Kiesewalter 1888 and Vitt 1952 data (European horses) | no spread given | May 1985, Z. Säugetierkd. 50:368–382, quoted in Choy & Kim 2026, Table 3 [S12] | Verified (secondary) |
| Bone ÷ SH (1/factor, computed here) | humerus 0.216, radius 0.243, MC 0.164, femur 0.286, tibia 0.253, MT 0.191 | ratio | as above | from factors | computed from [S12] | Verified (derived) |
| Whole hindlimb length (passively flexed) | 133.9 | cm | 4 domestic horses, Michigan, mean mass 446 250 g | no spread given | [S1] File S1 Table SA1 | Verified |
| Whole forelimb length (incl. scapula) | 134.0 | cm | as above | no spread given | [S1] Table SA2 | Verified |
| Body mass (PanTHERIA) | 403 598.53 | g | compilation | no spread given | [S19] | Verified |
| Foot posture | unguligrade | — | Perissodactyla (Kubo's example of a tip-toe walker: "deer and horses") | — | [S20] Introduction | Verified |

Caution: Möller's lengths are skin-surface measurements on live animals between palpable points. They are not dry-bone GL, which is why they differ from osteological GL.

### 1.6 Deer: red deer / elk, white-tailed deer

| Quantity | Value | Units | Population | Spread | Source, location | Status |
|---|---|---|---|---|---|---|
| Hindlimb / forelimb length, elk (*Cervus canadensis*) | 122.3 / 137.3 | cm | n = 5, Colorado, mean mass 241 500 g | no spread given | [S1] File S1 Tables SA1/SA2 | Verified |
| Hindlimb / forelimb length, red deer (*C. elaphus*) | 80.4 / 82 | cm | n = 1, South Africa (Western Cape), mass 166 563 g (estimated from literature, † in Table 1) | single specimen | [S1] Tables SA1/SA2; Table 1 | Verified |
| Hindlimb / forelimb length, white-tailed deer (*Odocoileus virginianus*) | 91.9 / 98.1 | cm | n = 4, Illinois, mean mass 69 318 g | no spread given | [S1] Tables SA1/SA2 | Verified |
| Head–body length (PanTHERIA) | red deer 2136.72; white-tailed deer 1513.69 | mm | compilation | no spread given | [S19] | Verified |
| Body mass (PanTHERIA) | red deer 240 867.13; white-tailed deer 75 901.25 | g | compilation | no spread given | [S19] | Verified |
| Foot posture | unguligrade | — | "deer and horses" | — | [S20] Introduction | Verified |

No open per-bone deer lengths with spread were reached. See "Not found".

### 1.7 Cattle (*Bos taurus*)

| Quantity | Value | Units | Population | Spread | Source, location | Status |
|---|---|---|---|---|---|---|
| Scapula GLP | Juglans Nigra L 75.7 / R 77.4; Cyclone 64.9 / 63; Chillingham 66.7 / 65.1 | mm | 3 modern bulls: 2 Dexter (Juglans Nigra, Cyclone), 1 Chillingham; reference skeletons | individuals (no spread) | Gidney 2013, Table 2:5a, p. 336 [S13] | Verified |
| Humerus GL | 298 / 300; 272 / 271; 285 / 283 | mm | as above | individuals | [S13] Table 2:5a | Verified |
| Metacarpal GL | 180 / 181; 154 / 151; — / 182 | mm | as above | individuals | [S13] Table 2:5a | Verified |
| Femur GL | 369 / 366; 323 / 325; 357 / 355 | mm | as above | individuals | [S13] Table 2:5a | Verified |
| Tibia GL | 321 / 323; 298 / 300; 331 / 327 | mm | as above | individuals | [S13] Table 2:5a | Verified |
| Metatarsal GL | 210 / 208; 174 / 176; 213 / 210 | mm | as above | individuals | [S13] Table 2:5a | Verified |
| Live height | Juglans Nigra and Chillingham "circa 1.10 m"; Cyclone "less than 1 m" | m | the same bulls | approximate | [S13] pp. 74–75 text | Verified |
| Matolcsi factors (humerus 4.14, radius 4.30, femur 3.53, tibia 3.45, metacarpus 6.0, metatarsus 5.45) | — | ratio | Hungarian material, 1970 | — | Matolcsi 1970 (Z. Tierzücht. Züchtungsbiol. 87, https://doi.org/10.1111/j.1439-0388.1970.tb01330.x); search summaries only, which may have echoed my own query | **Unverified**; see "Sources disagree" |
| Body mass (PanTHERIA) | 618 642.42 | g | compilation | no spread given | [S19] | Verified |
| Foot posture | unguligrade | — | Artiodactyla | — | [S20] Methods (artiodactyls scored) | Category Verified; species row Unverified |

### 1.8 Pig / wild boar (*Sus scrofa*)

From Guo et al. 2015, Table 1 [S14]: bones dissected from the right side of the carcass. F2 pigs (White Duroc × Erhualian) were slaughtered at 240 ± 3 days. Erhualian and Laiwu pigs were slaughtered at 300 ± 3 days. **These are sub-adult, fattening-stage pigs, not mature adults.** SD is computed here as SE × √n.

| Bone | F2 male (n=510) mean ± SE [SD] | F2 female (n≈414) mean ± SE [SD] | Erhualian all (n=331) | Laiwu all (n=314) | Units | Status |
|---|---|---|---|---|---|---|
| Scapula length | 22.60 ± 0.067 [1.51] | 22.17 ± 0.080 [1.63] | 20.92 ± 0.075 | 21.07 ± 0.064 | cm | Verified |
| Humerus length | 18.71 ± 0.055 [1.24] | 17.94 ± 0.058 [1.18] | 17.75 ± 0.058 | 17.70 ± 0.056 | cm | Verified |
| Ulna length | 20.45 ± 0.065 [1.47] | 19.65 ± 0.072 [1.47] | 18.97 ± 0.065 | 18.85 ± 0.059 | cm | Verified |
| Femur length | 21.14 ± 0.057 [1.29] | 20.72 ± 0.066 [1.34] | 20.40 ± 0.074 | 20.38 ± 0.058 | cm | Verified |
| Tibia length | 18.97 ± 0.053 [1.20] (n=509) | 18.66 ± 0.061 [1.24] (n=413) | 18.20 ± 0.059 | 18.33 ± 0.054 | cm | Verified |

| Other | Value | Source | Status |
|---|---|---|---|
| Body mass / head–body (PanTHERIA, *S. scrofa*) | 84 471.54 g / 1349.99 mm, no spread | [S19] | Verified |
| Teichert 1969 withers factors (humerus 4.0, radius 5.3, tibia 3.8, MC III 10.5, MC IV 10.3, astragalus 18.5, calcaneus 9.7) | search summary only | **Unverified** |
| Turopolje pig (Croatian breed) femur length 202.0 mm, humerus 194.9 mm; hogs n = 19, live weight 100.3 ± 4.9 kg (fattening stage) | Đikić et al. 2007, *Poljoprivreda* 13(1): 172–174, abstract [S21]. SDs (7.11, 4.31) appeared only in a search summary; the abstract gives no SD, so the SDs stay Unverified | Means Verified (abstract); SD Unverified |
| Foot posture | unguligrade (Artiodactyla scheme [S20]) | Category Verified; species row Unverified |

### 1.9 Bears (*Ursus arctos, U. americanus*)

| Quantity | Value | Units | Population | Spread | Source | Status |
|---|---|---|---|---|---|---|
| Head–body length | *U. arctos* 1484.07; *U. americanus* 1380.87 | mm | species values, compilation | no spread given | [S19] | Verified |
| Body mass | *U. arctos* 196 287.50; *U. americanus* 110 500.00 | g | as above | no spread given | [S19] | Verified |
| Foot posture | plantigrade (Ursidae) | — | — | — | not confirmed in a source opened here | **Unverified** |

No bear limb-bone lengths were reached. See "Not found".

### 1.10 European rabbit (*Oryctolagus cuniculus*, lab strain JW-NIBS)

Otsuka, Toyomitsu & Nishinakagawa 1980–1981 [S15]. JW-NIBS rabbits aged 366–368 days, 8 males and 6 females. Body mass M 2810 ± 80 g, F 3384 ± 354 g. Head–body length M 486 ± 21 mm, F 503 ± 14 mm (1980 paper, p. 445). Values are mean ± SD.

| Bone / measure | Male | Female | Units | Location | Status |
|---|---|---|---|---|---|
| Scapula lateral length | 67.1 ± 1.5 | 66.2 ± 1.0 | mm | 1981, Table 1 (p. 116) | Verified |
| Humerus GL | 72.1 ± 1.5 | 71.8 ± 1.6 | mm | 1981, Table 1 | Verified |
| Radius GL | 65.4 ± 1.1 | 63.0 ± 1.9 | mm | 1981, Table 1 | Verified |
| Ulna GL | 78.1 ± 0.9 | 76.4 ± 2.9 | mm | 1981, Table 1 | Verified |
| Metacarpal III GL | 22.0 ± 0.8 | 22.0 ± 0.9 | mm | 1981, Table 2 (p. 117) | Verified |
| Os coxae GL | 87.9 ± 2.4 | 89.3 ± 2.5 | mm | 1981, Table 4 (p. 119) | Verified |
| Femur GL | 93.9 ± 2.0 | 93.7 ± 2.1 | mm | 1981, Table 4 | Verified |
| Tibia GL | 104.7 ± 2.5 | 102.7 ± 3.0 | mm | 1981, Table 4 | Verified |
| Metatarsal III GL | 38.9 ± 1.4 | 38.7 ± 0.9 | mm | 1981, Table 5 (p. 120) | Verified |
| Skull (cranium) GL | 92.4 ± 1.8 | 92.9 ± 3.5 | mm | 1980, Table 1 (p. 447) | Verified |
| Skull greatest breadth | 42.4 ± 0.8 | 41.6 ± 1.4 | mm | 1980, Table 1 | Verified |
| Caudal vertebrae count | 15–17 | — | count | 1980, abstract | Verified |

The same tables give the wild hare *Lepus brachyurus* (wild-caught, 12 M, 10 F). Example: humerus GL M 84.1 ± 2.9, F 86.3 ± 3.9 mm; tibia M 123.0 ± 3.8, F 124.4 ± 4.9 mm. Verified.

Second source (New Zealand White rabbits, 1–1.5 y, 3–3.5 kg, CT reconstruction, 8 M + 8 F): Özkadif et al. 2015, Tables 1–2 [S16].

| Measure | Male | Female | Status |
|---|---|---|---|
| Radius maximal length, right / left | 64.5 ± 10.76 / 66.65 ± 10.92 mm | 66.29 ± 4.23 / 67.37 ± 4.58 mm | Verified |
| Ulna maximal length, right / left | 71.7 ± 8.29 / 73.39 ± 8.35 mm | 76.91 ± 3.25 / 77.99 ± 3.53 mm | Verified |

PanTHERIA species values: mass 1590.57 g, head–body 400.00 mm, no spread [S19]. Verified.

Unverified lead: NZW rabbit humerus by CT, "F 68.05 / M 57.14 mm" (https://doi.org/10.53913/aduveterinary.1819078). The sex gap is implausibly large; check before use.

### 1.11 Brown rat (*Rattus norvegicus*, Wistar)

| Quantity | Value | Units | Population | Spread | Source, location | Status |
|---|---|---|---|---|---|---|
| Humerus length | 29.88 | mm | 20 male Wistar rats, 10–38 weeks, 240–720 g (mean 476 ± 160 g), 40 paired bones | SD 3.33; range 26–34 | Prodinger et al. 2018, Results ("Humerus") [S17] | Verified |
| Femur length | 38.9 | mm | as above | SD 4.12; range 34–46 | [S17] Results ("Femur") | Verified |
| Tibia length | 41.8 | mm | as above | SD 4.12; range 37–48 | [S17] Results ("Tibia") | Verified |
| Whole hindlimb / forelimb (black rat *R. rattus*, not *norvegicus*) | 9.8 / 8.6 | cm | n = 4, Illinois, mean 331 g | no spread given | [S1] File S1 Tables SA1/SA2 | Verified |
| Body mass (PanTHERIA, *R. norvegicus*) | 282.89 | g | compilation | no spread given | [S19] | Verified |

Note: the rat sample mixes skeletally immature (<400 g) and mature animals. The paper also gives subgroup values; for example, light rats averaged 35.5 mm femur length.

### 1.12 Elephants (*Loxodonta africana, Elephas maximus*)

| Quantity | Value | Units | Population | Spread | Source, location | Status |
|---|---|---|---|---|---|---|
| Shoulder height, Grade I (≈90% of fully grown adults, optimal condition) | *L. africana* ♂ 304–336 (mean 320); ♀ 247–273 (260); *L. cyclotis* ♂ 209–231 (220); *E. maximus* ♂ 261–289 (275); ♀ 228–252 (240) | cm | fully grown adults, compiled from population studies | range as given | Larramendi 2016, Table 8, p. 555 [S18] | Verified |
| Body mass, Grade I | *L. africana* ♂ 5.2–6.9 (6); ♀ 2.6–3.5 (3); *E. maximus* ♂ 3.5–4.6 (4); ♀ 2.3–3.1 (2.7) | t | as above | range | [S18] Table 8 | Verified |
| Limb bones, "Jumbo" (AMNH 3283, male *L. africana*; SH 323 cm) | humerus 1091, ulna 1037, femur 1258, tibia 788, fibula 764 | mm | 1 individual (captive) | single specimen | [S18] SOM Table 1; SH in main text p. 541 | Verified |
| Limb bones, female *L. africana* NMNS002990–F002715 (SH in the flesh 253 cm) | humerus 846, ulna 750, femur 980, tibia 585, fibula 576 | mm | 1 individual | single specimen | [S18] SOM Table 1; SH p. 541 | Verified |
| Skeletal SH from forelimb | (scapula + humerus + ulna + manus, articular lengths) × 0.98; or maximal lengths × 0.95 | — | Elephantidae | method | [S18] p. 539 and Fig. 1 | Verified |
| Manus height | ≈ 2 × metacarpal III length; ≈ 50% of radius length; MC III ≈ 25% of radius | ratio | derived proboscideans | Table 1 ratios 1.97–1.98 for elephantids | [S18] p. 539, Tables 1–2 | Verified |
| In-life SH vs skeletal SH | about 5.5% taller in life | % | proboscideans | — | [S18] p. 540 (citing Larramendi 2014) | Verified |
| Bone ÷ SH ratios (mammoths, *Palaeoloxodon*; no extant column) | e.g. *M. primigenius* humerus max 34.36%, femur 40.98%, radius 26.67%, tibia 23.2% | % of skeletal SH | extinct elephantids | — | [S18] Table 3 | Verified (extinct taxa only) |
| Head–body length (PanTHERIA, *E. maximus*) | 1919.20 | mm | compilation | no spread given | [S19] | Verified |
| Body mass (PanTHERIA) | *L. africana* 3 824 539.93; *E. maximus* 3 269 794.34 | g | compilation | no spread given | [S19] | Verified |
| Foot posture | see "Sources disagree": subunguligrade vs plantigrade | — | — | — | [S18] p. 539; [S20] Methods | Verified (both claims) |


### 1.13 Pass 2: museum skeleton data from CC0 datasets

**(a) Samuels, Meachen & Sakai 2013 data [S23].** Tab-delimited file, one row per species, values in mm (greatest lengths). The file does not give n or spread for each species. It is probably species means of museum specimens, but the data file does not state this, so treat these as **no spread given**. Columns: ScL, HuL, RaL, UlL, MC3L, InnomL, FeL, TiL, MT3L.

| Species (as in file) | Scapula | Humerus | Radius | Ulna | Metacarpal III | Innominate | Femur | Tibia | Metatarsal III | Status |
|---|---|---|---|---|---|---|---|---|---|---|
| *Canis lupus* | 144.2 | 212.12 | 210.94 | 248.38 | 89.29 | 165.77 | 229.54 | 233.71 | 98.57 | Verified |
| *Felis silvestris libyca* (African wildcat, the wild ancestor of *F. catus*) | 73.67 | 114.26 | 114.64 | 132.24 | 39.74 | 83.85 | 128.6 | 135.21 | 63.59 | Verified |
| *Panthera leo* | 201.19 | 304.84 | 280.56 | 335.43 | 106.83 | 294.63 | 324.13 | 286.84 | 115.42 | Verified |
| *Ursus arctos* | 215.77 | 330.65 | 277.55 | 316.63 | 76.76 | 315.87 | 386.06 | 290.4 | 77.29 | Verified |
| *Ursus americanus* | 191.62 | 293.89 | 260.32 | 294.41 | 67.88 | 246.73 | 334.51 | 256.68 | 67.32 | Verified |

Units are mm. Location: data file "Carnivore Postcranial Morphology Data Samuels et al. 2013.txt" plus its README (measurement legend). Tiger, leopard and domestic dog are **not** in this file.

**(b) Meachen-Samuels & Van Valkenburgh forelimb database [S22].** One row per museum specimen (FMNH, LACM, MVZ, UCLA, USNM), forelimb only, GL in mm. Mean, SD and range were **computed here** from the individual rows. Sex is as recorded in the file.

| Species | Sex (n specimens) | Humerus GL | Radius GL | Ulna GL | Metacarpal III GL | Status |
|---|---|---|---|---|---|---|
| *Panthera leo* (incl. 1 *P. l. massaica*) | all (12: 8 M, 3 F, 1 unknown) | 322.7 ± 25.4 (290.3–353.6) | 293.7 ± 18.9 (263.9–319.4) | 353.0 ± 25.2 (308.8–385.1) | 114.8 ± 7.8 (101.0–124.6), n = 8 | Verified (computed) |
| *P. leo* | M (8) | 333.4 ± 24.0 | 302.5 ± 15.8 | 365.8 ± 17.6 | 116.1 ± 7.5, n = 7 | Verified (computed) |
| *P. leo* | F (3) | 297.1 ± 6.6 | 273.7 ± 10.6 | 322.2 ± 16.3 | not measured | Verified (computed) |
| *Panthera tigris* | all (11: 4 M, 3 F, 4 unknown) | 307.3 ± 16.3 (275.2–333.1) | 262.9 ± 13.9 (235.6–279.6) | 326.8 ± 17.0 (296.4–350.6) | 108.6 ± 5.6 (98.5–115.8), n = 10 | Verified (computed) |
| *P. tigris* | M (4) / F (3) | 308.1 ± 24.1 / 299.6 ± 14.1 | 261.5 ± 18.7 / 255.8 ± 14.4 | 333.2 ± 13.6 / 307.2 ± 18.0 | 108.3 ± 6.1 (n = 4) / 104.6 ± 8.6 (n = 2) | Verified (computed) |
| *Panthera pardus* | all (11: 7 M, 1 F, 3 unknown) | 217.7 ± 19.9 (190.8–252.3) | 184.6 ± 15.8 (165.2–210.3) | 224.3 ± 19.9 (200.5–258.1) | 71.1 ± 4.2 (63.8–76.7), n = 9 | Verified (computed) |
| *P. pardus* | M (7) | 228.2 ± 16.5 | 192.9 ± 13.7 | 234.9 ± 16.8 | 72.7 ± 3.3, n = 6 | Verified (computed) |
| *Canis lupus* | all (2: 1 M, 1 F) | 200.9 (195.2–206.7) | 195.9 (179.3–212.4) | 234.8 (217.6–251.9) | 83.6 (78.8–88.5) | Verified (computed; n = 2, so range only) |
| *Felis silvestris* (all subspecies) | all (8: 4 M, 4 F) | 114.3 ± 5.5 (105.7–121.6) | 110.0 ± 7.0 (99.4–120.8) | 128.2 ± 7.1 (118.5–138.4) | 39.8 ± 2.0 (37.4–43.0), n = 6 | Verified (computed) |

Values are mean ± SD (range), in mm. Location: data file "Felidae and Canidae postcranial database.csv", columns "GL hum", "GL r", "GL u", "GL mc3". The README says all measurements are in mm. Hindlimb, scapula, skull, neck and tail are not in this dataset.

---

## 2. Allometry (length ∝ body mass^b)

| Group | Quantity | Equation / exponent | CI / spread | n | Source, location | Status |
|---|---|---|---|---|---|---|
| 37 species, shrew to elephant | limb bone length | ∝ M^0.35 (diameter ∝ M^0.36) | not in abstract | 37 species | Alexander et al. 1979, abstract [S2] | Verified (abstract only; full tables paywalled) |
| Bovidae | limb bone length | exponent "much nearer 0.25" | not in abstract | — | [S2] abstract | Verified |
| 44 quadruped species, all | hindlimb length (cm) vs mass (g) | log L = 0.07 + 0.37 log M | b 95% CI 0.342–0.408; R² 0.920 | 44 | Kilbourne & Hoffman 2013, Table 3 [S1] | Verified |
| Artiodactyla | hindlimb | log L = 0.43 + 0.30 log M | 0.243–0.381; R² 0.897 | 12 | [S1] Table 3 | Verified |
| Carnivora | hindlimb | log L = −0.08 + 0.42 log M | 0.322–0.542; R² 0.771 | 17 | [S1] Table 3 | Verified |
| Rodentia | hindlimb | log L = 0.34 + 0.27 log M | 0.224–0.332; R² 0.951 | 9 | [S1] Table 3 | Verified |
| Cursors | hindlimb | log L = 0.51 + 0.29 log M | 0.265–0.313; R² 0.963 | 25 | [S1] Table 3 | Verified |
| 44 species, all | forelimb (incl. scapula) | log L = −0.02 + 0.40 log M | 0.367–0.434; R² 0.926 | 44 | [S1] Table 3 | Verified |
| Artiodactyla | forelimb | log L = 0.28 + 0.34 log M | 0.271–0.434 | 12 | [S1] Table 3 | Verified |
| Carnivora | forelimb | log L = −0.07 + 0.42 log M | 0.333–0.533 | 17 | [S1] Table 3 | Verified |
| Rodentia | forelimb | log L = 0.24 + 0.30 log M | 0.242–0.370 | 9 | [S1] Table 3 | Verified |
| Cursors | forelimb | log L = 0.40 + 0.32 log M | 0.288–0.350 | 25 | [S1] Table 3 | Verified |
| Felidae, 9 species (3.7–169 kg) | FLL (cm) vs mass (kg) | log FLL = 1.310 + 0.249 log M | slope ± 95% CL 0.096; intercept ± 0.149; r² 0.84 | 9 | Day & Jayne 2007, Table 4 [S7] | Verified |
| Felidae | HLL vs mass | log HLL = 1.440 + 0.231 log M | ± 0.107; ± 0.166; r² 0.79 | 9 | [S7] Table 4 | Verified |
| Felidae | intergirdle distance vs mass | log IGD = 1.445 + 0.238 log M | ± 0.081; ± 0.126; r² 0.87 | 9 | [S7] Table 4 | Verified |
| Felidae | FLL vs IGD | log FLL = −0.107 + 1.059 log IGD | ± 0.093; ± 0.024; r² 0.99 | 9 | [S7] Table 4 | Verified |
| Felidae | HLL vs IGD | log HLL = −0.015 + 1.001 log IGD | ± 0.182; ± 0.046; r² 0.96 | 9 | [S7] Table 4 | Verified |
| *L. africana* ♂, young to Grade I | body mass (kg) vs SH (cm) | M = 3.28×10⁻⁴ × SH^2.899 | r 0.999; mean PE 1.19% | — | Larramendi 2016, Table 7 [S18] | Verified |
| *L. africana* ♀ | as above | M = 3.22×10⁻⁴ × SH^2.886 | r 0.963; PE 4.55% | — | [S18] Table 7 | Verified |
| *E. maximus*, mixed sexes | as above | M = 3.21×10⁻⁴ × SH^2.903 | r 0.988; PE 3.18% | — | [S18] Table 7 | Verified |
| Terrestrial mammals | long-bone length vs mass | no single power law; large mammals have lower slopes ("differential scaling") | qualitative (abstract) | large sample | Christiansen 1999, abstract [S3] | Verified (abstract only) |
| 118 Carnivora | long-bone length and diameter | within families roughly linear; across the order non-linear; near-isometric below ~100 kg | qualitative (abstract) | 118 species | Bertram & Biewener 1990, abstract [S5] | Verified (abstract only) |

Notes:
- Kilbourne & Hoffman's units are not printed beside Table 3. I inferred g and cm by checking against their own Table SA1. Example: Carnivora hindlimb at wolf mass 30 773 g gives 63.8 cm predicted, against 65.7 cm measured. Slopes are Model II (RMA) regressions on log10 species means.
- Christiansen 2002 [S4] is about **bone mass**, not bone length. Femur, tibia and metatarsus masses scale isometrically; forelimb bone masses scale with positive allometry. Those exponents are not used here.
- Garcia & da Silva 2004 [S6] and 2006 [S6b] are theoretical models (bending plus axial compression). They are not species data. The "Garcia & da Silva 2006, J Mammal" named in the brief appears to be the 2006 review in *Physics of Life Reviews* 3:188–209.

---

## 3. Not found

- **Neck and tail lengths** for every species except rabbit caudal vertebra count (15–17). No open table of neck or tail length against trunk length was reached for dog, wolf, cat, big cats, horse, deer, cattle, pig, bear, rat or elephant. PanTHERIA has no tail field.
- **Skull length and width** for dog, cat, lion, tiger, leopard, horse, deer, cattle, pig, bear, rat and elephant. Only wolf basal length (n = 5) and rabbit skull GL and breadth were verified.
- **Bone-by-bone lengths with spread** for:
  - adult dog breeds
  - lion, tiger and leopard dry bones: filled in Pass 2 (§1.13) for the forelimb, and for the lion hindlimb; tiger and leopard hindlimbs are still missing
  - deer (only whole-limb lengths)
  - bears: filled in Pass 2 (§1.13; no spread given)
  - adult pigs (only sub-adult)
  - cattle beyond three individual bulls
  - extant elephants beyond two individuals
- **Metacarpal and metatarsal lengths** for dog, wolf, cat, pig, deer and elephant (only rabbit, horse, cattle and felid segment percentages were verified).
- **Alexander et al. 1979 species tables and exponent CIs**: the full text is paywalled. Only the abstract was read.
- **Christiansen 1999 slopes and CIs per bone**: only the abstract was read.
- **Garcia & da Silva 2006 in J. Mammal.**: no such paper was found. See the note above.
- **von den Driesch & Boessneck 1974 factor tables** (cattle, pig, sheep): the original was not opened. The Matolcsi and Teichert factors stay Unverified.
- **Pass 2:** the felid and canid forelimb set was obtained from Zenodo (now §1.13). **Serio et al. 2024 is still not obtained**: the Dryad API now needs a bearer token (a logged-in account), the web download returns 403, and there is no mirror on Zenodo or figshare. Getting it needs the owner to sign in to Dryad or download it by hand.
- (Pass 1 note) Dryad datasets that would close several gaps could not be downloaded: the API needs a token and the web download returned 403. The browser was busy with another agent. Both datasets are licensed CC0:
  - Serio et al. 2024: humerus, radius, femur and tibia lengths plus greatest skull length for 246 living species. https://doi.org/10.5061/dryad.8gtht76sp
  - Meachen-Samuels & Van Valkenburgh 2009/2011: felid and canid forelimbs. https://doi.org/10.5061/dryad.1fh61
- Open sources that are paywalled or blocked to automated fetch:
  - Toledo et al. 2014 genus-mean limb database (CONICET repository): https://ri.conicet.gov.ar/handle/11336/235897
  - Barrosã cattle osteometry (Saguntum Extra, behind a bot wall)
  - Scott 1985 bovid allometry (BHL, behind a Cloudflare challenge)

## 4. Sources disagree

1. **Elephant foot posture.**
   - Larramendi 2016 (p. 539) describes elephants as **subunguligrade**, citing Trevisan 1949, Miller et al. 2008 and Hutchinson et al. 2011.
   - Kubo et al. 2019 (Methods) **classify elephants as plantigrade (P)**, "based on the distribution of their planter pressure". The same paper scores rhinos, hippos and camels as digitigrade.
   - Choose by purpose: skeletal pose says subunguligrade (the toes are raised over a fat pad); ground-contact pressure says plantigrade.
2. **Cattle withers-height factors (Matolcsi 1970).**
   - Gidney 2013 (Table 2:5b, pp. 75 and 336) applied Matolcsi's factors to bulls of known height and found the humerus factor "seriously flawed": it overestimates height in every case (1.23 m against about 1.10 m live). Proximal bones overestimate, distal metapodials do well.
   - The femur factor implied by Gidney's own numbers is about 3.22 (1.19 m ÷ 369 mm). The factor commonly quoted in search results is 3.53 (Unverified). One of the two is wrong, or they used different femur measures.
3. **Dog shoulder-height formulae applied to wolves.** Gatta et al. 2016 (p. 948) note that Harcourt's (1974) formulae, built on domestic dogs, underestimate wolf withers height.
4. **Horse withers factors: May (1985) vs Hayashida (1957).** These come from European horses and Japanese horses respectively. Hayashida's factors are lower for every bone: humerus 4.58 vs 4.624, metacarpal 5.95 vs 6.102, metatarsal 4.98 vs 5.239 [S12, Table 3]. Eisenmann (2009) adds that metapodial factors in equids range from 5.32 (cursorial) to 6.72 (graviportal) for metacarpals, so any single factor misleads.
5. **Limb-length exponent: about 0.35 vs 0.37–0.40 vs about 0.24.**
   - Alexander et al. 1979: bone lengths ∝ M^0.35.
   - Kilbourne & Hoffman 2013: whole-limb length ∝ M^0.37 (hind) and M^0.40 (fore).
   - Day & Jayne 2007: within felids only, about M^0.23–0.25, which does not differ significantly from 0.33 given the wide CI.
   - Christiansen 1999 and Bertram & Biewener 1990 say the exponent itself changes with size.
   - So no single "b" is right across the whole working set. Use a group-specific equation.
6. **Rabbit radius length**: JW-NIBS (GL 65.4 M / 63.0 F mm, small SD) vs NZW CT (64.5–67.4 mm, SD up to 10.9 mm). The means agree, but the CT spreads are about 10 times larger.
7. **Lion femur vs tibia (Pass 2).**
   - Dry-bone data [S23] give lion femur 324.13 mm and tibia 286.84 mm, so the tibia is about 12% *shorter* than the femur.
   - Day & Jayne's live-animal video segments [S7, Table 3] give tibia 37.3% vs femur 36.4% of hindlimb length, so the tibia is slightly *longer*.
   - The likely cause is method: skin-marker joint centres vs bone greatest length. For skeleton building, use the dry-bone numbers.
8. **Lion humerus length between two museum samples (Pass 2).** Samuels et al. [S23] give 304.84 mm. Meachen-Samuels [S22] gives 322.7 ± 25.4 mm (n = 12, mostly males). The Samuels value is within one SD; sample sex mix is the likely cause.

---

## 5. Sources (full entries; all accessed 2026-11-16)

Link check, 2026-11-16 (curl from the box, plus WebFetch/WebSearch). Pass 2: S22 and S23 Zenodo records returned HTTP 200, and the CC0 data files were read before deletion.
- **Opened, and they show the cited number:**
  - S1 (article and File S1)
  - S7 (page text, through WebFetch; curl gets 403)
  - S8 (through WebFetch; curl gets 403)
  - S9 PDF, S10 PDF, S11 PDF, S12 PDF and article page, S13 PDF
  - S14 and S17 (PMC full text; DOIs resolve)
  - S15 (J-STAGE PDFs)
  - S16 PDF, S18 PDF and SOM, S19 data and metadata, S20 manuscript (CentAUR), S21 abstract page
  - S3, S4, S5 (PubMed abstracts)
- **DOIs that refuse automated requests** (403 or 500 to curl), but whose records were confirmed through Crossref metadata, PubMed or a search-engine copy of the page:
  - 10.1111/j.1469-7998.1979.tb03964.x (Wiley; abstract read through a search result)
  - 10.1002/jmor.1083
  - 10.1242/jeb.00890 and 10.1242/jeb.02703
  - 10.1073/pnas.1814329116
  - 10.1890/08-1494.1
  - 10.3390/vetsci11110522
  - 10.4202/app.00136.2014
  - 10.1186/s12863-015-0257-1
  - 10.1016/j.crpv.2016.04.011
  - 10.22099/ijvr.2015.3069

  For each of these, a direct PDF or PMC link that does open is given alongside it.
- **Not working:**
  - https://doi.org/10.60015/bjvas/v10i1a4 (tiger lead, 404). It is in an Unverified row only.
  - https://ri.conicet.gov.ar/handle/11336/235897 (timed out). It is in "Not found" only.

| ID | Full reference | URL(s) | Licence | Used for |
|---|---|---|---|---|
| S1 | Kilbourne BM, Hoffman LC (2013) Scale effects between body size and limb design in quadrupedal mammals. *PLoS ONE* 8(11): e78392. Table 1, Table 3; File S1 Tables SA1, SA2. | https://doi.org/10.1371/journal.pone.0078392 ; File S1: https://doi.org/10.1371/journal.pone.0078392.s001 | CC BY 4.0 | whole-limb lengths; allometry |
| S2 | Alexander RMcN, Jayes AS, Maloiy GMO, Wathuta EM (1979) Allometry of the limb bones of mammals from shrews (*Sorex*) to elephant (*Loxodonta*). *J. Zool., Lond.* 189(3): 305–314. Abstract. | https://doi.org/10.1111/j.1469-7998.1979.tb03964.x | paywalled (Wiley); abstract cited as fact | exponents 0.35/0.36/0.25 |
| S3 | Christiansen P (1999) Scaling of the limb long bones to body mass in terrestrial mammals. *J. Morphol.* 239(2): 167–190. Abstract. | https://pubmed.ncbi.nlm.nih.gov/9951716/ | paywalled; abstract cited | qualitative scaling |
| S4 | Christiansen P (2002) Mass allometry of the appendicular skeleton in terrestrial mammals. *J. Morphol.* 251(2): 195–209. Abstract. | https://doi.org/10.1002/jmor.1083 ; https://pubmed.ncbi.nlm.nih.gov/11748703/ | paywalled; abstract cited | bone mass note |
| S5 | Bertram JEA, Biewener AA (1990) Differential scaling of the long bones in the terrestrial Carnivora and other mammals. *J. Morphol.* 204(2): 157–169. Abstract. | https://pubmed.ncbi.nlm.nih.gov/2348461/ | paywalled; abstract cited | qualitative scaling |
| S6 | Garcia GJM, da Silva JKL (2004) On the scaling of mammalian long bones. *J. Exp. Biol.* 207: 1577–1584. | https://doi.org/10.1242/jeb.00890 | © Company of Biologists; cited as fact | theory note |
| S6b | Garcia GJM, da Silva JKL (2006) Interspecific allometry of bone dimensions: a review of the theoretical models. *Phys. Life Rev.* 3: 188–209. | https://doi.org/10.1016/j.plrev.2006.07.002 | Elsevier, paywalled | theory note |
| S7 | Day LM, Jayne BC (2007) Interspecific scaling of the morphology and posture of the limbs during the locomotion of cats (Felidae). *J. Exp. Biol.* 210(4): 642–654. Tables 1, 3, 4. | https://doi.org/10.1242/jeb.02703 ; https://journals.biologists.com/jeb/article/210/4/642/17244/Interspecific-scaling-of-the-morphology-and | © Company of Biologists (free to read); figures cited as facts | cat and big-cat proportions; felid allometry |
| S8 | Poradowski D, Mutlu Z, Altundağ Y, Chrószcz A, Sarıtaş Ö, Wolińska J, Onar V (2024) Estimation of height at withers based on long bone measurements of living cats. *Vet. Sci.* 11(11): 522. Tables 1–4. | https://doi.org/10.3390/vetsci11110522 ; https://www.mdpi.com/2306-7381/11/11/522 | CC BY 4.0 (MDPI open access) | cat bones, SH, factors |
| S9 | Gatta M, Rolfo MF, Petronio C, Salari L, Silvestri L (2016) Late Pleistocene skeleton of *Canis lupus* L., 1758 from Grotta Mora Cavorso (Jenne, Latium, central Italy). *C. R. Palevol* 15(8): 941–949. Tables 2–3, p. 948. | https://doi.org/10.1016/j.crpv.2016.04.011 ; PDF: https://sciencepress.mnhn.fr/sites/default/files/articles/pdf/comptes-rendus-palevol2016v15f8a06.pdf | © Académie des sciences / Elsevier, "all rights reserved"; measurements cited as facts | wolf bones, skull, withers |
| S10 | Crockford SJ (1997; SFU Archaeology Press edition 2017) *Osteometry of Makah and Coast Salish Dogs*. Ch. 9 "Characterization of dog types", p. 88; quotes Harcourt RA (1974) The dog in prehistoric and early historic Britain, *J. Archaeol. Sci.* 1: 151–175 and Clark KM (1995). | https://archpress.lib.sfu.ca/index.php/archpress/catalog/book/52 ; ch. 9 PDF: https://archpress.lib.sfu.ca/index.php/archpress/catalog/download/52/23/1016?inline=1 ; Harcourt: https://doi.org/10.1016/0305-4403(74)90040-5 | © 2017 SFU Archaeology Press (free PDF); formulae cited as facts | dog SH formulae |
| S11 | Möller R (2017) *Análisis morfométrico de los miembros del caballo*. MSc thesis, PEDECIBA Biología, Universidad de la República, Uruguay. Tabla 1 (p. 43). | https://www.colibri.udelar.edu.uy/server/api/core/bitstreams/5c86be74-4b64-42f2-80a4-6544c32e5d84/content | institutional repository (Colibrí); no licence stated in the PDF; cited as facts | horse limb lengths |
| S12 | Choy K, Kim J (2026) Estimating the withers height of horses in the Joseon Dynasty through zooarchaeological analysis: a case study from the Susong-dong and Euljiro-Sewoon districts in Seoul, Korea. *Anatomy & Biological Anthropology* 39(3): 225–239 (in Korean, English abstract). Table 3 quotes May E (1985) Widerristhöhe und Langknochenmaße bei Pferden: ein immer noch aktuelles Problem, *Z. Säugetierkd.* 50: 368–382, and Hayashida S, Yamauchi C (1957) *Bull. Fac. Agric. Kagoshima Univ.* 6: 146–156. | https://doi.org/10.11637/aba.2026.39.3.225 ; https://journal.kci.go.kr/aba/archive/articleView?artiId=ART003390670 ; PDF: https://journal.kci.go.kr/aba/archive/articlePdf?artiId=ART003390670 | journal open access; licence not stated in the text read; factors cited as facts | horse factors |
| S12b | Eisenmann V (2009) Withers height estimations (web page). | https://vera-eisenmann.com/withers-height-estimations?lang=en | author's website, © author | equid metapodial factor range |
| S13 | Gidney LJ (2013) *Offspring of the Aurochs: a comparison of a reference collection of Dexter cattle skeletons with archaeological and historical data.* PhD thesis, Durham University. Tables 2:5a–d (p. 336), text pp. 74–75. | https://etheses.dur.ac.uk/10561/ ; PDF: https://etheses.dur.ac.uk/10561/1/ethesis.pdf | Durham E-Theses use policy: free for personal research, study, education and not-for-profit use with citation; not an open licence. Cite as facts only. | cattle bones, Matolcsi test |
| S14 | Guo Y, Hou L, Zhang X, Huang M, Mao H, Chen H, et al. (2015) A meta analysis of genome-wide association studies for limb bone lengths in four pig populations. *BMC Genetics* 16: 95. Table 1. | https://doi.org/10.1186/s12863-015-0257-1 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC4518597/ | CC BY 4.0 (data CC0) | pig bones |
| S15 | Otsuka J, Toyomitsu Y, Nishinakagawa H (1980) Linear measurements of the bones of *Lepus brachyurus brachyurus*, *Pentalagus furnessi* and *Oryctolagus cuniculus* (JW-NIBS). I. Cranium and ossa trunci. *Exp. Anim.* 29(4): 441–455. Table 1, p. 445. And (1981) II. Ossa membri thoracici et pelvini. *Exp. Anim.* 30(2): 113–127. Tables 1, 2, 4, 5. | https://doi.org/10.1538/expanim1978.29.4_441 ; https://www.jstage.jst.go.jp/article/expanim1978/29/4/29_4_441/_pdf ; https://doi.org/10.1538/expanim1978.30.2_113 ; https://www.jstage.jst.go.jp/article/expanim1978/30/2/30_2_113/_pdf | © Japanese Association for Laboratory Animal Science (free on J-STAGE); cited as facts | rabbit bones, skull, body size |
| S16 | Özkadif S, Eken E, Beşoluk K, Dayan MO (2015) Three-dimensional reconstruction of New Zealand rabbit antebrachium by multidetector computed tomography. *Iran. J. Vet. Res.* 16(2): 205–209. Tables 1–2. | https://doi.org/10.22099/ijvr.2015.3069 ; https://ijvr.shirazu.ac.ir/article_3069_cabc3dba6f9be7b9a846861beccfb37c.pdf | CC BY-NC-ND 4.0 (journal site). Non-commercial: cite numbers as facts only, do not redistribute the article. | rabbit radius and ulna |
| S17 | Prodinger PM, Foehr P, Bürklein D, Bissinger O, Pilge H, Kreutzer K, von Eisenhart-Rothe R, et al. (2018) Whole bone testing in small animals: systematic characterization of the mechanical properties of different rodent bones available for rat fracture models. *Eur. J. Med. Res.* 23: 8. Results. | https://doi.org/10.1186/s40001-018-0307-z ; https://pmc.ncbi.nlm.nih.gov/articles/PMC5813325/ | CC BY 4.0 (data CC0) | rat bones |
| S18 | Larramendi A (2016) Shoulder height, body mass, and shape of proboscideans. *Acta Palaeontol. Pol.* 61(3): 537–574. Tables 1–3, 7, 8; pp. 539–541; SOM Table 1. | https://doi.org/10.4202/app.00136.2014 ; PDF: https://www.app.pan.pl/archive/published/app61/app001362014.pdf ; SOM: https://app.pan.pl/SOM/app61-Larramendi_SOM.pdf | CC BY 4.0 | elephant SH, mass, bones, posture |
| S19 | Jones KE, Bielby J, Cardillo M, et al. (2009) PanTHERIA: a species-level database of life history, ecology, and geography of extant and recently extinct mammals. *Ecology* 90(9): 2648. Ecological Archives E090-184. File PanTHERIA_1-0_WR05_Aug2008.txt, columns 5-1 and 13-1. | https://doi.org/10.1890/08-1494.1 ; https://esapubs.org/archive/ecol/E090/184/ ; metadata: https://esapubs.org/archive/ecol/E090/184/metadata.htm | Metadata: "Copyright restrictions: None. Proprietary restrictions: None"; citation requested. The data file was read in scratch only and deleted afterwards; it was not copied into any repo. | body mass, head–body length |
| S20 | Kubo T, Sakamoto M, Meade A, Venditti C (2019) Transitions between foot postures are associated with elevated rates of body size evolution in mammals. *PNAS* 116(7): 2618–2623. Introduction and Methods (accepted manuscript). | https://doi.org/10.1073/pnas.1814329116 ; manuscript: https://centaur.reading.ac.uk/81921/ | PNAS © (paywalled); author manuscript on CentAUR; cited as facts | foot-posture scheme; elephant as plantigrade |
| S21 | Đikić M, Salajpal K, Karolyi D, Đikić D, Pavić M, Jurić I (2007) Characteristics of femur and humerus in Turopolje pig, an autochthonous Croatian breed. *Poljoprivreda* 13(1): 172–174. Abstract. | https://hrcak.srce.hr/16131 | Hrčak open-access journal portal; licence not stated on the abstract page; cited as facts | pig femur and humerus means |
| S22 | Meachen-Samuels J, Van Valkenburgh B (2009) Forelimb indicators of prey-size preference in the Felidae. *J. Morphol.* 270(6): 729–744. Data: "Data from: Forelimb indicators of prey-size preference in the Felidae" (Dryad 2011; Zenodo mirror record 4969945), file "Felidae and Canidae postcranial database.csv" and README. | Paper: https://doi.org/10.1002/jmor.10712 ; data: https://doi.org/10.5061/dryad.1fh61 ; mirror: https://zenodo.org/records/4969945 | Data **CC0 1.0**; paper © Wiley (paywalled). Raw file deleted after use. | big-cat, wolf and wildcat forelimb bones |
| S23 | Samuels JX, Meachen JA, Sakai SA (2013) Postcranial morphology and the locomotor habits of living and extinct carnivorans. *J. Morphol.* 274(2): 121–146. Data: "Data from: Postcranial morphology and the locomotor habits of living and extinct carnivorans" (Dryad; Zenodo mirror record 4964743), file "Carnivore Postcranial Morphology Data Samuels et al. 2013.txt" and README. | Paper: https://doi.org/10.1002/jmor.20077 ; mirror: https://zenodo.org/records/4964743 | Data **CC0 1.0**; paper © Wiley (paywalled). Raw file deleted after use. | bone-by-bone lengths: wolf, wildcat, lion, bears |

Datasets named but **not** used: Serio et al. 2024 (Dryad, CC0) https://doi.org/10.5061/dryad.8gtht76sp ; paper: Serio C, Brown RP, Clauss M, Meloro C (2024) *Palaeontology* 67(4): e12720, https://doi.org/10.1111/pala.12720 (CC BY 4.0). Still blocked in Pass 2 (the Dryad API needs a token); see "Not found".
