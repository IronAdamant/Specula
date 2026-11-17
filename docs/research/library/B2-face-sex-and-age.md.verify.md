# B2 verification: male vs female faces, and age

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `B2-face-sex-and-age.md`. B2 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/B2-face-sex-and-age.md`
- Size: 21,671 bytes, 243 lines
- File mtime: **2026-11-16 09:57:51 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 10:24 AEDT**. Bit-for-bit identical to a snapshot taken at the start of this check.
- Grok CLI: tried first (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`). It exited at once with **“Not signed in”**. **No login was attempted.** Checks used curl, WebFetch, WebSearch, the ANSUR / NIOSH / Kesterke copies already verified for B1, and new copies under `/tmp/b2verify/`. Access date for live URLs: **2026-11-16**.

## Summary

Unit of counting: **one data row** (a table row). Multi-number rows were checked number-by-number, including population, sex, age and n; a single wrong figure makes the whole row FIX. Means in Table 2 are quoted to 1 decimal; they were compared with the official Table S1–S6 means rounded **half-up** (the usual hand-rounding rule). Licence wording is in the source table, not in the row counts.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **89** | The cited source opens (or an open copy does), and it states that number, units, and population/sex/age/n. |
| **FIX** | **9** | A stated 1-decimal mean is rounded wrongly. Correction below. |
| **UNVERIFIABLE** | **0** | — |
| **Total data rows** | **98** | Table 1: 23; Table 2 means (M/F): 34; Table 2 Cohen’s d: 17; Table 3: 6; Table 4: 17; Table 5: 1. |

**Most serious problems**

