# R7-2E Chains: snakes, eels and fish (Q22-Q26)

Round 7 library research. **Access date for every source in this file: 2026-11-24.** Values marked **computed** were worked out by me from published numbers. SVL = snout-vent length; SL = standard length; BL = body length.

## Sources used (licence and access)

| Key | Citation | Free to read? | Licence | Read |
|---|---|---|---|---|
| S1 | Szyndlar Z, Georgalis GL (2023). "An illustrated atlas of the vertebral morphology of extant non-caenophidian snakes, with special emphasis on the cloacal and caudal portions of the column." *Vertebrate Zoology* 73: 717-886. doi:10.3897/vz.73.e101372 | Yes | **CC BY 4.0** | Full text (Pensoft XML), "Material examined" and vertebral-count paragraphs |
| S2 | Shine R, Shine TG, Brown GP, Udyawer V (2023). "Sexual dimorphism in aipysurine sea snakes (Elapidae, Hydrophiinae)." *Royal Society Open Science* 10(12): 231261. doi:10.1098/rsos.231261. PMC10716647 | Yes | **CC BY 4.0** | Full text; values only in figures and residuals, so used for method and n only |
| S3 | Kara C, Alp A, Gürlek ME (2011). "Morphological variations of the trouts (*Salmo trutta* and *Salmo platycephalus*) in the rivers of Ceyhan, Seyhan and Euphrates, Turkey." *Turkish Journal of Fisheries and Aquatic Sciences* 11: 77-85. doi:10.4194/trjfas.2011.0111 | Yes (journal PDF) | Not stated in the PDF | Full text, Tables 1 and 4 |
| S4 | Hu DL, Nirody J, Scott T, Shelley MJ (2009). "The mechanics of slithering locomotion." *PNAS* 106(25): 10081-10085. doi:10.1073/pnas.0812533106. PMC2700932 | Yes (PMC) | © National Academy of Sciences; licence not checked | Full text (PMC article page) |
| DS3 | Di Santo et al. 2021 fish body-wave data set | see R7-00 | see R7-00 | already reported in **R7-00-dryad-DS1-DS3.md** (section DS3; per-species amplitude envelopes) |

## Q22. Girth along the body (snakes and eels)
Not found as numbers. S2 measured mid-body girth (as a diameter, with ruler and flexible tape), head and tail length and width, and SVL on 419 preserved specimens of six sea-snake species (*Aipysurus duboisii* 71, *A. fuscus* 29, *A. foliosquama* 29, *A. mosaicus* 108, *A. laevis* 152, *Emydocephalus annulatus* 30; S2 Table 1), but reports them only as regressions, residuals and figures. It is a lead for a future data request; its data are not in the text.

## Q23. Vertebra counts (snakes)

Format in S1: total = trunk + cloacal + caudal vertebrae, one skeleton per line (museum number given). "+" after a number means some vertebrae were missing.

| Q | Species | Specimen | Total | Trunk + cloacal + caudal | Note | Source |
|---|---|---|---|---|---|---|
| Q23 | Ball python *Python regius* | SMF PH 28 | 239 | 205 + 4 + 30 | | S1 |
| Q23 | *Python regius* | SMF PH 29 | 239 | 206 + 4 + 29 | | S1 |
| Q23 | *Python regius* | SMF PH 49 | 242 | 206 + 4 + 32 | rearmost caudals fused | S1 |
| Q23 | Indian python *Python molurus* | SMF PH 184 | 349 | 272 + 4 + 73 | rearmost caudals fused | S1 |
| Q23 | *Python molurus* | ISEZ R/460 | 342 | 272 + 4 + 66 | | S1 |
| Q23 | Reticulated python *Malayopython reticulatus* | ISEZ R/436 (juvenile) | 413 | 312 + 3 + 98 | | S1 |
| Q23 | *Malayopython reticulatus* (literature) | | | 269-316 trunk + 92-102 cloacal and caudal | Nopcsa 1923, as cited in S1 | S1 |
| Q23 | Green anaconda *Eunectes murinus* | ZFMK 5179 | 313 | 252 + 4 + 57 | includes a final fusion | S1 |
| Q23 | *Eunectes murinus* (literature) | | | e.g. 252 trunk + 59 cloacal and caudal (Polly et al. 2001); 247-259 + 68-72 (Nopcsa 1923) | as cited in S1 | S1 |
| Q23 | Boa *Boa constrictor* (literature) | | 302-310 (Parmley & Reed 2003); 305 with 60 cloacal and caudal (Owen 1850, 1877) | 228 trunk and cloacal + 56 caudal (J. Shi, personal communication) | as cited in S1 | S1 |
| Q23 | Javelin sand boa *Eryx jaculus* | NHMUK 1920.1.20.1526 | 193 | 160 + 4 + 29 | includes a final fusion | S1 |

Tail share of the vertebral column (caudal / total, **computed**): *P. regius* 0.13 (30/239), *P. molurus* 0.21 (73/349), *M. reticulatus* juvenile 0.24 (98/413), *E. murinus* 0.18 (57/313), *E. jaculus* 0.15 (29/193). These are shares of vertebra counts, not of length.

S1 covers non-caenophidian snakes only (blind snakes, pythons, boas, sand boas and relatives), so it has no colubrids, vipers, elapids or sea snakes.

## Q24. Fin origins as a share of body length (fish)

