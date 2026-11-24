# R7-00 The three Dryad data sets (DS1, DS2, DS3): access, contents and values

Round 7 library research. Access date for every page, API call and file in this file: **2026-11-24** (checked from a Linux box with curl and a headless browser; no sign-in, no account created, no captcha solved).

## How access was tested (applies to DS1 to DS3)

| Test | Result on 2026-11-24 |
|---|---|
| Dryad API metadata, `GET https://datadryad.org/api/v2/datasets/doi%3A...` | HTTP 200, no token. Gives title, licence, storageSize, related works. |
| Dryad API file list, `GET /api/v2/versions/{id}/files` | HTTP 200, no token. Gives each file's name, size and MIME type. |
| Dryad API file bytes, `GET /api/v2/files/{id}/download` and `/datasets/{doi}/download` | **HTTP 401** `{"error":"Unauthorized, must have current bearer token"}` |
| Dryad web download link from the landing page, `/downloads/file_stream/{id}` | **HTTP 403** to a script. The landing page's own code shows an AWS WAF "Confirm you are human" captcha before the zip is served (seen in the page source). |
| Dryad's own API description, `https://datadryad.org/openapi.yml` | Says: "Anonymous users of the API are limited to 30 requests per minute, and are **not allowed to download data files**." All download routes are marked `bearerAuth`. |
| Dryad's API account documentation (github.com/datadryad/dryad-app, `documentation/apis/api_accounts.md`) | A token needs a Dryad API account; a Dryad account is made "using an ORCID login"; then "create an API account ... on your My account page". Tokens "last 10 hours". |
| Dryad login page `https://datadryad.org/sessions/choose_login` | "Dryad uses ORCID as its primary login method. Please use, or create, your ORCID login credentials." |

**So, for all three data sets:** (a) free, no fee; (c) a human in a normal browser can download after a "Confirm you are human" check (no sign-in was seen on that path, but the check stopped the script, so I could not confirm the whole browser path end to end); a script or the API needs a **free ORCID iD -> free Dryad account -> free Dryad API account -> 10-hour token**. ORCID and Dryad accounts are free for anyone. I did not create any account (rule of this task). The earlier round's 401 and 403 are confirmed, and the reason is now known: it is Dryad policy, not a fault.

---

## DS1. Serio et al. 2024, limb bones and skull of living mammals

