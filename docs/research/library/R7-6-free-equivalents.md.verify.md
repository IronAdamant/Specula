# R7-6.verify.md: self-check of library/R7-6-free-equivalents.md

Checked 2026-11-24. Method: `sources/equivalents-r7c/R7-6_verify_script.py` re-extracts the named PDF page with pdftotext (layout and raw modes) and confirms each quoted number appears on that page. For computed values, it checks the saved script output. It also re-reads the Shine summary CSV and confirms every cell in R7-6's sea-snake table matches. Result: **56 of 56 checks VERIFIED, 0 missing.** All 27 PDFs in `sources/equivalents-r7c/` (outside `_screened_not_used/`) start with `%PDF-`.

## Automated checks
| Row | Claim | File and page | Result |
|---|---|---|---|
| 1A | V. brevicauda 7.6 g row | `Q9_dick_clemente2016_varanus_FrontZool/12983_2016_141_MOESM2_ESM.pdf` p.1 | VERIFIED |
| 1A | V. gouldii 429.4 g row | `Q9_dick_clemente2016_varanus_FrontZool/12983_2016_141_MOESM2_ESM.pdf` p.1 | VERIFIED |
| 1A | V. panoptes 4150 g row | `Q9_dick_clemente2016_varanus_FrontZool/12983_2016_141_MOESM2_ESM.pdf` p.1 | VERIFIED |
| 1A | V. varius 4025.2 g row | `Q9_dick_clemente2016_varanus_FrontZool/12983_2016_141_MOESM2_ESM.pdf` p.1 | VERIFIED |
| 1A | V. komodoensis 40000 g row | `Q9_dick_clemente2016_varanus_FrontZool/12983_2016_141_MOESM2_ESM.pdf` p.1 | VERIFIED |
| 1A | V. varius 810.7 duplicates V. tristis | `Q9_dick_clemente2016_varanus_FrontZool/12983_2016_141_MOESM2_ESM.pdf` p.1 | VERIFIED |
| 1A | Scaling slopes SVL 0.297 tail 0.317 prox 0.359 dist 0.363 foot 0.323 | `Q9_dick_clemente2016_varanus_FrontZool/12983_2016_141_MOESM1_ESM.pdf` p.1 | VERIFIED |
| 1B | Table 1 lacertids | `U8_lacertids_uneven_SciRep2019/PMC6856151.1.pdf` p.5 | VERIFIED |
| 1B | computed DF fit | `U8_lacertids_DF_speed_fit_computed.txt` | VERIFIED |
| 2 | earlier alligator 0.62 m/s DF ~0.70 | `Q11_hutchinson2019_crocodylia_SciRep/PMC6917812.1.pdf` p.4 | VERIFIED |
| 2 | fastest young A. miss 2.0 m/s DF 0.50 | `Q11_hutchinson2019_crocodylia_SciRep/PMC6917812.1.pdf` p.4 | VERIFIED |
| 2 | Table 1 DF | `Q11_hutchinson2019_crocodylia_SciRep/PMC6917812.1.pdf` p.6 | VERIFIED |
| 2 | Table 2 DF | `Q11_hutchinson2019_crocodylia_SciRep/PMC6917812.1.pdf` p.7 | VERIFIED |
| 2 | AMA strides + fit | `Q11_crocodylia_DF_speed_fit_computed.txt` | VERIFIED |
| 3A | Cupiennius up to 0.6 m/s | `Q28_Weihmann2013_Cupiennius_PLOSONE.pdf` p.7 | VERIFIED |
| 3A | Table 1 slopes | `Q28_Weihmann2013_Cupiennius_PLOSONE.pdf` p.7 | VERIFIED |
| 3A | Table 1 note transitions | `Q28_Weihmann2013_Cupiennius_PLOSONE.pdf` p.7 | VERIFIED |
| 3A | Table 2 fT | `Q28_Weihmann2013_Cupiennius_PLOSONE.pdf` p.8 | VERIFIED |
| 3A | DF below 0.5 legs 1,2,4 | `Q28_Weihmann2013_Cupiennius_PLOSONE.pdf` p.14 | VERIFIED |
| 3B | n=5 mass 5.10 g | `Q28_Biancardi2019_Eupalaestrus_PeerJ/PMC6766366.1.pdf` p.6 | VERIFIED |
| 3B | Table 3 | `Q28_Biancardi2019_Eupalaestrus_PeerJ/PMC6766366.1.pdf` p.11 | VERIFIED |
| 3B | Table 1 leg segments | `Q28_Biancardi2019_Eupalaestrus_PeerJ/PMC6766366.1.pdf` p.7 | VERIFIED |
| 4 | Parle Table 2 | `Q27_Parle2016_insect_tibia_PLOSONE/PMC4972353.1.pdf` p.7 | VERIFIED |
| 5 | dove Table 2 | `Q15_PMC10942624/PMC10942624.1.pdf` p.15 | VERIFIED |
| 6 | Nauphoeta numbers | `U6_weihmann2017_cockroach_FrontZool/PMC5719566.1.pdf` p.7 | VERIFIED |
| 6 | transitional 0.15 | `U6_weihmann2017_cockroach_FrontZool/PMC5719566.1.pdf` p.8 | VERIFIED |
| 8A | EPA Table 6-2 | `W70_EPA-454-R-99-005_met_monitoring_guidance.pdf` p.57 | VERIFIED |
| 8B | 1/7 day 1/2 night | `W70_NASA_19800005367_modified_power_law.pdf` p.3 | VERIFIED |
| 8B | 0.1 to almost 0.5 | `W70_NASA_19800005367_modified_power_law.pdf` p.6 | VERIFIED |
| 9 | 16% moisture ~50% | `U21_NASA_19870012848_AZ_desert_reflectance.pdf` p.6 | VERIFIED |
| 10 | Table 2 albedo SZA 31 | `U21_NASA_19870012848_AZ_desert_reflectance.pdf` p.9 | VERIFIED |
| 10 | Table 3 albedo SZA 57 | `U21_NASA_19870012848_AZ_desert_reflectance.pdf` p.10 | VERIFIED |
| 10 | Table 4 Mohawk | `U21_NASA_19870012848_AZ_desert_reflectance.pdf` p.11 | VERIFIED |
| 11A | Argentinosaurus 83,230 kg | `W13_sellers2013_PLOSONE.pdf` p.5 | VERIFIED |
| 11A | Table 83,230.29 | `W13_sellers2013_PLOSONE.pdf` p.8 | VERIFIED |
| 11B | Campione 2012 eq 1 | `W13_campione_evans2012_PMC3403949/PMC3403949.1.pdf` p.14 | VERIFIED |
| 12 | T rex 7.7 m/s | `W19_sellers2017_trex_PeerJ/PMC5518979.1.pdf` p.8 | VERIFIED |
| 12 | up to 20 m/s | `W19_sellers2017_trex_PeerJ/PMC5518979.1.pdf` p.2 | VERIFIED |
| 16 | cervcount extracted | `A29sub_cervcount_selected_extracted.txt` | VERIFIED |
| 17 | TU Delft Table IX | `W40_TUDelft_bicycle_tire_rolling_losses.pdf` p.47 | VERIFIED |
| 18A | CEC footnote kg/T | `W43_CEC_2003_600-03-001CR_vol2.pdf` p.33 | VERIFIED |
| 18B | UC Davis 0.0065-0.013 | `W43_UC_eScholarship_tire_technologies_06r0q71c.pdf` p.18 | VERIFIED |
| 19 | Ford Raptor page 2 | `Q31_Ford_2021_F-150_Raptor_Tech_Specs_fordpresse.pdf` p.2 | VERIFIED |
| 19 | Ford Raptor page 3 dimensions | `Q31_Ford_2021_F-150_Raptor_Tech_Specs_fordpresse.pdf` p.3 | VERIFIED |
| 13 | Shine Aipysurus duboisii f: 969.3 ± 128.4, 12.1 ± 1.0, 2.7 ± 0.2, 3.0 ± 0.5, 97.4 ± 21.6 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Aipysurus duboisii m: 772.3 ± 103.6, 13.5 ± 1.1, 3.0 ± 0.3, 3.1 ± 0.5, 99.8 ± 14.3 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Aipysurus eydouxii f: 634.1 ± 105.9, 15.0 ± 1.4, 3.6 ± 0.3, 4.2 ± 0.8, 86.4 ± 15.1 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Aipysurus eydouxii m: 587.0 ± 92.4, 17.6 ± 2.3, 3.9 ± 0.4, 4.3 ± 0.6, 88.3 ± 11.0 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Aipysurus foliosquama f: 805.7 ± 109.5, 11.2 ± 1.0, 3.0 ± 0.3, 3.2 ± 0.5, 92.0 ± 7.3 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Aipysurus foliosquama m: 716.8 ± 52.1, 12.3 ± 0.9, 2.9 ± 0.2, 3.0 ± 0.4, 96.7 ± 12.4 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Aipysurus fuscus f: 706.0 ± 44.8, 14.9 ± 0.8, 3.3 ± 0.2, 3.5 ± 0.3, 97.9 ± 12.0 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Aipysurus fuscus m: 659.7 ± 45.1, 15.3 ± 0.6, 3.2 ± 0.2, 3.4 ± 0.4, 96.9 ± 9.3 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Aipysurus laevis f: 987.3 ± 145.0, 14.9 ± 1.6, 5.1 ± 0.6, 4.8 ± 0.8, 82.3 ± 16.7 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Aipysurus laevis m: 814.3 ± 107.3, 17.5 ± 1.7, 4.8 ± 0.5, 4.5 ± 0.9, 85.5 ± 11.8 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Emydocephalus annulatus f: 645.6 ± 56.5, 16.0 ± 0.5, 3.6 ± 0.2, 4.9 ± 0.8, 84.9 ± 14.9 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |
| 13 | Shine Emydocephalus annulatus m: 575.4 ± 43.6, 17.9 ± 0.9, 3.4 ± 0.2, 4.0 ± 0.5, 95.3 ± 12.9 | `A7_shine_species_summary_computed.csv` | VERIFIED (in R7-6 table) |