S3, brown trout *Salmo trutta*, wild fish from five Turkish streams (Zamantı is in the Seyhan system; the others are in the Ceyhan and Euphrates systems). Measured to the nearest 0.1 mm. Predorsal distance (PD) = snout tip to the origin of the dorsal fin; dorsal fin length (DFL) = dorsal fin origin to the end of the fin; SL = snout tip to the end of the caudal peduncle. Values are the mean, with the minimum and maximum, as % of SL (S3 Table 4). n = fish per stream (S3 Table 1).

| Q | Stream | n | SL mean (min-max), mm | PD, % SL mean (min-max) | DFL, % SL mean (min-max) | Dorsal fin end, % SL (**computed**: PD + DFL means) | Source |
|---|---|---|---|---|---|---|---|
| Q24 | Zamantı | 15 | 229.83 (85.9-407.11) | 47.14 (42.75-72.00) | 13.13 (11.85-14.44) | 60.27 | S3 Tables 1 and 4 |
| Q24 | Göksu | 12 | 156.12 (119.22-197.84) | 44.96 (42.69-47.47) | 13.79 (11.83-14.87) | 58.75 | S3 Tables 1 and 4 |
| Q24 | Törbüzek (Terbüzek in Table 1) | 19 | 164.74 (130.61-238.46) | 46.15 (41.63-49.00) | 13.34 (11.07-14.97) | 59.49 | S3 Tables 1 and 4 |
| Q24 | Kömür | 10 | 150.67 (89.97-178.16) | 47.63 (45.18-54.26) | 13.89 (13.02-14.86) | 61.52 | S3 Tables 1 and 4 |
| Q24 | Aksu | 9 | 186.63 (156.9-245.3) | 44.57 (42.55-48.01) | 13.81 (13.17-14.34) | 58.38 | S3 Tables 1 and 4 |
| Q24 | Hurman | 17 | 150.93 (120.02-224.82) | 47.02 (44.83-50.59) | 12.90 (11.75-14.67) | 59.92 | S3 Tables 1 and 4 |

Other trout proportions in S3 Table 4 (head length about 24.5-26.2% SL, body depth about 24.5-26.6% SL) are not copied here.

## Q25. Wavelength and amplitude per species

| Q | Species | n | Value | Source |
|---|---|---|---|---|
| Q25 | Fish (several species) | see R7-00 | per-species amplitude envelopes along the body | DS3 in R7-00 (cross-reference; not repeated here) |
| Q25 | Pueblan milk snake *Lampropeltis triangulum campbelli* (juveniles, slithering on a flat surface) | n = 20 digitised runs | dimensionless wavelength 0.4 ± 0.1 and amplitude A = 0.1 ± 0.03, both scaled by body length | S4 Results |

## Q26. Snake undulation speed in body lengths per second

| Q | Species | n | Published value | In BL/s (**computed**) | Source |
|---|---|---|---|---|---|
| Q26 | Pueblan milk snake, ten juveniles, mass 15.2 ± 2.6 g, length 36 ± 2.3 cm (mean ± SE) | 20 runs | forward speed Ū = 0.22 ± 0.08, made dimensionless by L/τ, with L = 30 cm and an undulation period τ = 2 s | 0.22 × (1 BL / 2 s) = **about 0.11 BL/s** (about 3.3 cm/s) | S4 |
| Q26 | Red racer (S4's ref 24; secondary; species name not given in S4) | not given | L = 60-130 cm, U = 130 cm/s | **about 1.0-2.2 BL/s** | S4 Discussion |

## Not found (this file)
- **Q22:** girth (or diameter) at points along the body for any snake or eel, as numbers. S2 measured it but gives no table.
- **Q23:** head length and tail length vs SVL as numbers (S2 gives them only as residuals and figures); vertebra counts for colubrids, vipers, elapids and sea snakes (S1 excludes them; Meik & Pires-daSilva 2009, *BMC Evolutionary Biology* 9: 35, uses scale counts as stand-ins and was not mined); eel vertebra counts.
- **Q24:** fin origins for any species other than brown trout (pelvic, anal and pectoral fin origins are also missing for trout).
- **Q25:** wavelength for eels and fish beyond DS3; snake data beyond one milk-snake study.
- **Q26:** primary BL/s values for other snakes (Jayne 1986 is paywalled); sea-krait swimming speeds were seen only in a search extract and are not used.

## Sources disagree (this file)
- ***Python molurus* vertebra count:** S1's two skeletons have 342 and 349 vertebrae, but S1 also cites 324-328 in total (Parmley & Reed 2003) and 259 trunk + 68 cloacal and caudal (Polly et al. 2001) for the species. The counts vary between individuals and sources by up to about 25 vertebrae.
- ***Boa constrictor*:** 302-310 in total (Parmley & Reed 2003) vs 228 + 56 = 284 (Shi, personal communication, **computed** sum). S1 itself marks one older count (Rochebrune 1881) as "apparently erroneous".
- **Trout predorsal distance:** the Zamantı maximum of 72.00% SL is far outside every other stream's range (41.6-54.3%). It may be a typo or an outlier; treat the Zamantı mean (47.14) with care.
- **Milk snake speed:** an earlier search extract described the milk snake speed as "0.22 BL/s"; the full text shows 0.22 is in units of body length per **undulation period** (2 s), so the speed is about 0.11 BL/s (**computed**). The earlier reading was wrong.