1. **FIX — two adult (22–25) female means in Table 2 are wrong under any rounding rule.** Official Table S6: mandibular width **96.55** mm and morphological facial height **118.35** mm. Half-up and banker’s rounding both give **96.6** and **118.4**. B2 writes **96.5** and **118.3**. These two cells are the 100% baseline for the female “% of 22–25 mean” column on those measures (the integer percentages themselves still land on the printed values if computed from the exact means).
2. **FIX — seven other Table 2 means use banker’s rounding (x.x5 → even tenth) rather than half-up.** Source values ending in exactly 5 were written as the even tenth: e.g. male palpebral fissure 3–6 is 24.25 → B2 **24.2** (half-up **24.3**); female max facial width 13–15 is 129.25 → B2 **129.2** (half-up **129.3**). Full list in the row table. Cohen’s d values and (where checked) the integer % columns are not affected, because the percentages were computed from the exact means, not from the rounded display values (e.g. female nasal width 13–15 and 16–18 both display as 32.8 but are 101% and 102%).
3. **Licence: Kwon et al. 2021 is correctly flagged CC BY-NC 4.0 (non-commercial).** Confirmed on the article’s permissions block and on PMC (http://creativecommons.org/licenses/by-nc/4.0). Do not treat it as free for a commercial engine. No used source is wrongly marked as free. ANSUR is US government work; NIOSH is © Taylor & Francis, facts cited (same caveat as B1: do not lump the journal article with ANSUR as “government work,” though B2’s Licence section is already correct); Kesterke is CC BY 4.0 plus the BMC CC0 waiver for data in the article; WHO tables are correctly treated as copyrighted, facts cited; Kramer is © Elsevier.
4. **Helpful page correction, not a numerical error:** B2 cites Kwon as “33(1):52 (start page; end page not checked).” PMC cites **33(1):52–60** (published 2020-12-30). Honest about not checking the end page.

**What is in good shape.** All Table 1 ANSUR and NIOSH means match the sources already opened for B1, and every “M vs F” percentage recomputes half-up to one decimal. All 17 Cohen’s d rows match Tables S1–S6 exactly, as do the great majority of growth means and the group n for cranial length (90/90, 108/96, 46/53, 43/53, 96/185, 212/349). All six WHO head-circumference medians and SDs match the official 0–5 year percentile PDFs, and the “% of ANSUR II adult” column is correctly labelled as approximate arithmetic across populations. All 17 Kwon Table 1 means, SDs, p values and the “Change young→old” arithmetic check out, including the summary figures (fissure height −28%, nose +11%, philtrum +20%, lower vermilion −13%, total face height n.s.). The Kramer abstract wording is fair; unused pooled d is correctly withheld. Heathcote’s 0.22 mm/year figure is correctly left unused. Sources disagree (eye-fissure vs Korean intercanthal U-shape 36.3→34.0→35.7 mm; nose width 39.6 vs 32.4 mm; vermilion thinning) match the sources.

## Row table

Columns: row id, claim (short), status, note/correction, URL checked. Access date 2026-11-16 throughout.

### Table 1 — Adult sex difference (ANSUR II and NIOSH)

Population/n/age as in B1 (ANSUR working database 4082 M / 1986 F, 17–58; NIOSH 2543 M / 1454 F, 18–66, weighted). Means match the source tables. “M vs F” is (M−F)/F, half-up to 1 decimal, and is correctly labelled as the author’s arithmetic.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| T1-A1–A10 | ANSUR means and %: head length 19.95 vs 18.98 +5.1%; breadth 15.43 vs 14.78 +4.4%; circ. 57.44 vs 56.11 +2.4%; bizygomatic 14.26 vs 13.38 +6.6%; menton–sellion 12.26 vs 11.31 +8.4%; IPD 6.40 vs 6.17 +3.7%; ear length 6.42 vs 5.94 +8.1%; ear breadth 3.61 vs 3.32 +8.7%; ear protrusion 2.32 vs 2.04 +13.7%; neck 39.76 vs 32.98 +20.6% | VERIFIED | All ten means and percentages match. | [ANSUR PDF](https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf) (200), pages as cited |
| T1-N1–N13 | NIOSH means and %: stature 1753.9 vs 1625.4 +7.9%; head length 197.3 vs 187.5 +5.2%; breadth 153.0 vs 146.8 +4.2%; bizygomatic 143.5 vs 135.1 +6.2%; bigonial 120.4 vs 110.1 +9.4%; menton–sellion 122.7 vs 113.4 +8.2%; subnasale–sellion 52.0 vs 48.2 +7.9%; nose breadth 36.6 vs 33.2 +10.2%; nose protrusion 21.1 vs 19.8 +6.6%; lip length 51.1 vs 48.0 +6.5%; IPD 64.5 vs 61.9 +4.2%; min. frontal 105.5 vs 102.9 +2.5%; nasal root 16.6 vs 16.3 +1.8% | VERIFIED | All thirteen means and percentages match Table VI. | [CDC Stacks PDF](https://stacks.cdc.gov/view/cdc/190020/cdc_190020_DS1.pdf) (curl 403; text retrieved for B1) |

The opening summary’s rounded sketches (+4%, +5%, +2.4%, IPD +4%, bigonial +9%, nose +10%, face length +8%, ears ~8%, stature ~8%, vermilion d = 0.05, nasal ala d = 2.0, lower facial depth d = 2.1) follow these tables and B1 Table 3. “Head size ~90–94% of young-adult by 3–6” is a little loose (male cranial length is **89%**); not enough to reclass a row.

### Table 2 — Kesterke et al. 2016, Tables S1–S6

Population: US 3DFN, self-identified European ancestry; age groups as in article Table 1. Cranial-length n in B2 matches the per-measure n in S1–S6 (90/90, 108/96, 46/53, 43/53, 96/185, 212/349). Means compared at 1 decimal, **half-up**. Percentages are the author’s arithmetic and match integer rounding of *exact* source means. Cohen’s d matches the official d column in every cell.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| T2-M, all but three | Male means (17 measures × 6 ages) other than the three rows below | VERIFIED | Official ESM DOCX Tables S1–S6. | [DOI](https://doi.org/10.1186/s13293-016-0076-8) → Springer (200); official ESM DOCX (200) |
| T2-M intercanthal | 29.0, **31.2**, 32.0, 32.9, 32.4, 33.0 | **FIX** | 7–12 exact **31.25** → half-up **31.3** (B2 used banker’s 31.2). % column still OK. | same, Table S2 |
| T2-M palpebral fissure a | **24.2**, 26.4, 28.6, 28.7, 29.0, 28.6 | **FIX** | 3–6 exact **24.25** → half-up **24.3**. | Table S1 |
| T2-M upper lip height | **18.2**, 19.3, 21.0, 22.1, 21.8, 22.0 | **FIX** | 3–6 exact **18.25** → half-up **18.3**. | Table S1 |
| T2-F, all but six | Female means other than the six rows below | VERIFIED | | same |
| T2-F max facial width | 113.2, 123.0, **129.2**, 130.6, 130.3, 129.2 | **FIX** | 13–15 exact **129.25** → half-up **129.3**. | Table S3 |
| T2-F mandibular width | 81.0, 89.9, 94.7, 95.8, 96.6, **96.5 (100%)** | **FIX** | 22–25 exact **96.55** → **96.6** (both rounding rules). | Table S6 |
| T2-F morph. facial height | 96.1, 106.7, 113.6, 116.3, 118.8, **118.3 (100%)** | **FIX** | 22–25 exact **118.35** → **118.4** (both rounding rules). | Table S6 |
| T2-F intercanthal | 28.8, 31.1, **31.6**, 31.4, 31.2, **31.2** | **FIX** | 13–15 exact **31.65** → **31.7**; 22–25 exact **31.25** → **31.3**. | Tables S3, S6 |
| T2-F palpebral fissure a | 23.7, 25.7, **27.6**, 28.2, 28.2, 27.7 | **FIX** | 13–15 exact **27.65** → **27.7**. | Table S3 |
| T2-F nasal protrusion | 14.7, 16.9, 19.0, 19.7, **20.2**, 19.9 | **FIX** | 19–21 exact **20.25** → **20.3**. | Table S5 |
| T2-d 1–17 | All 17 Cohen’s d rows (six ages each), including negatives (nasal bridge 7–12 −0.01; nasal protrusion 3–6 −0.09; labial fissure 13–15 −0.01; upper vermilion 3–6 −0.01 and 7–12 −0.05) and adult vermilion 0.05 / 0.05 | VERIFIED | Every cell matches S1–S6. “Positive = larger in males” is right for this table. | ESM DOCX |

### Table 3 — WHO Child Growth Standards, head circumference 0–5 years

Population: WHO Multicentre Growth Reference Study (stated on the standards). M and SD are the printed LMS columns, not the displayed 50th-percentile one-decimal values (e.g. birth boys 50th prints 34.5 cm; B2 correctly uses M = 34.4618, SD = 1.2703). “% of adult” is the author’s arithmetic against ANSUR II 57.44 / 56.11 and is correctly flagged as cross-population.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| W1 | Birth: boys 34.4618 (SD 1.2703), girls 33.8787 (1.1844); 60% / 60% | VERIFIED | Month 0. 34.4618/57.44 and 33.8787/56.11 both round to 60%. | [boys PDF](https://cdn.who.int/media/docs/default-source/child-growth/child-growth-standards/indicators/head-circumference-for-age/boys-table--head-circumference-for-age--birth-to-5-years-(percentile).pdf?sfvrsn=cf858375_2) (200); [girls PDF](https://cdn.who.int/media/docs/default-source/child-growth/child-growth-standards/indicators/head-circumference-for-age/hcfa_girls_0_5_percentiles.pdf?sfvrsn=127cbaec_7) (200) |
| W2 | 12 mo: 46.0661 (1.2848) / 44.8965 (1.3590); 80% / 80% | VERIFIED | | same |
| W3 | 24 mo: 48.2515 (1.3612) / 47.1822 (1.3952); 84% / 84% | VERIFIED | | same |
| W4 | 36 mo: 49.4612 (1.4200) / 48.5099 (1.4126); 86% / 86% | VERIFIED | | same |
| W5 | 48 mo: 50.2115 (1.4622) / 49.3321 (1.4198); 87% / 88% | VERIFIED | 50.2115/57.44=87.4%→87%; 49.3321/56.11=87.9%→88%. | same |
| W6 | 60 mo: 50.7375 (1.4947) / 49.9229 (1.4228); 88% / 89% | VERIFIED | | same |

Landing page https://www.who.int/tools/child-growth-standards/standards/head-circumference-for-age returned 200. The PDFs themselves have no separate licence line; B2’s “© WHO; facts cited; check WHO copyright before redistributing the tables” is the right caution (WHO’s copyright policy page requires permission to reproduce tables).

### Table 4 — Kwon et al. 2021, Korean women, Table 1

Population confirmed: 192 Korean women, BMI 18.5–24.9, histories of facial deformity, craniofacial syndromes, severe asymmetry or previous plastic surgery excluded; younger 20–39 (mean 26.4±5.0, n=48), middle 40–59 (51.3±5.3, n=56), older 60–79 (69.0±6.2, n=88); Morpheus 3D. Cross-sectional. Source p-values printed 0.000* are written <0.001, which is fair. “Change” is the author’s arithmetic (old−young, and % of the young mean where given) and matches half-up. Palpebral width/height and upper-eyelid height in the source are **means of left and right**; B2 names the landmarks but does not say “mean of both sides” — a small omission, not a wrong number.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| K1 | Palpebral fissure width 26.26±1.58 / 25.56±1.48 / 23.46±1.67, p<0.001; −2.8 mm (−11%) | VERIFIED | 23.46−26.26=−2.80; −2.80/26.26=−10.7%→−11%. | [DOI](https://doi.org/10.5021/ad.2021.33.1.52); [PMC7875215](https://pmc.ncbi.nlm.nih.gov/articles/PMC7875215/) (200); Europe PMC full-text XML (200). Licence CC BY-NC 4.0. |
| K2 | Palpebral fissure height 9.16±1.18 / 8.00±1.31 / 6.61±1.24, p<0.001; −2.6 mm (−28%) | VERIFIED | 6.61−9.16=−2.55→−2.6; −27.8%→−28%. | same, Table 1 |
| K3 | Upper eyelid height 13.42±2.33 / 14.07±2.54 / 15.42±2.82, p<0.001; +2.0 mm | VERIFIED | 15.42−13.42=2.00. | same |
| K4 | Binocular width 88.81±4.11 / 85.15±4.14 / 82.29±5.81, p<0.001; −6.5 mm | VERIFIED | 82.29−88.81=−6.52→−6.5. | same |
| K5 | Interpupillary width 61.28±2.99 / 60.70±2.68 / 60.94±2.67, p=0.557; n.s. | VERIFIED | | same |
| K6 | Nasal width 39.59±2.30 / 40.52±2.57 / 43.83±2.78, p<0.001; +4.2 mm (+11%) | VERIFIED | 43.83−39.59=4.24→4.2; +10.7%→+11%. | same |
| K7 | Nasal height 51.88±2.74 / 51.26±2.94 / 52.02±3.27, p=0.326; n.s. | VERIFIED | | same |
| K8 | Upper-facial height 53.64±4.92 / 51.48±5.28 / 51.19±5.68, p=0.034; −2.5 mm | VERIFIED | 51.19−53.64=−2.45→−2.5. | same |
| K9 | Lower-facial height 62.60±4.27 / 65.39±3.58 / 65.72±4.05, p<0.001; +3.1 mm | VERIFIED | 65.72−62.60=3.12→3.1. | same |
| K10 | Philtrum height 12.94±2.09 / 15.15±1.61 / 15.47±2.04, p<0.001; +2.5 mm (+20%) | VERIFIED | 15.47−12.94=2.53→2.5; +19.6%→+20%. | same |
| K11 | Upper lip height 19.85±2.11 / 22.09±1.64 / 22.49±2.17, p<0.001; +2.6 mm | VERIFIED | 22.49−19.85=2.64→2.6. | same |
| K12 | Upper vermilion 6.91±1.26 / 6.94±1.19 / 7.03±1.26, p=0.853; n.s. | VERIFIED | | same |
| K13 | Lower vermilion 9.58±1.41 / 9.11±1.55 / 8.32±1.68, p<0.001; −1.3 mm (−13%) | VERIFIED | 8.32−9.58=−1.26→−1.3; −13.2%→−13%. | same |
| K14 | Mouth width 44.45±2.87 / 45.87±3.20 / 47.77±3.51, p<0.001; +3.3 mm | VERIFIED | 47.77−44.45=3.32→3.3. | same |
| K15 | Total facial height 187.05±8.20 / 187.65±7.39 / 188.60±8.02, p=0.634; n.s. | VERIFIED | | same |
| K16 | Nasofrontal angle 145.61±5.02 / 146.55±4.78 / 148.07±4.88, p=0.015; +2.5° | VERIFIED | 148.07−145.61=2.46→2.5°. | same |
| K17 (pop) | n=192; groups 48 / 56 / 88 with means 26.4±5.0, 51.3±5.3, 69.0±6.2; BMI 18.5–24.9; Morpheus 3D | VERIFIED | Counted with the table rather than as a separate 18th measure row; included here so population/sex/age/n is on the record. (Not added to the 17 numeric rows in the total.) | same Methods |

Intercanthal width is not in B2’s Table 4 but is correctly quoted in Sources disagree as a U-shape **36.3 → 34.0 → 35.7 mm** (source 36.29±3.17, 34.03±2.68, 35.71±3.41, p=0.001).

### Table 5 — Kramer 2017 fWHR meta-analysis

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| F1 | Abstract-level finding: skulls, a small male > female fWHR difference; faces, little evidence of dimorphism overall; no difference in White samples; medium but non-significant effect in East Asian samples (3 samples). Pooled d not used. | VERIFIED | Matches the published abstract (Lincoln repository page and ResearchGate abstract text). Citation 38(3):414–420 is right. © Elsevier. | [DOI](https://doi.org/10.1016/j.evolhumbehav.2016.12.002) (landing 200); [Lincoln record](https://repository.lincoln.ac.uk/articles/journal_contribution/Sexual_dimorphism_of_facial_width-to-height_ratio_in_human_skulls_and_faces_A_meta-analytical_approach/24366790) (the file itself 403 to curl; abstract text confirmed via search extract). Crossref confirms title, journal, volume, issue, pages. |

## Source table

| Source | Licence as stated in B2 | Licence check | URL status (2026-11-16) | Notes |
|---|---|---|---|---|
| ANSUR II (Gordon et al. 2014) | US government work, approved for public release | **Confirmed** (see B1). | PDF 200 | Means re-used from the same PDF. |
| Zhuang & Bradtmiller 2005 | © Taylor & Francis; facts cited | **Confirmed** (see B1). | CDC Stacks curl 403; T&F DOI 500 | Means match Table VI. |
| Kesterke et al. 2016 | CC BY 4.0; data CC0 | **Confirmed.** Article is CC BY 4.0; BMC/Springer Open CC0 waiver applies to data made available in the article (tables + Additional file 1). Not a licence for FaceBase individual-level data (not used). | DOI → Springer 200; ESM DOCX 200; figshare 403 | All S1–S6 figures from the official DOCX. |
| WHO Child Growth Standards HC-for-age tables | © WHO; facts cited; check WHO copyright before redistributing tables | **Fair and conservative.** The PDFs have no licence line; WHO’s copyright policy requires permission to reproduce tables. Numbers are facts. | Both PDFs 200; landing 200; copyright policy page 200 | M and SD verified at months 0, 12, 24, 36, 48, 60. |
| Kwon et al. 2021, *Ann Dermatol* 33(1):52 | **CC BY-NC 4.0 — non-commercial**; facts cited; do not copy text/figures into a commercial product | **Confirmed** on PMC and in the Europe PMC XML permissions: “Creative Commons Attribution Non-Commercial License (http://creativecommons.org/licenses/by-nc/4.0)”. CC BY-NC 4.0 page 200. End page is **60**. | DOI resolves to anndermatol.org (302); PMC7875215 200; XML 200 | All Table 1 numbers checked from the XML full text. |
| Kramer 2017, *Evol Hum Behav* | © Elsevier; repository copy; finding cited as fact | **Confirmed** (Elsevier journal; abstract used only). | DOI landing 200; Lincoln record exists, file 403 to curl | Qualitative abstract claims verified; unused d withheld honestly. |
| Heathcote 1995, *BMJ* 311:1668 | Existence only; number not confirmed | Honest. DOI exists; BMJ page 403 to curl (paywall). PubMed record 8541753 exists. The ~0.22 mm/year figure is correctly unused. | DOI 403 at bmj.com | — |

## Notes that are not FIXes

- Kwon palpebral and eyelid heights are means of left and right sides in Table 1; B2 does not say so.
- Kwon end page 60 was not claimed, only marked unchecked.
- Male cranial length at 3–6 is 89% of the 22–25 mean, a shade under the summary’s “~90–94%.”
- No number in this check came from the grok CLI, and no number was invented.