## Manual checks (not covered by the script)
| Row | Item | Check | Result |
|---|---|---|---|
| 5 | Downstroke share 60.3 % | 61.80 / (61.80 + 40.65) = 0.6032; the cycle of 102.45 ms matches 1 / 9.80 Hz = 102.0 ms | VERIFIED (derived) |
| 19 | 14.0 in about 356 mm, 15.0 in = 381 mm | 14.0 × 25.4 = 355.6; 15.0 × 25.4 = 381.0 | VERIFIED (derived) |
| 18A | kg/t to coefficient | divide by 1000: 6.9-11.4 kg/t = 0.0069-0.0114; 8.7-13.4 kg/t = 0.0087-0.0134 | VERIFIED (derived) |
| 13 | Shine file is the Dryad file | Zenodo 8317256 lists DOI 10.5061/dryad.t1g1jwt82; xlsx 38,412 bytes = Dryad API size for Aipysurine_sex_dimorphism_for_Dryad2.xlsx (dryad_files.json) | VERIFIED |
| 13 | Unit note (mm, not cm) | Raw rows read 405-430 for juveniles (e.g. R.21451, 405); 400 cm would be impossible for these snakes | WEAK: an inference, flagged as such in R7-6 |
| 1A | V. varius / V. tristis duplicate | Automated: the 810.7 g V. varius row and the 104.5 g V. tristis row carry identical lengths | VERIFIED (flagged as a likely source error) |
| 1B | SD mismatch | Script SDs 6.70 and 14.02 against table SDs 5.49 and 11.66; means match exactly | VERIFIED as reported (a discrepancy, not resolved) |
| 8B | "almost 0.5" | OCR reads "O.S"; read as 0.5 from context | WEAK (OCR) |
| 16 | cervcount convention | Not checked against the BMC Biology paper | UNVERIFIABLE here, flagged in R7-6 |
| all | Licences | PMC bucket items are in the PMC open-access subset (CC BY for Sci Rep, Front Zool, PLOS, PeerJ, BMC; eLife CC BY); Zenodo items CC0; EPA and NASA US-government works; Ford spec is a maker press document; CEC report is a public state report on a third-party host; TU Delft thesis and UC Davis report from their own repositories | Recorded, not legal advice |