| Item | Answer | Source |
|---|---|---|
| Paper | Serio C, Brown RP, Clauss M, Meloro C (2024). "Morphological disparity of mammalian limb bones throughout the Cenozoic: the role of biotic and abiotic factors." *Palaeontology* 67(4): e12720. doi:10.1111/pala.12720 | Crossref record (title now found; it was missing from the project's files) |
| Paper licence | **CC BY 4.0** (Crossref licence field: creativecommons.org/licenses/by/4.0/). Free to read. Confirmed. | Crossref API |
| (a) Free? | Yes, no fee. | Dryad landing page |
| (b) Open, licence | **CC0 1.0**: API `license` = `https://spdx.org/licenses/CC0-1.0.html`; landing page shows "CC0" / "Creative Commons Zero v1.0 Universal". **Confirmed.** | Dryad API + landing page |
| (c) Sign-in? | As above: API 401, web 403 + human check. Token route = free ORCID-based Dryad API account. | |
| (d) Size | **7 files, 26,144,481 bytes (about 26.1 MB) in total**, version 7 (last modified 2025-02-03): Data_1.csv 713,505 B; Data_2.csv 166,118 B; Data_3.csv 22,226 B; Data_4.csv 522 B; Data_5.csv 583 B; Data_6.zip 25,237,827 B; README.md 3,700 B. Formats CSV, ZIP, Markdown. | API `/versions/343455/files` |
| (e) Mirror? | **None found.** Zenodo search for "dryad.8gtht76sp" and for the title gave nothing; the paper links only to Dryad. | Zenodo API search |
| (f) What it holds | From the README shown on the landing page: **Data_1 (raw data)** has columns Species, Locality, Cat (museum catalogue number), Humerus (mm), Radius (mm), Femur (mm), Tibia (mm), Brachial index (R/H), Crural index (T/F), Greatest skull length (mm), Note, Source. So it is **specimen-level rows with a catalogue number and a source reference**, not one value a species. From the Dryad methods text: assembled "from the literature and published databases": **793 humerus, 805 radius, 668 femur and 663 tibia lengths** for **246 living and 177 fossil species**, plus **1,883 greatest skull length values**; orders: Artiodactyla 206, Carnivora 178, Perissodactyla 59 species, plus small fossil orders. Data_2 holds phylogeny, diet, substrate and habitat categories with references; Data_3 to 5 are node ages and time bins; Data_6 disparity results. **No metapodials, no body mass column** (mass is not in the README column list). Rabbits and rats (Lagomorpha, Rodentia) and elephants (Proboscidea) are **not among the orders listed**, so those quadruped-set species are almost certainly absent. | Dryad landing page README and methods |
| (g) Open alternative | The data are compiled from published sources, so the per-species values can be rebuilt from the "Source" column's references once the file is read. For the bones the plan wants, open alternatives found this round: Humphries et al. 2020 (PLOS ONE, CC BY) gives Labrador and German shepherd humerus, radius and tibia as % of withers height (see file R7-2A, Q1/Q3 rows). No single open file with the same 246-species coverage was found. | |
| (h) Values | **Not read**: the file needs the token or a browser download. | |

**What a human would do:** open https://datadryad.org/dataset/doi:10.5061/dryad.8gtht76sp in a browser, click Data_1.csv (713 KB), pass the "confirm you are human" check, and put the file in the box; I can then extract the quadruped-set rows with n, mean and SD.

---

## DS2. Skelevision, Weeks et al. 2025, bird skeletons

| Item | Answer | Source |
|---|---|---|
| Paper | Weeks BC, Zhou Z, Probst CM, Berv JS, O'Brien BK, Benz BW, Skeen HR, Ziebell M, Bodt L, Fouhey DF (2025). "Skeletal trait measurements for thousands of bird species." *Scientific Data* **12: 884**. doi:10.1038/s41597-025-05234-y | Crossref |
| Paper licence | **CC BY-NC-ND 4.0** (Crossref licence field). Free to read; non-commercial, no derivatives. Confirmed. | Crossref |
| (a) Free? | Yes. | |
| (b) Licence | **CC0 1.0** (API `license` field and landing page). **Confirmed.** | Dryad API |
| (c) Sign-in? | As for all Dryad sets (401 / 403 + human check). | |
| (d) Size | **4 files, 4,982,436 bytes (about 5.0 MB)**, version 4 of 2025-11-05: Complete_Trait_Dataset_v1.csv 2,148,343 B; Skelevision_Only_Dataset_v1.csv 1,311,777 B; Skelevision_species_complete_v1.1.csv 1,511,749 B; README.md 10,567 B. | API `/versions/405499/files` |
| (e) Mirror? | **Yes, a partial open mirror on GitHub**: repository `bcweeks/Trait-Imputation` (licence **GPL-3.0**, file LICENSE) holds `individual_data.mvBM.nomass.12.22.23.RDS` (2,242,690 B) with three tables of 14,419 specimens x 14 columns (`filtered_dataset` = measured only, `imputed_dataset`, `completed_dataset`) and `species_data.mvBM.nomass.12.22.23.RDS` (2,057 species: means, SE, variance, 95% CI). Values in these RDS files are **natural logs of mm** (checked: exp of the stored value gives plausible mm). Dated 22 Dec 2023, so it may differ slightly from Dryad v1 (Nov 2025). No sign-in needed (raw.githubusercontent.com). Note GPL-3.0 on the repo, CC0 on Dryad: prefer the Dryad copy for redistribution. Stanford DataWorks has a catalogue record only (no files). | GitHub API, files read 2026-11-24 |
| (f) What it holds | **Passerines only**: abstract says "2,057 species of birds (Aves: Passeriformes) ... 14,419 individuals". So no pigeon, gull, hawk, owl, heron, duck or chicken. 12 traits, all lengths in mm: **tibiotarsus (column named `tarsus`, the README says "Length of the tibiotarsus")**, ulna, skull (bill tip to back of skull, `cv.skull.1`), 2nd digit 1st phalanx, sclerotic ring outer diameter, **tarsometatarsus (`metatarsus`)**, keel, humerus, furcula, **femur**, carpometacarpus, radius. **It does hold all three leg bones.** No body mass (the file names say "nomass"). | Dryad landing page README; RDS columns |
| (g) Open alternative | The GitHub mirror above. | |
| (h) Values | Below, from the GitHub mirror, measured-only table. | |

**DS2/Q12 values: five-plus passerines of different sizes.** File `individual_data.mvBM.nomass.12.22.23.RDS`, table `filtered_dataset` (Skelevision measurements with high confidence only, no imputed values), repo bcweeks/Trait-Imputation, read 2026-11-24. Values are **computed** by me: exp() of each stored ln(mm) value, then arithmetic mean ± SD (sample SD) over the specimens of that species that have the bone measured; n = specimens with that bone. Measured on photographs of museum skeletons (dry bone) by a computer-vision pipeline; paired bones are the mean of left and right where both were confident (README). Machine error against hand measurement (paper Table 1, from round 6): humerus 0.47 mm, ulna 0.96 mm, carpometacarpus 1.11 mm RMSE. Sex not given.

| Q | Species (common name) | n specimens in file | femur | tibiotarsus ("tarsus") | tarsometatarsus | humerus | ulna | carpometacarpus | skull+bill | keel |
|---|---|---|---|---|---|---|---|---|---|---|
| DS2/Q12 | *Troglodytes aedon* (house wren) | 38 | 14.6 ± 0.8 (n=35) | 23.9 ± 1.9 (n=36) | 17.5 ± 1.7 (n=28) | 14.0 ± 0.8 (n=32) | 14.4 ± 0.8 (n=12) | 8.7 ± 0.5 (n=17) | 28.9 ± 2.0 (n=36) | 12.1 ± 1.2 (n=25) |
| DS2/Q12 | *Passer domesticus* (house sparrow) | 60 | 17.7 ± 0.9 (n=55) | 28.2 ± 1.3 (n=57) | 18.8 ± 1.2 (n=49) | 18.3 ± 0.7 (n=56) | 20.2 ± 1.1 (n=44) | 12.0 ± 0.6 (n=43) | 28.4 ± 1.4 (n=60) | 19.8 ± 1.9 (n=57) |
| DS2/Q12 | *Turdus migratorius* (American robin) | 85 | 27.0 ± 1.4 (n=70) | 46.0 ± 4.0 (n=71) | 33.6 ± 2.4 (n=63) | 28.6 ± 1.5 (n=67) | 35.2 ± 2.8 (n=58) | 21.1 ± 1.1 (n=48) | 44.8 ± 4.1 (n=78) | 31.2 ± 3.5 (n=65) |
| DS2/Q12 | *Sturnus vulgaris* (common starling) | 63 | 25.1 ± 0.8 (n=58) | 44.2 ± 1.3 (n=60) | 29.4 ± 2.1 (n=54) | 27.3 ± 1.4 (n=57) | 32.5 ± 1.5 (n=53) | 20.3 ± 0.8 (n=51) | 48.0 ± 2.6 (n=62) | 31.9 ± 2.5 (n=61) |
| DS2/Q12 | *Cyanocitta cristata* (blue jay) | 34 | 29.6 ± 2.1 (n=29) | 48.5 ± 2.2 (n=31) | 35.1 ± 2.2 (n=23) | 32.0 ± 1.1 (n=33) | 37.1 ± 1.9 (n=24) | 20.9 ± 0.9 (n=24) | 53.3 ± 1.9 (n=32) | 27.0 ± 2.4 (n=29) |
| DS2/Q12 | *Corvus brachyrhynchos* (American crow) | 39 | 50.9 ± 3.9 (n=37) | 85.2 ± 6.7 (n=35) | 57.9 ± 4.3 (n=23) | 64.8 ± 3.0 (n=36) | 76.4 ± 6.5 (n=36) | 47.2 ± 2.9 (n=37) | 85.4 ± 4.5 (n=38) | 52.8 ± 3.8 (n=31) |
| DS2/Q12 | *Corvus corax* (common raven) | 15 | 62.9 ± 16.4 (n=13) | 104.6 ± 28.8 (n=12) | 63.9 ± 4.3 (n=7) | 87.4 ± 23.6 (n=14) | 111.5 ± 8.4 (n=13) | 66.0 ± 4.6 (n=12) | 108.6 ± 25.2 (n=14) | 68.3 ± 12.0 (n=11) |

Caution, *Corvus corax*: the SDs for femur, tibiotarsus, humerus and skull (16 to 29 mm) are far larger than for the crow and suggest some mis-measured or mis-identified specimens in the raw machine data; the tarsometatarsus, ulna and carpometacarpus SDs are normal. Use with care, or use the species-level model means instead. Medians for *C. corax* (**computed**, same specimens): femur 66.5, tibiotarsus 113.5, tarsometatarsus 63.6, humerus 93.0, ulna 108.6, carpometacarpus 66.2, skull+bill 112.7, keel 71.2 mm.

---

## DS3. Di Santo et al. 2021, the body wave of swimming fish

| Item | Answer | Source |
|---|---|---|
| Paper | Di Santo V, Goerig E, Wainwright DK, Akanyeti O, Liao JC, Castro-Santos T, Lauder GV (2021). "Convergence of undulatory swimming kinematics across a diversity of fishes." *PNAS* 118(49): e2113206118. doi:10.1073/pnas.2113206118. PMC8670443. | PMC page |
| Paper licence | "Published under the PNAS license" (Crossref licence URL pnas.org/site/aboutpnas/licenses.xhtml). Free to read at PMC; Europe PMC says "Article ... is not open access one" (not in the OA re-use subset). **Not a Creative Commons licence.** Confirmed. | PMC, Europe PMC, Crossref |
| What "Dataset S2" is | Part of the **PNAS supplementary information**: PMC lists `pnas.2113206118.sd01.xlsx` (25.4 KB, Dataset S1, species list) and `pnas.2113206118.sd02.xlsx` (57 KB, Dataset S2, "species-specific kinematic parameters" per the paper text), plus the SI appendix PDF (987.3 KB). **Different from the Dryad deposit.** | PMC article page |
| Can Dataset S2 be had? | Free to a person in a browser. To a script: PMC answered with a proof-of-work page, then a **Google reCAPTCHA** page; pnas.org and Europe PMC answered 403. I did not solve captchas. So: free, no account, but a human must click. | tested 2026-11-24 |
| Dryad deposit | doi:10.5061/dryad.bg79cnp9x, "Convergence of undulatory swimming kinematics across a diversity of fishes", published 2021-11-04. | Dryad API |
| (a) Free? | Yes. | |
| (b) Licence | **CC0 1.0** (API licence field). **Confirmed.** | Dryad API |
| (c) Sign-in? | As for all Dryad sets. | |
| (d) Size | **11 files, 77,531,646 bytes (about 77.5 MB)**: MidlinesAdj.csv 77,085,156 B (the bulk: digitised midlines); Megakine.csv 66,507 B; elsa_phaseEnv.csv 186,040 B; elsa_clipID.csv 491 B; resultsMeansSE.csv 34,689 B; MegakineMorpho.xlsx 91,029 B; Multi-species_spreadsheet.xlsx 36,735 B; Swimmingmode.xlsx 11,553 B; Megakinematic_dataset_column_descriptions.docx 17,309 B; MegakineTree.newick 1,281 B; README.txt 856 B. | API `/versions/148273/files` |
| (e) Mirror? | **Yes, partial, on Zenodo, no sign-in:** (1) Zenodo 10.5281/zenodo.4623882 (licence **AFL-3.0**, the "software" record linked from Dryad): CurveMapper6.m, Analysis.Rmd, Kinematics.rmd, polyfitDat.m, CurveMapper5.doc and **dat.mat (229,926 B)**, which holds, for **151 clips of 44 species**, the species name, swimming mode and the **amplitude envelope at 200 points along the body**. Downloaded without any account. (2) Zenodo 10.5281/zenodo.5114877 (CC BY 4.0): one file, Curvature_Equations.pdf. Wavelength and tail-beat frequency per species are **not** in the Zenodo files; they are in the Dryad files (Megakine.csv, resultsMeansSE.csv, by the code's column names `TBF`, `TL`, `speedflowBL`) and in Dataset S2. | Zenodo API |
| (f) What it holds | 44 species (3 jawless, 3 cartilaginous, 37 bony, plus amphioxus, per the paper); midlines at 200 points; amplitude and phase envelopes; tail-beat frequency; swim speed in BL/s; total length; morphology; swimming mode (codes in dat.mat: a, sc, c, t = anguilliform, subcarangiform, carangiform, thunniform). | paper, Kinematics.rmd, dat.mat |
| (g) Open alternative | dat.mat on Zenodo (amplitude only), and the paper's text values for the four model species. | |
| (h) Values | Amplitude envelope per species below (from dat.mat). Wavelength, frequency and speed per species: **not read** (need the Dryad files or Dataset S2). | |

**Amplitude convention (from the authors' own code and text):** `Kinematics.rmd` computes `amplitude = maxy - miny` per body point over a tail beat (**peak-to-peak**) and `HalfAmp = amplitude/2`; the paper says the envelopes model "how **peak-to-peak** amplitude, y, varies along the body". The plots in the code label amplitude "Amplitude (BL)", so units are **body lengths** (inferred from the code's axis label; the .mat file has no unit field). Speed for each clip is **not** stored in dat.mat, so these values pool all speeds recorded for that species.

**Paper text values (PMC8670443, Results, "Kinematics of the Four Canonical Species"), at about 1 BL/s, n = 5 sequences each:** mean body wavelength eel 0.58 BL; trout 1.00 BL; mackerel 0.96 BL; tuna 1.17 BL (Q25). No spread given in that sentence.

**DS3/Q25 per-species amplitude envelope**, file dat.mat (Zenodo 10.5281/zenodo.4623882, AFL-3.0), field `dat.ampenv` (151 x 200), grouped by `dat.species` (I merged name variants with a space or underscore, e.g. "Anguilla rostrata" and "Anguilla_rostrata"). Point 1 = head tip, point 100 = mid-body, point 200 = tail tip. Peak-to-peak amplitude in body lengths, **mean ± SD over clips (computed by me)**, n = clips. Head:tail = mean of per-clip ratios (**computed**). Raw values, without the 20-point smoothing the authors apply in polyfitDat.m.

| Q | Species | Mode (dat.swimmingmode) | n clips | Head amp (pt 1/200) mean ± SD | Mid-body (pt 100/200) mean ± SD | Tail tip (pt 200/200) mean ± SD | Head:tail (computed) |
|---|---|---|---|---|---|---|---|
| DS3/Q25 | *Acipenser brevirostrum* | a | 3 | 0.039 ± 0.029 | 0.059 ± 0.025 | 0.203 ± 0.053 | 0.18 |
| DS3/Q25 | *Alosa sapidissima* | sc | 4 | 0.038 ± 0.011 | 0.044 ± 0.009 | 0.226 ± 0.035 | 0.17 |
| DS3/Q25 | *Anguilla rostrata* | a | 10 | 0.030 ± 0.014 | 0.061 ± 0.018 | 0.157 ± 0.022 | 0.19 |
| DS3/Q25 | *Archosargus probatocephalus* | sc | 2 | 0.034 ± 0.006 | 0.041 ± 0.004 | 0.203 ± 0.001 | 0.17 |
| DS3/Q25 | *Branchiostoma lanceolatum* | a | 4 | 0.277 ± 0.056 | 0.261 ± 0.033 | 0.413 ± 0.074 | 0.68 |
| DS3/Q25 | *Caranx hippos* | c | 7 | 0.036 ± 0.008 | 0.038 ± 0.005 | 0.165 ± 0.049 | 0.22 |
| DS3/Q25 | *Catostomus commersonii* | sc | 4 | 0.078 ± 0.029 | 0.076 ± 0.009 | 0.199 ± 0.021 | 0.39 |
| DS3/Q25 | *Chaetodipterus faber* | sc | 1 | 0.024 (n=1) | 0.054 (n=1) | 0.178 (n=1) | 0.14 |
| DS3/Q25 | *Chitala ornata* | a | 3 | 0.025 ± 0.013 | 0.039 ± 0.017 | 0.124 ± 0.012 | 0.20 |
| DS3/Q25 | *Danio rerio* | sc | 1 | 0.044 (n=1) | 0.098 (n=1) | 0.243 (n=1) | 0.18 |
| DS3/Q25 | *Decapterus macarellus* | c | 5 | 0.027 ± 0.008 | 0.029 ± 0.005 | 0.202 ± 0.013 | 0.13 |
| DS3/Q25 | *Devario aequipinnatus* | sc | 6 | 0.071 ± 0.038 | 0.086 ± 0.051 | 0.240 ± 0.093 | 0.29 |
| DS3/Q25 | *Eptatretus stoutii* | a | 1 | 0.115 (n=1) | 0.141 (n=1) | 0.301 (n=1) | 0.38 |
| DS3/Q25 | *Esox lucius* | sc | 3 | 0.029 ± 0.013 | 0.044 ± 0.008 | 0.203 ± 0.020 | 0.14 |
| DS3/Q25 | *Haemulon aurolineatum* | sc | 4 | 0.042 ± 0.015 | 0.042 ± 0.014 | 0.200 ± 0.034 | 0.21 |
| DS3/Q25 | *Leiostomus xanthurus* | sc | 3 | 0.058 ± 0.014 | 0.066 ± 0.013 | 0.235 ± 0.018 | 0.25 |
| DS3/Q25 | *Lepisosteus platyrhincus* | sc | 2 | 0.038 ± 0.021 | 0.031 ± 0.009 | 0.146 ± 0.029 | 0.25 |
| DS3/Q25 | *Lepomis macrochirus* | sc | 2 | 0.012 ± 0.005 | 0.019 ± 0.011 | 0.114 ± 0.044 | 0.11 |
| DS3/Q25 | *Lutjanus griseus* | sc | 3 | 0.067 ± 0.014 | 0.054 ± 0.011 | 0.171 ± 0.004 | 0.39 |
| DS3/Q25 | *Megalops cyprinoides* | sc | 4 | 0.045 ± 0.015 | 0.044 ± 0.009 | 0.172 ± 0.028 | 0.27 |
| DS3/Q25 | *Menticirrhus americanus* | sc | 3 | 0.052 ± 0.001 | 0.052 ± 0.003 | 0.196 ± 0.019 | 0.27 |
| DS3/Q25 | *Micropterus dolomieu* | sc | 3 | 0.069 ± 0.022 | 0.072 ± 0.028 | 0.215 ± 0.031 | 0.32 |
| DS3/Q25 | *Micropterus salmoides* | sc | 2 | 0.009 ± 0.002 | 0.023 ± 0.003 | 0.136 ± 0.020 | 0.07 |
| DS3/Q25 | *Mugil cephalus* | sc | 2 | 0.074 ± 0.006 | 0.059 ± 0.011 | 0.212 ± 0.046 | 0.36 |
| DS3/Q25 | *Myxine glutinosa* | a | 2 | 0.178 ± 0.045 | 0.212 ± 0.042 | 0.383 ± 0.104 | 0.47 |
| DS3/Q25 | *Myxocyprinus asiaticus* | sc | 4 | 0.026 ± 0.011 | 0.035 ± 0.011 | 0.131 ± 0.032 | 0.20 |
| DS3/Q25 | *Noturus insignis* | a | 3 | 0.038 ± 0.009 | 0.054 ± 0.009 | 0.253 ± 0.031 | 0.15 |
| DS3/Q25 | *Oncorhynchus mykiss* | sc | 4 | 0.024 ± 0.003 | 0.032 ± 0.007 | 0.161 ± 0.018 | 0.15 |
| DS3/Q25 | *Perca flavescens* | sc | 2 | 0.006 ± 0.005 | 0.015 ± 0.010 | 0.108 ± 0.025 | 0.05 |
| DS3/Q25 | *Petromyzon marinus* | a | 3 | 0.030 ± 0.017 | 0.081 ± 0.025 | 0.196 ± 0.062 | 0.15 |
| DS3/Q25 | *Polypterus senegalus* | sc | 3 | 0.013 ± 0.011 | 0.050 ± 0.034 | 0.221 ± 0.079 | 0.05 |
| DS3/Q25 | *Pomatomus saltatrix* | c | 2 | 0.052 ± 0.005 | 0.047 ± 0.007 | 0.219 ± 0.003 | 0.24 |
| DS3/Q25 | *Protopterus annectens* | a | 1 | 0.009 (n=1) | 0.027 (n=1) | 0.187 (n=1) | 0.05 |
| DS3/Q25 | *Salmo salar* | sc | 2 | 0.045 ± 0.018 | 0.055 ± 0.001 | 0.182 ± 0.012 | 0.25 |
| DS3/Q25 | *Salmo trutta* | sc | 3 | 0.060 ± 0.008 | 0.093 ± 0.005 | 0.241 ± 0.050 | 0.26 |
| DS3/Q25 | *Salvelinus fontinalis* | sc | 10 | 0.022 ± 0.009 | 0.038 ± 0.015 | 0.158 ± 0.024 | 0.14 |
| DS3/Q25 | *Sander vitreus* | sc | 1 | 0.073 (n=1) | 0.100 (n=1) | 0.195 (n=1) | 0.38 |
| DS3/Q25 | *Sardinella aurita* | sc | 2 | 0.054 ± 0.002 | 0.043 ± 0.008 | 0.217 ± 0.000 | 0.25 |
| DS3/Q25 | *Scomber scombrus* | c | 10 | 0.023 ± 0.016 | 0.046 ± 0.006 | 0.178 ± 0.028 | 0.14 |
| DS3/Q25 | *Scyliorhinus retifer* | a | 2 | 0.034 ± 0.004 | 0.062 ± 0.015 | 0.220 ± 0.020 | 0.15 |
| DS3/Q25 | *Sphyrna tiburo* | a | 1 | 0.081 (n=1) | 0.069 (n=1) | 0.234 (n=1) | 0.35 |
| DS3/Q25 | *Squalus acanthias* | a | 1 | 0.054 (n=1) | 0.092 (n=1) | 0.195 (n=1) | 0.28 |
| DS3/Q25 | *Synodus foetens* | sc | 3 | 0.077 ± 0.014 | 0.060 ± 0.021 | 0.275 ± 0.006 | 0.28 |
| DS3/Q25 | *Thunnus albacares* | t | 10 | 0.044 ± 0.015 | 0.057 ± 0.021 | 0.178 ± 0.027 | 0.25 |

---

## Not found (this file)
- DS1 values (all): file not readable without a token or a human browser download.
- DS2 birds outside Passeriformes: not in the data set at all.
- DS2 body mass: not in the data set.
- DS3 per-species wavelength, tail-beat frequency, speed, body length: in the Dryad files and in Dataset S2, neither readable by script.

## Sources disagree (this file)
- DS2: the Dryad column `tarsus` is the **tibiotarsus** (README), not the tarsometatarsus (`metatarsus`). Anyone reading the CSV by column name will swap them.
- DS2: the paper and the earlier round call it "passerine skeletons" (true); the brief asked whether non-passerines are included: they are not.
- DS1: the Dryad methods give 246 living species "from the literature", so values are compiled, not newly measured; skull length has 1,883 values against 663 to 805 for each limb bone, so many species will lack one or more bones.
