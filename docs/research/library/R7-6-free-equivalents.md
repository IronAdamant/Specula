# R7-6: free equivalents for the sources that stayed blocked (round 7, third pass)

Compiled 2026-11-24. Access date for every source: 2026-11-24. Files are in `sources/equivalents-r7c/`. Every PDF listed here was downloaded by script and starts with `%PDF-`; data files came from Zenodo (CC0) or the PMC open-access bucket (`pmc-oa-opendata`). No sign-ins, payments or pirate sites. A few pages returned 403 to scripts and were dropped rather than used (listed under "Tried and refused" at the end). Cruse 1976 and the Castillo-Figueroa s001 supplement are handled by another worker and are not covered here.

"Computed" means I worked the value out from the raw data file named. The script output is saved beside the data, so it can be re-run.

## Summary, one line per original

| Original (blocked) | Free equivalent used | Status |
|---|---|---|
| Fieler & Jayne 1998 (JEB) | Dick & Clemente 2016 *Front. Zool.* (Varanus limb segments, 30 lizards); Druelle et al. 2019 *Sci. Rep.* (lacertid per-stride DF against speed) | Substitute |
| Reilly & Elias 1998 (JEB) | Hutchinson et al. 2019 *Sci. Rep.* 9: 19302 (15 crocodylian species, per-stride DF and speed) | Substitute (DF yes, femoral adduction no) |
| Biancardi 2011 (JEB) | Weihmann 2013 *PLOS ONE* (Cupiennius); Silva-Pereyra et al. 2019 *PeerJ* (tarantula, incl. leg segment table) | Substitute |
| Kram 1997 (JEB) | Parle et al. 2016 *PLOS ONE* (Blaberus hind tibia only); Silva-Pereyra 2019 for spider legs | Partial |
| Tobalske 1999; Tobalske & Dial 1996 (JEB) | Deetjen et al. 2024 *eLife* (ringneck dove downstroke and upstroke durations) | Partial (one species) |
| Full & Tu 1991; Bender 2011 (JEB) | Weihmann et al. 2017 *Front. Zool.* (Nauphoeta cockroach gaits vs speed) | Substitute |
| McElroy 2008 (JEB) | lacertid Sci Rep 2019 data; Hutchinson 2019 crocodylians | Substitute (no Varanus duty factor) |
| Peterson & Hennessey 1978 (AMS) | US EPA-454/R-99-005 (2000) Table 6-2; NASA TM-79275 (Spera & Richards 1979) | Substitute |
| Idso 1975 (AMS) | NASA Technical Paper 2643 (1987), soil-moisture test on Sonora Desert sand | Partial |
| Tsvetsinskaya 2002 (Wiley/AGU) | NASA TP 2643 (1987) dry desert spectral albedos | Partial (no MODIS broadband) |
| Campione & Evans 2020 (Wiley) | Sellers et al. 2013 *PLOS ONE* (Argentinosaurus 83,230 kg); Campione & Evans 2012 *BMC Biol.* (equation) | Substitute |
| Hutchinson 2004 (RVC) | Sellers et al. 2017 *PeerJ* (T. rex top speed 7.7 m/s) | Substitute |
| Shine Dryad data | Zenodo 8317256, same DOI and filename (38,412 bytes, matching Dryad's file listing) | **Exact data** |
| Baliga figshare raw angles | none free (figshare 403 to scripts and to the fetch service; Harvey 2022 ROM data also on figshare) | None free |
| Kubo figshare | Zenodo 4970077 CarnData.csv (101 Carnivora, digitigrade 0/1) | Partial (Carnivora only) |
| Marek ESM S2 | Zenodo 10785541 (Marek, CC0): cervical counts for 112 bird species | Substitute |
| Willett AFM tyre data | Dijkman, TU Delft MSc thesis (road Crr of 26/28 mm racing tyres, 30 and 40 km/h) | Substitute |
| Green Seal 2003 | California Energy Commission 600-03-001CR (2003) Vol. II; UC Davis report (Lutsey et al.) | Substitute |
| Ford Raptor 2021 release | Ford's own 2021 F-150 Raptor Technical Specifications PDF (Ford Belgium press site) | **Exact maker spec** |

## Download URLs (every file was downloaded)
PDF links below were checked for `%PDF-`. For PMC sources the bucket PDF is the actual downloaded file; the PMC page gives the citation and licence. Zenodo links are XLSX or CSV, not PDFs. A re-check at about 22:00 got 200 for all of them, except that Zenodo refuses partial-range requests (use a plain GET).

| Row | Source | Direct free URL |
|---|---|---|
| 1A | Dick & Clemente 2016, main PDF | https://pmc-oa-opendata.s3.amazonaws.com/PMC4758084.1/PMC4758084.1.pdf |
| 1A | Individual lizard measurements | https://pmc-oa-opendata.s3.amazonaws.com/PMC4758084.1/12983_2016_141_MOESM2_ESM.pdf |
| 1B, 7 | Druelle et al. 2019 | https://pmc-oa-opendata.s3.amazonaws.com/PMC6856151.1/PMC6856151.1.pdf |
| 1B, 7 | Raw per-stride lacertid CSV | https://pmc-oa-opendata.s3.amazonaws.com/PMC6856151.1/41598_2019_53329_MOESM2_ESM.csv |
| 2 | Hutchinson et al. 2019 | https://pmc-oa-opendata.s3.amazonaws.com/PMC6917812.1/PMC6917812.1.pdf |
| 2 | Crocodylian per-stride XLSX | https://pmc-oa-opendata.s3.amazonaws.com/PMC6917812.1/41598_2019_55768_MOESM13_ESM.xlsx |
| 3A | Weihmann 2013 | https://journals.plos.org/plosone/article/file?id=10.1371/journal.pone.0065788&type=printable |
| 3B | Silva-Pereyra et al. 2019 | https://pmc-oa-opendata.s3.amazonaws.com/PMC6766366.1/PMC6766366.1.pdf |
| 4 | Parle et al. 2016 | https://pmc-oa-opendata.s3.amazonaws.com/PMC4972353.1/PMC4972353.1.pdf |
| 5 | Deetjen et al. 2024 | https://pmc-oa-opendata.s3.amazonaws.com/PMC10942624.1/PMC10942624.1.pdf |
| 6 | Weihmann et al. 2017 | https://pmc-oa-opendata.s3.amazonaws.com/PMC5719566.1/PMC5719566.1.pdf |
| 8 | EPA guidance | https://www.epa.gov/sites/default/files/2020-10/documents/mmgrma_0.pdf |
| 8 | NASA TM-79275 | https://ntrs.nasa.gov/api/citations/19800005367/downloads/19800005367.pdf |
| 9, 10 | NASA TP2643 | https://ntrs.nasa.gov/api/citations/19870012848/downloads/19870012848.pdf |
| 11A | Sellers et al. 2013 | https://journals.plos.org/plosone/article/file?id=10.1371/journal.pone.0078733&type=printable |
| 11B | Campione & Evans 2012 | https://pmc-oa-opendata.s3.amazonaws.com/PMC3403949.1/PMC3403949.1.pdf |
| 12 | Sellers et al. 2017 | https://pmc-oa-opendata.s3.amazonaws.com/PMC5518979.1/PMC5518979.1.pdf |
| 13 | Shine raw measurements XLSX (CC0) | https://zenodo.org/records/8317256/files/Aipysurine_sex_dimorphism_for_Dryad2.xlsx?download=1 |
| 15 | Carnivora foot-posture CSV (CC0) | https://zenodo.org/records/4970077/files/CarnData.csv?download=1 |
| 16 | Marek cervical counts CSV (CC0) | https://zenodo.org/records/10785541/files/svp2022data_v2.csv?download=1 |
| 16 | Original Marek specimen list (CC0) | https://zenodo.org/records/4515823/files/SpecimenInformation.xlsx?download=1 |
| 17 | Dijkman 2024 TU Delft thesis | https://repository.tudelft.nl/file/File_7fab07bf-99e2-4a68-9d86-e0430120941b |
| 18A | CEC 2003 Vol. II (public report, third-party host) | https://www.kannahconsulting.com/wp-content/uploads/2016/08/2003-01-31_600-03-001CRVOL2.pdf |
| 18B | UC Davis report (landing page; the direct PDF downloaded at 21:15 but now returns 403 to scripts) | https://escholarship.org/uc/item/06r0q71c |
| 19 | Ford 2021 Raptor specs | https://www.fordpresse.be/editor/files/2021_ford_f_150_raptor_tech_specs.pdf |

## Detailed rows

### 1. Fieler & Jayne 1998 → Q9, U7, U8
**Equivalent A:** Dick TJM, Clemente CJ 2016. How to build your dragon: scaling of muscle architecture from the world's smallest to the world's largest monitor lizard. *Frontiers in Zoology* 13: 8. doi:10.1186/s12983-016-0141-5. PMC4758084 (bucket). Files: `Q9_dick_clemente2016_varanus_FrontZool/` (Additional file 2 = `12983_2016_141_MOESM2_ESM.pdf`, 1 page).
- Why comparable: measured snout-vent, tail, thigh, crus and pes lengths of individual lizards, the same quantities Q9/U7 ask for, across 9 *Varanus* species from 7.6 g to 40 kg.
- Numbers (Additional file 2, table, one row per lizard; SVL, tail, thigh, crus, pes in mm):
  - *V. brevicauda* 7.6 g: 93.0, 80.0, 8.0, 8.8, 11.1
  - *V. gouldii* 429.4 g: 308.1, 542.1, 34.6, 45.3, 59.4
  - *V. panoptes* 4150.0 g: 590.0, 686.0, 72.0, 97.4, 82.8
  - *V. varius* 4025.2 g: 632.0, 892.0, 76.0, 83.7, 91.2
  - *V. komodoensis* 40000.0 g: 1250.0, 1460.0, 182.0, 249.0, 189.0
  - All 30 rows are in the file. Caution: the first *V. varius* row (810.7 g) repeats the second *V. tristis* row's lengths exactly (222.0, 417.0, 31.9, 32.3, 35.3), which looks like a copy error in the source.
  - Scaling slopes against mass (Additional file 1): SVL 0.297, tail 0.317, proximal hindlimb 0.359, distal hindlimb 0.363, foot 0.323.
- Questions: Q9 now has Varanus limb and tail against SVL (n = 30). U7 (Dipsosaurus percentages) is still open; no free Dipsosaurus table was found.

**Equivalent B:** Druelle F, Goyens J, Vasilopoulou-Kampitsi M, Aerts P 2019. Small vertebrates running on uneven terrain: a biomechanical study of two differently specialised lacertid lizards. *Scientific Reports* 9: 16858 (CC BY 4.0). doi:10.1038/s41598-019-53329-5. PMC6856151 (bucket). Files: `U8_lacertids_uneven_SciRep2019/` (PDF, text, `41598_2019_53329_MOESM2_ESM.csv` per-stride data).
- Why comparable: duty factor, stride frequency and stride length against speed for sprawling lizards on a flat track, the same variables as Fieler & Jayne's speed series.
- Numbers, Table 1 (PDF page 5), flat control, mean (SD): *Acanthodactylus boskianus* speed 1.72 (0.48) m/s, stride frequency 11.74 (2.52) Hz, duty factor 26.67 (5.49) %, stride length 141.18 (20.6) mm; *Podarcis muralis* 1.16 (0.27) m/s, 11.69 (2.29) Hz, 53.25 (11.66) %, 77.09 (16.13) mm.
- Computed from the CSV (control strides; `U8_lacertids_DF_speed_fit_computed.txt`): *P. muralis* n = 21, 0.35-1.74 m/s, duty factor % = 70.76 - 15.14 v (R² 0.15), stride frequency Hz = 3.55 + 7.03 v (R² 0.55); *A. boskianus* n = 29, 0.69-2.63 m/s, duty factor flat with speed (R² 0.00), stride frequency Hz = 3.65 + 4.69 v (R² 0.65). Means match Table 1; the SDs I get (6.70 and 14.02 for duty factor) are larger than the table's, so the table probably uses a different SD basis.
- Questions: U8's polynomial shape is still open (no free Dipsosaurus data), but a free per-stride lizard DF-speed data set now exists.

### 2. Reilly & Elias 1998 → Q11, U9
**Equivalent:** Hutchinson JR, Felkler D, Houston K, Chang Y-M, Brueggen J, Kledzik D, Vliet KA 2019. Divergent evolution of terrestrial locomotor abilities in extant Crocodylia. *Scientific Reports* 9: 19302. PMC6917812 (bucket). Files: `Q11_hutchinson2019_crocodylia_SciRep/` (PDF; `41598_2019_55768_MOESM13_ESM.xlsx` = per-stride data, 184 strides).
- Why comparable: duty factor and speed for *Alligator mississippiensis* and 14 other crocodylians, including the walking and trotting speeds of Reilly & Elias's treadmill study, plus fast gaits.
- Numbers:
  - Earlier alligator data (as the paper cites them): maximal 0.62 m/s, DF about 0.70 (PDF page 4).
  - Table 1 (PDF page 6), adjusted mean DF ± SE: Alligatoroidea symmetrical gaits 0.57 ± 0.03; Crocodyloidea symmetrical 0.56 ± 0.02; Crocodyloidea asymmetrical 0.46 ± 0.02.
  - Table 2 (PDF page 7), fastest stride per individual: Alligatoroidea DF 0.49 ± 0.03; Crocodyloidea 0.42 ± 0.02.
  - Fastest young *A. mississippiensis*: trotting at 2.0 m/s with DF 0.50 (PDF page 4).
  - Per-stride, alligator individual AMA (MOESM13, sheet dataset1_all): 0.210 m/s DF 0.724; 0.305 m/s DF 0.673; 0.312 m/s DF 0.714; 0.508 m/s DF 0.618; 2.003 m/s DF 0.496.
  - Computed fit (`Q11_crocodylia_DF_speed_fit_computed.txt`): Alligatoroidea symmetrical, n = 36, 0.15-3.59 m/s, DF = 0.688 - 0.082 v (R² 0.69).
- Questions: Q11 duty factor against speed is now answered from a free primary data set. Femoral adduction in sprawl versus high walk (Q11, U9 second half) is still open: the 2025 *Sci. Adv.* alligator model (PMC12542966) gives the angles only as figures, so it was screened out.

### 3. Biancardi et al. 2011 → U5, Q28 (and Q27 spider legs)
Note: R7-5 had already read Biancardi 2011 itself (U5 closed). These are added free sources.
**Equivalent A:** Weihmann T 2013. Crawling at high speeds: steady level locomotion in the spider *Cupiennius salei*. *PLOS ONE* 8: e65788. File `Q28_Weihmann2013_Cupiennius_PLOSONE.pdf` (CC BY).
- Numbers: constant running speeds up to 0.6 m/s (PDF page 7); Table 1 (page 7) inverse contact duration rises linearly through the origin with speed, slopes 33.47, 35.72, 39.29 and 47.16 per metre (legs 1 to 4); Table 2 (page 8) stride frequency above 0.5 m/s, median (Q25/Q75): 6.3 (5.7/6.7), 7.2 (5.1/7.7), 8.6 (7.9/9.1), 8.3 (7.7/9.0) Hz for legs 1 to 4; at high speed duty factors fell below 0.5 in legs 1, 2 and 4 (page 14). Transitional speeds 0.2 to 0.39 m/s depending on leg and measure (Table 1 note).
**Equivalent B:** Silva-Pereyra V, Fábrica CG, Biancardi CM, Pérez-Miles F 2019. Kinematics of male *Eupalaestrus weijenberghi* (Araneae, Theraphosidae) locomotion on different substrates and inclines. *PeerJ* 7: e7748. PMC6766366 (bucket). Files `Q28_Biancardi2019_Eupalaestrus_PeerJ/`.
- Numbers: n = 5 spiders, mass 5.10 g (SD 0.68) (PDF page 6). Table 3 (page 11): glass 0° speed 2.55 ± 0.90 cm/s, DF 0.61 ± 0.01, stride frequency 0.71 ± 0.27 per s; stride frequency rose with speed (r = 0.821), stride length nearly constant.
- Table 1 (page 7), leg segment lengths, mean ± SD, n = 12, mm: femur I 14.12 ± 0.70, II 12.91 ± 0.61, III 11.48 ± 0.63, IV 14.13 ± 0.71; patella-tibia I 16.87 ± 0.73, II 15.11 ± 0.56, III 13.59 ± 0.49, IV 18.52 ± 0.59; metatarsus I 10.11 ± 0.58, II 9.95 ± 0.54, III 11.18 ± 0.32, IV 16.96 ± 0.60. No tarsus length (tarsus mass only), no coxa or trochanter length.
- Questions: Q28 now has two free species (one fast, one slow). Q27 gets a measured tarantula leg table with spread (femur, patella-tibia, metatarsus for all four legs).

### 4. Kram, Wong & Full 1997 → U2, Q27
**Equivalent:** Parle E, Larmon H, Taylor D 2016. Biomechanical factors in the adaptations of insect tibia cuticle. *PLOS ONE* 11: e0159262. PMC4972353 (bucket). File `Q27_Parle2016_insect_tibia_PLOSONE/`.
- Numbers, Table 2 (PDF page 7), mean ± SD: *Blaberus discoidalis* hind tibia length 8.34 ± 1.3 mm (n = 10), body mass 3.36 ± 0.48 g; *Periplaneta americana* hind tibia 5.98 ± 0.8 mm (n = 17), 1.12 ± 0.21 g.
- Questions: U2 still open for the full table (coxa, femur, tarsus of each leg). Only the hind tibia is confirmed free. The tarantula table in row 3 helps Q27.

### 5. Tobalske 1999; Tobalske & Dial 1996 → Q15
**Equivalent:** Deetjen ME, Chin DD, Heers AM, Tobalske BW, Lentink D 2024. Small deviations in kinematics and body form dictate muscle performances in the finely tuned avian downstroke. *eLife* 12: RP89968. PMC10942624 (bucket). Files `Q15_PMC10942624/`.
- Numbers, Table 2 (PDF page 15), ringneck dove *Streptopelia risoria*, second wingbeat after take-off, N = 4 birds, n = 5 flights each: flight speed 1.23 ± 0.13 m/s; downstroke 61.80 ± 4.73 ms; upstroke 40.65 ± 3.88 ms; flapping frequency 9.80 ± 0.62 Hz.
- Computed: downstroke share = 61.80 / (61.80 + 40.65) = 60.3 % of the cycle (take-off, not cruising flight).
- Questions: Q15 gains one species at a stated speed. The other free leads found (University of Montana ScholarWorks: a dove thesis table and Dial 1992 *Auk* on pigeons) returned 403 to scripts and are listed for a browser.

### 6. Full & Tu 1991; Bender et al. 2011 → U6
Note: R7-5 had already closed U6 from the Full & Tu abstract and Bender 2011. This adds a free comparison species.
**Equivalent:** Weihmann T, Brun PG, Pycroft E 2017. Speed dependent phase shifts and gait changes in cockroaches running on substrates of different slipperiness. *Frontiers in Zoology* 14: 54. PMC5719566 (bucket). Files `U6_weihmann2017_cockroach_FrontZool/`.
- Numbers (*Nauphoeta cinerea*): top speed up to 0.35 m/s on non-slippery ground (PDF page 7); maximal stride frequency about 15 per s (page 7); switch from alternating tripod to a metachronal fast gait at a transitional speed of 0.15 m/s (page 8); duty factors fell from about 0.7 at low speed to about 0.5 and below at top speed (PDF page 7).
- Questions: U6 confirmed in kind for a second blaberid; the 27 Hz figure for *Periplaneta* stays with Full & Tu.

### 7. McElroy, Hickey & Reilly 2008 → U8 (Varanus duty factor)
R7-5 had already read McElroy 2008 (walk 69 %, run 41 %). No free *Varanus* duty-factor table was found. Free comparators are the lacertid data (row 1B) and the crocodylian data (row 2).

### 8. Peterson & Hennessey 1978 → W70
**Equivalent A:** US EPA 2000. *Meteorological Monitoring Guidance for Regulatory Modeling Applications*, EPA-454/R-99-005. File `W70_EPA-454-R-99-005_met_monitoring_guidance.pdf`. Table 6-2 (PDF page 57, printed page 6-9), recommended power-law exponents (urban / rural): stability A 0.15 / 0.07; B 0.15 / 0.07; C 0.20 / 0.10; D 0.25 / 0.15; E 0.30 / 0.35; F 0.30 / 0.55.
**Equivalent B:** Spera DA, Richards TR 1979. *Modified power law equations for vertical wind profiles*. NASA TM-79275 (DOE/NASA/1059-79/4). File `W70_NASA_19800005367_modified_power_law.pdf` (scanned, OCR). The exponent measured over one site changed from 1/7 by day to 1/2 at night (Sisterson & Frenzen 1978, as cited; PDF page 3). The roughness exponents in their Table I range from 0.1 to almost 0.5 (PDF page 6; OCR reads "O.S").
- Questions: W70 answered in substance. Neutral (class D) rural 0.15 is consistent with 1/7 (0.143), and the exponent varies with stability and roughness from about 0.07 to 0.55. Peterson & Hennessey's own figures are still unread.

### 9. Idso et al. 1975 → U21 (wet against dry soil albedo)
**Equivalent:** NASA Technical Paper 2643 (May 1987), *Surface bidirectional reflectance properties of two southwestern Arizona deserts for wavelengths between 0.4 and 2.2 micrometers*. File `U21_NASA_19870012848_AZ_desert_reflectance.pdf` (NTRS 19870012848).
- Numbers: Sonora Desert sand lab test: a soil moisture of 16 % by weight reduces reflectance by about 50 %; the loss is nonlinear in moisture and varies with wavelength (PDF page 6, Fig. 8b). The field data are dry sand (0.16 % soil moisture by weight).
- Questions: U21's "wet soil 32 to 58 % below dry (mean 45 %)" and "albedo = 0.069 x Munsell - 0.114" are still unconfirmed. This gives an independent figure of about 50 % at 16 % moisture for desert sand. A free Idso 1974 conference paper (Univ. of Arizona repository, hwr_04-041-054.pdf) exists but returned 403 to scripts.

### 10. Tsvetsinskaya et al. 2002 → U21 (desert albedos)
**Equivalent:** the same NASA TP 2643. Spectral hemispherical albedo of dry Sonora Desert sand at 0.4, 0.55, 0.65, 0.75 and 1.65 µm: solar zenith 31° (Table 2, PDF page 9): 0.114, 0.245, 0.325, 0.417, 0.468; solar zenith 57° (Table 3, page 10): 0.155, 0.283, 0.366, 0.419, 0.501. Mohawk Valley (Table 4, page 11; last band 1.04 µm): 0.108, 0.205, 0.275, 0.349, 0.446.
- Questions: these are narrow-band ground albedos, not MODIS broadband, so the 0.426 ± 0.043 dune figure is still unconfirmed. The later Tsvetsinskaya / Zhou papers (2003 GRL, 2005 GRL, 2006 JGR) are "bronze" on Wiley and refused scripts.

### 11. Campione & Evans 2020 → W13 (Argentinosaurus about 75 t)
**Equivalent A:** Sellers WI, Margetts L, Coria RA, Manning PL 2013. March of the Titans: the locomotor capabilities of sauropod dinosaurs. *PLOS ONE* 8: e78733 (CC BY). File `W13_sellers2013_PLOSONE.pdf`. The Argentinosaurus body mass from convex hulling is 83,230 kg (PDF page 5), and the model table lists 83,230.29 kg (Table 4, PDF page 8).
**Equivalent B:** Campione NE, Evans DC 2012. A universal scaling relationship between body mass and proximal limb bone dimensions in quadrupedal terrestrial tetrapods. *BMC Biology* 10: 60. PMC3403949. Equation (1): logBM = 2.749·logC(H+F) - 1.104, with R² = 0.988 and mean PPE 25.6 % (PDF page 14). This supplies an open alternative method, not verification of the 2020 review's figure.
- Questions: W13 partly answered. There is a free volumetric estimate (about 83 t) and the free scaling equation, but the 2020 paper's own number and error bar are still unread.

### 12. Hutchinson 2004 (RVC green copy, HTTP 500) → W19
**Equivalent:** Sellers WI, Pond SB, Brassey CA, Manning PL, Bates KT 2017. Investigating the running abilities of *Tyrannosaurus rex* using stress-constrained multibody dynamic analysis. *PeerJ* 5: e3420. PMC5518979 (bucket). Files `W19_sellers2017_trex_PeerJ/`. Maximum speed 7.7 m/s with high stress limits (over 200 MPa) (PDF page 8). The paper notes earlier qualitative claims of up to 20 m/s (PDF page 2, citing Bakker 1986 and Paul 1998, 2008).
- Questions: W19 partly answered (which source gave 20 m/s is narrowed to Bakker and Paul, not Hutchinson). Hutchinson 2004's own range is still unread.

### 13. Shine et al. 2023 Dryad data → Q22, Q23
**Exact data:** Zenodo record 8317256, "Sexual dimorphism in aipysurine sea snakes", doi:10.5061/dryad.t1g1jwt82, CC0. File `A7_shine_zenodo8317256_Aipysurine_sex_dimorphism_for_Dryad2.xlsx` (38,412 bytes, the same size as Dryad's API file list), plus README.
- Unit note: the README says SVL is in cm, but the values (for example 405 for a juvenile, up to about 1,000 for adults) only make sense in mm. Tail, head and body widths are treated as mm too. One genus cell reads "Aipysu+C370rus" (a spreadsheet slip); it is counted as *Aipysurus foliosquama*.
- Computed per species and sex, adults only (`A7_shine_species_summary_computed.csv`, mean ± SD (n)):

| Taxon | Sex | SVL mm | Tail % SVL | Head length % SVL | Mid-body width % SVL | Max tail width % mid-body width |
|---|---|---|---|---|---|---|
| *Aipysurus laevis* | f | 987.3 ± 145.0 (70) | 14.9 ± 1.6 | 5.1 ± 0.6 | 4.8 ± 0.8 | 82.3 ± 16.7 |
| *A. laevis* | m | 814.3 ± 107.3 (58) | 17.5 ± 1.7 | 4.8 ± 0.5 | 4.5 ± 0.9 | 85.5 ± 11.8 |
| *A. duboisii* | f | 969.3 ± 128.4 (24) | 12.1 ± 1.0 | 2.7 ± 0.2 | 3.0 ± 0.5 | 97.4 ± 21.6 |
| *A. duboisii* | m | 772.3 ± 103.6 (39) | 13.5 ± 1.1 | 3.0 ± 0.3 | 3.1 ± 0.5 | 99.8 ± 14.3 |
| *A. eydouxii* | f | 634.1 ± 105.9 (62) | 15.0 ± 1.4 | 3.6 ± 0.3 | 4.2 ± 0.8 | 86.4 ± 15.1 |
| *A. eydouxii* | m | 587.0 ± 92.4 (27) | 17.6 ± 2.3 | 3.9 ± 0.4 | 4.3 ± 0.6 | 88.3 ± 11.0 |
| *A. foliosquama* | f | 805.7 ± 109.5 (7) | 11.2 ± 1.0 | 3.0 ± 0.3 | 3.2 ± 0.5 | 92.0 ± 7.3 |
| *A. foliosquama* | m | 716.8 ± 52.1 (20) | 12.3 ± 0.9 | 2.9 ± 0.2 | 3.0 ± 0.4 | 96.7 ± 12.4 |
| *A. fuscus* | f | 706.0 ± 44.8 (13) | 14.9 ± 0.8 | 3.3 ± 0.2 | 3.5 ± 0.3 | 97.9 ± 12.0 |
| *A. fuscus* | m | 659.7 ± 45.1 (15) | 15.3 ± 0.6 | 3.2 ± 0.2 | 3.4 ± 0.4 | 96.9 ± 9.3 |
| *Emydocephalus annulatus* | f | 645.6 ± 56.5 (8) | 16.0 ± 0.5 | 3.6 ± 0.2 | 4.9 ± 0.8 | 84.9 ± 14.9 |
| *E. annulatus* | m | 575.4 ± 43.6 (14) | 17.9 ± 0.9 | 3.4 ± 0.2 | 4.0 ± 0.5 | 95.3 ± 12.9 |

  (Sample sizes per ratio vary slightly; see the CSV. Head width as % of mid-body width is also in the CSV.)
- Questions: Q23 (head and tail against SVL) is answered for six sea snakes. Q22 is partly answered for sea snakes only: there are just two girth stations (mid-body and widest tail), and these tail widths belong to laterally flattened sea snakes and must not be transferred to land snakes. There are no vertebra counts.

### 14. Baliga et al. 2019 figshare raw angles → Q16
**No downloadable free equivalent obtained.** figshare's API and downloader returned 403 to the box and to the fetch service. The Harvey et al. 2022 *Nature* ROM data (same lab) are also on figshare. Harvey et al. 2019 *J. R. Soc. Interface* (gull elbow/wrist ranges) is PMC6364660 but is not in the open bucket, and the PMC, Europe PMC and Royal Society PDFs refused scripts. Q16 stays open beyond the pigeon.

### 15. Kubo et al. 2019 figshare → U12
**Partial equivalent:** Zenodo 4970077 (CC0; supplement to a 2012 *BMC Evol. Biol.* PGLS paper, doi:10.1186/1471-2148-12-102), file `U12_zenodo4970077_CarnData/CarnData.csv`: 101 Carnivora species, each coded Digitigrady = 1 (digitigrade) or 0 (plantigrade). Example rows: *Ailurus fulgens* 0; *Vulpes lagopus* 1; *Canis adustus* 1.
- Questions: U12 is partly answered for Carnivora. The 880-mammal Kubo list (PNAS Dataset S01) is still not free to scripts.

### 16. Marek et al. 2021 ESM S2 → Q13 (neck counts)
**Substitute:** Zenodo 10785541, Marek R (CC0), "Data from: The neck as a keystone structure in avian macroevolution and mosaicism" (cited by *BMC Biology* doi:10.1186/s12915-023-01715-x). File `A29sub_zenodo10785541_avian_neck/svp2022data_v2.csv`, column `cervcount`, 112 species (`A29sub_cervcount_selected_extracted.txt`): *Columba livia* 12; *Corvus albus* 12; *Tyto alba* 12; *Passerina cyanea* (indigo bunting) 13; *Pelecanus occidentalis* 17; *Grus leucogeranus* 18; *Anseranas semipalmata* 18; *Anser fabalis* 19 (the maximum); minimum 11; median 13. Zenodo 4515823 (the 2021 paper's own Dryad mirror) holds only meshes and a specimen list (SpecimenInformation.xlsx, kept), with no counts.
- Questions: Q13 counts are now free for 112 species. Neck length is not in this file. The count convention (whether the atlas is included) is not checked.

### 17. Willett AFM tyre data → W40
**Substitute:** Dijkman KH 2024, MSc thesis, TU Delft (January 2024), *Impact of bicycle tire parameters on the total rolling losses*. File `W40_TUDelft_bicycle_tire_rolling_losses.pdf`. Table IX (PDF page 47) gives road-measured Crr ± SE for Vittoria Corsa Pro tyres. On asphalt: 26 mm at 30 km/h 0.001610 to 0.001736 (5.3 to 6 bar), at 40 km/h 0.001939 to 0.002068; 28 mm at 30 km/h 0.001613 to 0.001727, at 40 km/h 0.001774 to 0.002011. On bricks: 0.002306 to 0.003174.
- Questions: W40 partly answered. Free measured road Crr for modern racing tyres is about 0.0016 to 0.0032, against Wikipedia's 0.0022 to 0.0050 roller figure. Willett's own 120 psi / 50 km/h roller data are still unread.

### 18. Green Seal 2003 → W43
**Substitute A:** California Energy Commission, *California State Fuel-Efficient Tire Report: Volume II* (Consultant report 600-03-001CR, January 2003). File `W43_CEC_2003_600-03-001CR_vol2.pdf`. The copy is a third-party host of a public state report; the UC Davis report below confirms the CEC data independently. Footnote (PDF page 33): Michelin's 1995 list for 25 vehicles gave OE tyre RRC of 6.9 to 11.4 kg/t (average 9.2, sales-weighted 8.4), and 37 replacement tyres gave 8.7 to 13.4 kg/t (average 11.2). That is 0.0069 to 0.0114 and 0.0087 to 0.0134 as coefficients.
**Substitute B:** Lutsey N, Regnier J, Burke A et al. 2006, *Assessment of Tire Technologies and Practices for Potential Waste and Energy Use Reductions*, UC Davis ITS report (escholarship.org/uc/item/06r0q71c). File `W43_UC_eScholarship_tire_technologies_06r0q71c.pdf`. Per NRC (2006), the RRC of individual tyre brands and models varies "generally from 0.0065 to 0.013" (PDF page 18).
- Questions: W43 has a similar, narrower range in two free sources; the exact 0.0062 and 0.0150 endpoints are still unconfirmed.

### 19. Ford F-150 Raptor 2021 → Q31
**Exact maker spec:** Ford, *2021 Ford F-150 Raptor Technical Specifications* (InDesign file dated 2 Feb 2021), from Ford's Belgian press site fordpresse.be. File `Q31_Ford_2021_F-150_Raptor_Tech_Specs_fordpresse.pdf`. Page 2: front suspension travel 14.0 in standard (13.0 in with optional 37 in tyres); rear 15.0 in (14.1 in with 37 in tyres). Page 3 of the same file lists wheelbase 145.4 in, front track 74.0 / 74.3 in, rear 73.6 / 73.9 in, ground clearance 12.0 / 13.1 in, approach angle 31.0° / 33.1°, and departure 23.9° / 24.9° (35 in / 37 in tyres).
- Questions: Q31 gets the off-road class (about 356 mm front, 381 mm rear). Car, SUV, lorry and tank travel are still open.

## Tried and refused (not used, nothing paid or signed in)
- figshare API and downloader (Baliga 8856161, Kubo 24382114): 403 to the box and to the fetch service.
- Dryad file download API: 401 (needs a token); solved for Shine through the Zenodo mirror.
- University of Montana ScholarWorks (dove thesis, Dial 1992 *Auk*), University of Arizona repository (Idso 1974), PeerJ direct, Wiley pdfdirect, Royal Society, Europe PMC render, media.ford.com, NHTSA, and TRB: 403, 500 or timeout to scripts.
- Screened and set aside in `sources/equivalents-r7c/_screened_not_used/` (no numbers used): a Biomimetics 2026 stroke-asymmetry paper (stock video), JRSI 2022 wingbeat power, a PLOS ONE Blaberus leg-regrowth paper, a 2025 *Sci. Adv.* alligator model (angles in figures only), Harvey 2022 *Nature* (data on figshare), a gecko incline paper, Hutchinson 2011 *PLOS ONE*, a high-school journal bicycle paper, and failed-download stubs.

