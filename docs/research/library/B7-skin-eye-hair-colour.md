# B7 — Skin, eye, hair and lip colour

> Verify fixes applied 2026-11-17: ITA category names (very light … dark) are not in Alipour Table 3, which lists Fitzpatrick types and numeric bands only; the names are now cited from Charlton et al. 2020, *PLoS ONE*, Table 1, which attributes them to Del Bino et al. (Chardon 1991 and Del Bino not opened). ISSA 2025 first author corrected to Lu Y (Yan Lu), not Xiao. The Loussouarn 2016 `…306835` URL was replaced with the working `https://…306744/article.phtml?tab=texte` form (the old URL redirected to an unrelated article).

Access date for every source: **2026-11-16** unless a row says otherwise. Each row gives its source, the table it came from and the URL. "My arithmetic" marks numbers I computed; it is not a published value. All sRGB values I computed use CIE D65, 2° observer, the standard sRGB matrix and gamma, and are clipped to gamut. I copied no figures or wording from copyrighted books.

## 1. Monk Skin Tone (MST) scale

Source: Monk, E. (2019–2023), Monk Skin Tone Scale, Google. https://skintone.google. The values come from the site's JS bundle https://skintone.google/assets/tono-color-block-arranger.73c427e1.js.
**Licence:** the site states "Creative Commons Attribution 4.0 International" (https://skintone.google/assets/tono-get-started.62a4df4e.js, get-started page). It is CC BY 4.0: commercial use is allowed with attribution; there is **no NC and no ND term**.

| Tone | Hex (site) | CIELAB L*, a*, b* (site) | ITA° (my arithmetic) |
|---|---|---|---|
| Monk 01 | #f6ede4 | 94.211, 1.503, 5.422 | 83.0 |
| Monk 02 | #f3e7db | 92.275, 2.061, 7.28 | 80.2 |
| Monk 03 | #f7ead0 | 93.091, 0.216, 14.205 | 71.8 |
| Monk 04 | #eadaba | 87.573, 0.459, 17.748 | 64.7 |
| Monk 05 | #d7bd96 | 77.902, 3.471, 23.136 | 50.3 |
| Monk 06 | #a07e56 | 55.142, 7.783, 26.74 | 10.9 |
| Monk 07 | #825c43 | 42.47, 12.325, 20.53 | −20.1 |
| Monk 08 | #604134 | 30.678, 11.667, 13.335 | −55.4 |
| Monk 09 | #3a312a | 21.069, 2.69, 5.964 | −78.4 |
| Monk 10 | #292420 | 14.61, 1.482, 3.525 | −84.3 |

- The site gives Lab but does not name an illuminant. My check: converting the hex values for tones 01, 06 and 10 with D65 sRGB reproduces the site's Lab. So the Lab is most likely D65 and derived from the hex swatch. It is **not a skin measurement**.
- MST is a 10-point **perceptual** scale for annotation. It has no SD because it is not a population sample.
- Related dataset: MST-E (Schumann et al., arXiv 2305.09073, https://arxiv.org/abs/2305.09073), CC BY 4.0 according to its download page (storage.googleapis.com/mste). I did not download it.

## 2. ITA° (Individual Typology Angle) and skin CIELAB by population

ITA° = arctan((L* − 50)/b*) × 180/π.

| Category | ITA° range | Fitzpatrick mapping (Alipour 2026) | Source / table / URL |
|---|---|---|---|
| Very light | > 55 | I | **Names** (this column): Charlton et al. 2020, *PLoS ONE* 15(11):e0241843, Table 1 (PMC7688144, CC BY 4.0), which presents them as “proposed by Del Bino et al.” https://www.ebi.ac.uk/europepmc/webservices/rest/PMC7688144/fullTextXML (accessed 2026-11-17). **Numeric bands and FST I–VI:** Alipour et al. 2026, *Skin Res Technol*, Table 3 (PMC13459198, CC BY) https://www.ebi.ac.uk/europepmc/webservices/rest/PMC13459198/fullTextXML (accessed 2026-11-16). Alipour Table 3 does not print the category names. |
| Light | 41–55 | II | same |
| Intermediate | 28–41 | III | same |
| Tan | 10–28 | IV | same |
| Brown | −30 to 10 | V | same |
| Dark | < −30 | VI | same |

- Numeric ITA bands and the Fitzpatrick I–VI mapping are from Alipour et al. 2026, Table 3 (that table does not print the category names). The names very light / light / intermediate / tan / brown / dark are from Charlton et al. 2020, Table 1, which attributes them to Del Bino et al. Charlton’s Del Bino citation is Del Bino & Bernerd 2013, *Br J Dermatol* 169:33 (DOI 10.1111/bjd.12529). I did not open Del Bino 2013 or Chardon 1991 (Int J Cosmet Sci 13:191–208, copyrighted). Alipour notes that the ITA cut-offs came from lighter-skinned cohorts. The ITA-to-Fitzpatrick mapping is Alipour’s choice; treat it as approximate.

**Population skin CIELAB**

| Study | Population / n | What is reported | Licence | URL |
|---|---|---|---|---|
| ISSA (Lu Y, Xiao K, Pointer M, et al.) 2025, *Sci Data* 12:487, Table 1 | 15,256 spectra from 2,113 subjects in 11 datasets (e.g. UK Caucasian 172, China 201, Iraq 149, Thai 710, Pakistani 117, Spain 27). D65, CIE 1931 2° observer. | Dataset description only; **the paper has no summary L*a*b* table**. Data are on figshare 10.6084/m9.figshare.28228571.v4 (dataset licence not checked; not downloaded). | Paper CC BY | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC11930939/fullTextXML |


**Xiao et al. 2017**, *Skin Res Technol* 23(1):21–29, Table 2: skin colour mean (SD). CC BY (the paper says it is published under a Creative Commons Attribution License).
- Instruments: Minolta CM-2600d with a 3 mm aperture (Caucasian, Chinese, Kurdish) and X-Rite SP62 with a 4 mm aperture (Thai), D65.
- n = 960, aged 18–75. Caucasian 187 (102 F / 85 M), Chinese 202 (65/137), Kurdish 145 (74/72), Thai 426 (283/143) (Table 1).
- Open PDF: https://eprints.whiterose.ac.uk/id/eprint/100718/7/Xiao_et_al-2017-Skin_Research_and_Technology.pdf (DOI 10.1111/srt.12295).
- I found this PDF through a Grok lead and re-read every value below myself.

| Group | Site | L* | a* | b* |
|---|---|---|---|---|
| Caucasian | forehead | 59.2 (5.1) | 11.6 (2.8) | 15.1 (2.3) |
| Caucasian | cheek | 59.6 (5.5) | 11.8 (3.1) | 14.6 (2.6) |
| Caucasian | back of hand | 60.1 (5.5) | 7.4 (1.9) | 14.7 (2.4) |
| Caucasian | inner arm | 63.0 (5.5) | 5.6 (1.9) | 14.0 (2.9) |
| Caucasian | all | 60.5 (5.6) | 9.0 (3.6) | 14.6 (2.6) |
| Chinese | forehead | 56.4 (3.2) | 11.7 (2.1) | 16.3 (1.4) |
| Chinese | cheek | 58.9 (3.1) | 11.4 (2.1) | 14.2 (1.5) |
| Chinese | back of hand | 57.9 (3.5) | 9.3 (1.9) | 16.1 (1.6) |
| Chinese | inner arm | 60.9 (3.4) | 7.0 (1.7) | 15.0 (1.8) |
| Chinese | all | 58.5 (3.7) | 9.8 (2.7) | 15.4 (1.8) |
| Kurdish | forehead | 56.1 (4.5) | 11.3 (2.1) | 16.4 (2.2) |
| Kurdish | cheek | 58.0 (4.4) | 11.7 (2.3) | 15.8 (2.1) |
| Kurdish | back of hand | 57.3 (5.5) | 8.6 (1.9) | 16.5 (1.9) |
| Kurdish | inner arm | 60.6 (4.8) | 6.5 (1.6) | 16.4 (2.3) |
| Kurdish | all | 58.0 (5.1) | 9.5 (2.9) | 16.2 (2.2) |
| Thai | forehead | 56.8 (4.1) | 11.6 (2.2) | 17.7 (1.8) |
| Thai | cheek | 60.7 (4.0) | 10.5 (2.3) | 17.2 (2.1) |
| Thai | back of hand | 57.6 (4.5) | 9.4 (1.7) | 19.0 (1.8) |
| Thai | inner arm | 61.9 (3.7) | 7.1 (1.7) | 17.4 (2.0) |
| Thai | all | 59.3 (4.6) | 9.7 (2.6) | 17.9 (2.1) |

Within-group spread, Table 3 (mean ΔE*ab ± SD from the group mean, all sites): Caucasian 6.2 ± 3.5; Chinese 4.3 ± 2.3; Kurdish 5.5 ± 3.0; Thai 5.0 ± 2.6.

**Lu, Xiao, Li, Pointer 2026**, *Skin Res Technol* 32(3):e70343, Table 2: mean (SD) over the ISSA spectra, 18–75 y, multiple body sites. **CC BY 4.0.** https://www.ebi.ac.uk/europepmc/webservices/rest/PMC13097458/fullTextXML (DOI 10.1111/srt.70343). I found this through a Grok lead and confirmed it in the XML.

| Group | Subjects (F/M); spectra | L* | a* | b* | C* | ΔE*ab vs group mean |
|---|---|---|---|---|---|---|
| Caucasian (CA) | 352 (274/78); 2376 | 61.2 (5.4) | 11.0 (3.9) | 14.7 (2.7) | 18.7 (3.2) | 6.3 (3.5) |
| Chinese (CN) | 370 (177/193); 2736 | 59.8 (4.3) | 10.1 (2.9) | 16.7 (2.4) | 19.7 (2.6) | 5.2 (2.4) |
| Japanese (JP) | 118 (67/51); 944 | 63.5 (4.2) | 9.8 (2.7) | 17.0 (2.6) | 19.8 (2.5) | 5.1 (2.3) |
| South Asian, Pakistani (SA) | 157 (92/65); 1289 | 52.7 (6.4) | 10.4 (2.5) | 17.9 (2.3) | 20.8 (2.4) | 6.4 (3.4) |
| African (AF) | 145 (85/60); 929 | 39.6 (6.7) | 10.2 (2.0) | 14.4 (4.3) | 17.7 (4.4) | 6.9 (4.5) |
| Iraqi (IQ) | 149 (77/72); 1183 | 57.3 (5.5) | 10.2 (3.2) | 15.8 (2.4) | 19.0 (2.7) | 6.0 (3.2) |
| Thai (TH) | 708 (494/214); 4112 | 56.4 (5.6) | 10.0 (2.4) | 19.0 (2.6) | 21.6 (2.7) | 5.8 (3.1) |
| Arabian (AB) | 107 (87/20); 963 | 60.2 (6.5) | 10.9 (3.0) | 17.9 (2.5) | 21.1 (2.7) | 6.5 (3.9) |

- 89.4% of samples (95% CI 81.5–91.9%) have a counterpart in another ethnic group with ΔE below the JND.
- The median shared gamut is 60.5% (95% CI 54.5–63.6%).
- N = 14,532 spectra.

My arithmetic, ITA° from the Lu 2026 means:

| Group | ITA° |
|---|---|
| CA | 37.3 |
| CN | 30.4 |
| JP | 38.5 |
| SA | 8.6 |
| AF | −35.8 |
| IQ | 24.8 |
| TH | 18.6 |
| AB | 29.7 |

These are computed from group means, not mean ITA.

## 3. Iris colour

**Liu et al. 2010**, *PLoS Genet* 6(5):e1000934, Table 1, HSB measured from digital photos. Hue is in degrees on a 0–360° wheel; saturation runs 0–1. CC BY. https://doi.org/10.1371/journal.pgen.1000934 (full text: Europe PMC PMC2865509).

| Cohort | n | Hue mean ± SD | Saturation mean ± SD |
|---|---|---|---|
| RS1 (Rotterdam, age 74.0 ± 8.2) | 2429 | 34.86 ± 7.62 | 0.40 ± 0.13 |
| RS2 | 1535 | 32.19 ± 7.25 | 0.44 ± 0.14 |
| RS3 | 1987 | 32.27 ± 6.93 | 0.48 ± 0.15 |
| TwinsUK | 2261 | 19.22 ± 18.44 | 0.47 ± 0.19 |
| BTNS (Brisbane) | 1282 | 27.99 ± 6.38 | 0.54 ± 0.24 |

Categorical shares, same Table 1. The four columns are probably RS1, RS2, RS3 and BTNS; the table layout is ambiguous, see Sources disagree.

| Category | col 1 | col 2 | col 3 | col 4 |
|---|---|---|---|---|
| Blue | 68.01% | 69.51% | 71.26% | 53.51% |
| Intermediate | 9.76% | 4.82% | 7.50% | 28.39% |
| Brown | 22.23% | 25.67% | 21.24% | 18.10% |

Five-grade split (RS1 / RS2 / RS3): dark blue 4.12 / 3.52 / 2.62%; grey-light blue 57.93 / 52.83 / 41.62%; green-brown spots 18.98 / 18.89 / 31.71%; light brown 18.03 / 23.13 / 22.35%; dark brown 0.95 / 1.63 / 1.71%. Hue and saturation correlate at r = −0.77 (same paper). The paper explains the large TwinsUK SD itself: TwinsUK eye colour came from low-resolution portrait photos taken under varying daylight, while the RS and BTNS cohorts had high-resolution full-eye photos.

**Edwards et al. 2016**, *Pigment Cell Melanoma Res* 29(2):141–162, DOI 10.1111/pcmr.12435, CC BY. It measures iris colour in CIELAB in European, East Asian and South Asian samples. **Abstract only; I did not get the numeric means.** https://doi.org/10.1111/pcmr.12435

## 4. Hair colour and pigment

| Measure | Population / n | Values | Source / table | Licence | URL |
|---|---|---|---|---|---|
| Self-reported natural hair colour | UK Biobank, white British, n = 343,884 | Red 3.8% M / 5.2% F / 4.6% all. Blonde 10.0 / 12.7 / 11.5. Light brown 40.0 / 42.5 / 41.1. Dark brown 37.2 / 37.2 / 37.2. Black 7.4 / 1.5 / 4.2. Other or NA 2.0 / 0.9 / 1.4 | Morgan et al. 2018, *Nat Commun* 9:5271, Table 1 | CC BY | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC6288091/fullTextXML |
| Hair tress CIELAB | Japanese males aged 4–72, n = 42 | L* 15.19 ± 1.22 (12.3–18.0); a* 1.98 ± 0.76 (0.5–4.0); b* 2.72 ± 0.95 (0.9–5.5). Means and SD are **my arithmetic** from the per-subject values. L* falls with age (paper: p = 0.0035). | Itou et al. 2022, *Int J Mol Sci* 23:14459, Suppl. Table S1 | CC BY | https://doi.org/10.3390/ijms232214459 |
| Hair tone on a 1 (darkest) to 10 scale | Multi-country, n = 1,922 | Counts for tones 1–10: 231, 489, 456, 245, 178, 166, 86, 55, 14, 2. 74% are tones 1–4. | Loussouarn et al. 2016, *Eur J Dermatol* 26(2):144–154, Table 3 | Publisher copyright (JLE); facts only | https://www.jle.com/fr/revues/ejd/e-docs/diversity_in_human_hair_growth_diameter_colour_and_shape._an_in_vivo_study_on_young_adults_from_24_different_ethnic_groups_observed_in_the_five_continents_306744/article.phtml?tab=texte |

My arithmetic: sRGB of the Itou mean (L* 15.19, a* 1.98, b* 2.72) ≈ **#2a2522**.

## 5. Lip colour

| Study | Population | L* | C* | h° | Source / table | Licence | URL |
|---|---|---|---|---|---|---|---|
| Charton et al. 2025, *Int J Cosmet Sci* 48:161–171 | 514 women aged 19–68 (mean 42.5 ± 12.8), SpectraFace. African American n = 103 | 33.14 | 16.16 | 36.75 | group means, Results | **CC BY-NC-ND** | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC12877972/fullTextXML |
| same | Caucasian n = 207 (100 French, 107 American) | 45.96 | 26.56 | 31.93 | same | same | same |
| same | Hispanic n = 100 | 42.48 | 25.06 | 35.52 | same | same | same |
| same | Chinese n = 104 | 42.36 | 24.99 | 35.46 | same | same | same |
| same | Overall range | 23–56 | — | 20–52 | same; the chart has 53 tones, and Hispanic women covered 32 of them | same | same |
| Vergnaud et al. 2024, *Skin Res Technol* 30(2) | 410 women | 41.9 ± 6.6 (24.6–54.4) | 23.6 ± 6.0 (4.9–35.1) | 34.0 ± 4.7 (20.8–48.2) | Results; Table 2 has 11 HAC clusters | **CC BY-NC-ND** | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC10823443/fullTextXML |

Charton 2025 SDs (from the Results text):

| | African American | Hispanic | Caucasian | Asian |
|---|---|---|---|---|
| L* SD | 4.99 | 4.03 | 3.31 | 3.16 |
| C* SD | 6.06 | 3.47 | 3.06 | 3.04 |
| h SD | 3.28 | 4.79 | 4.18 | 4.42 |

- Measurement conditions: D65 illuminant, 10° observer, bare lower lip.
- Tones each group covers on the 53-tone chart: Asian 23, African American 25, Caucasian 27, Hispanic 32.
- Shared tones (Table 1):

| | Asian | Caucasian | Hispanic |
|---|---|---|---|
| African American | 10 | 4 | 15 |
| Asian | — | 15 | 21 |
| Caucasian | — | — | 17 |

- The 4 tones shared by African American and Caucasian women are the 4 tones common to all four groups.
- I first got the SDs and overlaps from the Grok pass and then confirmed them in the XML.

My arithmetic, LCh → Lab → sRGB (D65):
- African American ≈ a* 12.9, b* 9.7 → #66463f
- Caucasian ≈ a* 22.5, b* 14.0 → #975e56
- Hispanic → #8b574d
- Chinese → #8b574d
- Vergnaud mean → #88564e

## Licence summary
- **CC BY (reuse OK with attribution):** MST scale, Lu 2026, MST-E, Liu 2010, Edwards 2016, Morgan 2018, Itou 2022, Xiao 2017, ISSA 2025 paper, Alipour 2026.
- **NC/ND, flagged:** Charton 2025 and Vergnaud 2024 lip papers are **CC BY-NC-ND**. Cite the numbers as facts; do not reproduce their tables or figures.
- **Copyrighted, facts only:** Loussouarn 2016 (JLE) and Chardon 1991.

## Verification note
- Every number above was read from the full text, a supplement or a table at the URL given, except the rows marked "abstract only".
- **Grok live-search pass** (grok-4.6, xhigh, live search; prompt /workspace/grok-runs-B/B7.txt; ran to exit about 10:58 AEDT). Its claims were used only as leads.
  - **Added after I confirmed them in the primary source:**
    - Xiao 2017 open PDF (Tables 1–3)
    - Lu 2026 Table 1–2 with 8-group CIELAB means and SDs
    - Charton 2025 SDs, tone coverage and overlap table, D65/10°
    - Liu 2010 explanation of the TwinsUK spread
  - **Matched what I already had:** MST hex and Lab (Grok cites Wikipedia, Wikidata and Cheng 2024), Liu Table 1, Morgan Table 1, Vergnaud and Charton means.
  - **Contradicted:** see Sources disagree (Charton licence; MST hex variants).

## Not found
- Skin L*a*b* for women only, by ethnicity: De Rigal 2007/2010 and Del Bino 2006/2013 not opened. Del Bino 2013 reports cheek ITA by country only as histograms.
- Edwards 2016 iris CIELAB means.
- A standard melanin index (eumelanin / pheomelanin µg/mg) by hair colour from an open source.
- Iris CIELAB in Edwards 2012 and Norton 2016, and the Seddon 1990 grading. Not opened.
- Natural hair CIELAB by population (Vaughn 2008; Lozano 2017). Not opened.
- Ito & Wakamatsu 2011 hair eumelanin/pheomelanin µg/mg table. Not opened. Grok's "22.2 µg/mg black / 4.7 blond" is a tertiary figure and was **not used**.
- The illuminant for the MST site Lab (only inferred, see §1).

## Sources disagree
- **ITA category names vs Alipour Table 3:** Alipour 2026 Table 3 gives numeric ITA bands and a Fitzpatrick I–VI mapping only; it does not print the names very light / light / intermediate / tan / brown / dark. Those names are cited from Charlton et al. 2020 Table 1, which attributes them to Del Bino et al. (Del Bino 2013 and Chardon 1991 not opened). Alipour notes that the cut-offs came from lighter-skinned cohorts. Treat the ITA-to-Fitzpatrick mapping as approximate.
- **Liu 2010 Table 1 layout:** five cohorts carry hue and saturation, but only four columns carry categorical shares and three carry the five-grade shares. I could not confirm which cohort each column belongs to from the XML.
- **TwinsUK hue SD (18.44) vs 6–8 in the other cohorts:** the paper says this comes from low-resolution portrait photos taken in variable daylight. It is a method artefact, not a population difference.
- **Lip L* range:** Charton 2025 gives 23–56 and Vergnaud 2024 gives 24.6–54.4. These are different cohorts, so the ranges overlap without contradicting each other.
- **Charton 2025 licence (Grok vs source):** Grok said to "treat as CC BY". The article XML says **CC BY-NC-ND 4.0** (https://creativecommons.org/licenses/by-nc-nd/4.0/). The source wins, so the NC/ND flag stays.
- **MST hex variants (Grok lead, not confirmed):** Grok reports that a commercial web guide (itechguides, said to quote an FDA summary) lists hex values 1 RGB unit off for tones 1, 2, 3, 4, 6, 7, 8 and 10 (e.g. #F7EDE4 vs #f6ede4). The official skintone.google bundle matches the values in §1. Use the official values.
- **MST lightness is not monotonic:** the official Lab gives Monk 03 L* 93.091 > Monk 02 L* 92.275 (visible in §1). Grok attributes the same observation to Cheng 2024 (*J Imaging Sci Technol* 68(5):050404), which I did not open.
- **Facial a* across ethnic groups:** Xiao 2017 finds facial a* within measurement error across groups. Xiao's own discussion notes that De Rigal reported Caucasian women about 10% redder than Chinese women. The studies differ in sex mix, residency and instruments.
- **Caucasian L* in two papers from the same group:** 60.5 (5.6) (Xiao 2017, n = 187, 4 sites) vs 61.2 (5.4) (Lu 2026, n = 352, the larger ISSA sample). These are overlapping but different samples.
- **Xiao 2017 Table 3 caption** says "three ethnic groups" but lists four.

## Sources
1. Monk Skin Tone Scale, Google — https://skintone.google (values from https://skintone.google/assets/tono-color-block-arranger.73c427e1.js; licence statement from https://skintone.google/assets/tono-get-started.62a4df4e.js), accessed 2026-11-16.
2. Schumann C. et al. 2023, MST-E, arXiv 2305.09073 — https://arxiv.org/abs/2305.09073, accessed 2026-11-16.
3. Alipour et al. 2026, Skin Res Technol, PMC13459198 — https://www.ebi.ac.uk/europepmc/webservices/rest/PMC13459198/fullTextXML, accessed 2026-11-16.
3a. Charlton M, Stanley S.A., et al. 2020, *PLoS ONE* 15(11):e0241843, Table 1 (ITA category names, attributed there to Del Bino et al.) — https://www.ebi.ac.uk/europepmc/webservices/rest/PMC7688144/fullTextXML, accessed 2026-11-17. CC BY 4.0.
4. Xiao K. et al. 2017, Skin Res Technol 23(1):21–29 — https://doi.org/10.1111/srt.12295, accessed 2026-11-16.
5. Lu Y, Xiao K, Pointer M, et al. The International Skin Spectra Archive (ISSA)… *Sci Data* 2025;12:487, PMC11930939 — https://www.ebi.ac.uk/europepmc/webservices/rest/PMC11930939/fullTextXML, accessed 2026-11-16.
6. Liu F. et al. 2010, PLoS Genet 6(5):e1000934 — https://doi.org/10.1371/journal.pgen.1000934, accessed 2026-11-16.
7. Edwards M. et al. 2016, Pigment Cell Melanoma Res 29(2):141–162 — https://doi.org/10.1111/pcmr.12435, accessed 2026-11-16.
8. Morgan M.D. et al. 2018, Nat Commun 9:5271, PMC6288091 — https://www.ebi.ac.uk/europepmc/webservices/rest/PMC6288091/fullTextXML, accessed 2026-11-16.
9. Itou T. et al. 2022, Int J Mol Sci 23:14459, PMC9693441 — https://doi.org/10.3390/ijms232214459, accessed 2026-11-16.
10. Loussouarn G. et al. 2016, Eur J Dermatol 26(2):144–154 — JLE URL above, accessed 2026-11-16.
11. Charton et al. 2025, Int J Cosmet Sci 48:161–171, PMC12877972 — https://www.ebi.ac.uk/europepmc/webservices/rest/PMC12877972/fullTextXML, accessed 2026-11-16.
12. Vergnaud et al. 2024, Skin Res Technol 30(2), PMC10823443 — https://www.ebi.ac.uk/europepmc/webservices/rest/PMC10823443/fullTextXML, accessed 2026-11-16.
13. Lu Y, Xiao K, Li C, Pointer M. Skin colour does not define ethnicity… *Skin Res Technol* 2026;32(3):e70343, PMC13097458 — https://www.ebi.ac.uk/europepmc/webservices/rest/PMC13097458/fullTextXML (DOI 10.1111/srt.70343), accessed 2026-11-16.
14. Xiao K. et al. 2017, open PDF (White Rose eprints) — https://eprints.whiterose.ac.uk/id/eprint/100718/7/Xiao_et_al-2017-Skin_Research_and_Technology.pdf, accessed 2026-11-16.
