# H3 — What makes a standing figure look alive: breathing, postural sway, weight shifts, small movements, blinks

**Changelog**
- 2026-11-17 (AEDT): per-row access dates added. Every table row now ends with a "Source URL · accessed" cell (or "Accessed" in the source list), and every numeric prose line ends with its source URL and access date. Rows checked on 2026-11-16 carry 2026-11-16; rows re-checked today carry 2026-11-17.
- 2026-11-17 09:21 AEDT (second Library Verify pass, verify file 09:15): (1) Tobin 1983 pooled rate **restored to 16.6 ± 2.8 breaths/min** — confirmed on the page image of p. 203 (Results: "The average f for all the normals was 16.6±2.8 breaths/min"); the earlier "no pooled SD" wording and the Tobin row in Sources disagree are removed. (2) Duarte & Freitas: 0.5 Hz reworded as the low-pass filter cut-off (p. 186), not sway frequency; quiet-stance signal "below 10 Hz" is on printed p. **187** (the verify note said p. 186). (3) Elble 2003: 9–12 Hz under load scoped to 8 of 100 young adults (PubMed abstract). Also the matching tremor row in Sources disagree.
- 2026-11-17 (earlier; item 1 superseded by the entry above): Library Verify FIXes applied after re-opening the primary source. (1) Tobin 1983 breathing rate: the pooled "16.6 ± 2.8 (all)" is gone. Table 1 gives 16.7 ± 2.7 young and 16.6 ± 2.8 older; the Discussion states 16.6 breaths/min observed overall **with no SD**, so the older-group SD is no longer attached to the pooled value. (2) Parreira 2010 (T2) licence: now **CC BY-NC 4.0 International**, as stated on the SciELO article page. (3) HBEDB sway frequency cells were **not changed**: Library Verify's recompute differs by 0.01–0.04 Hz under an unstated spectral method, which is a method difference, not a primary-source conflict. Both versions are now in "Sources disagree"; treat those cells as ±0.04 Hz.


Researched 2026-11-16 for the M6 motion libraries (brief item H3). All links accessed **2026-11-16**.
Every number was read from the source named in its row, or computed by me from an open dataset read in `/tmp` and deleted afterwards. Nothing was downloaded into any repo. The grok CLI passes could not run (the CLI is not signed in on this box), so all research was done with direct search and by reading the primary sources.
Link status: `OK` = opened from this box and the number was read there.

---

## 1. Breathing at rest

| Measure | Value (mean ± SD) | Population, posture, method | Source | Source URL · accessed |
|---|---|---|---|---|
| Breathing rate | **All normals: 16.6 ± 2.8 breaths/min** (Results, p. 203: "The average f for all the normals was 16.6±2.8 breaths/min"); young 16.7 ± 2.7; older 16.6 ± 2.8 (Table 1) | 65 healthy adults: 47 young (28.6 ± 5.3 y) and 18 older (68.9 ± 6.5 y); **supine**; respiratory inductive plethysmography (RIP) | Tobin et al. 1983, *Chest* 84:202–205, Table 1 and Results p. 203 | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf · 2026-11-17 |
| Tidal volume | 383 ± 91 mL (all); young 383 ± 85; older 382 ± 108 | same | same | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf · 2026-11-16 |
| Minute ventilation | 6.01 ± 1.39 L/min | same | same | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf · 2026-11-16 |
| Inspiratory time Ti | 1.62 ± 0.31 s | same | same | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf · 2026-11-16 |
| Ti / Ttot (fraction of the breath spent breathing in) | 0.421 ± 0.033 | same | same | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf · 2026-11-16 |
| Rib-cage share of tidal volume | 42 ± 17 % | same, supine | same (Results text) | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf · 2026-11-16 |
| Age effect | the mean values of the breathing components did not change with age, but breathing rhythm was more irregular in older subjects | same | Tobin 1983, abstract | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf · 2026-11-16 |
| Breathing rate by sex and age | 20–39 y: women 15 ± 2, men 13 ± 4; 40–59 y: women 14 ± 2, men 16 ± 3; 60–80 y: women 15 ± 2, men 15 ± 3 breaths/min | 104 adults (63 F, 41 M), normal BMI, **supine**, RIP | Parreira et al. 2010, *Rev Bras Fisioter* 14(5):411–416, Table 2 | https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en · 2026-11-16 |
| Tidal volume by sex and age | 20–39: F 325 ± 127, M 441 ± 114 mL; 40–59: F 309 ± 111, M 325 ± 115 mL; 60–80: F 283 ± 83, M 383 ± 124 mL | same | same | https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en · 2026-11-16 |
| Ti / Ttot by sex and age | 20–39: F 0.39 ± 0.03, M 0.42 ± 0.04; 40–59: F 0.39 ± 0.03, M 0.41 ± 0.02; 60–80: F 0.38 ± 0.04, M 0.39 ± 0.04 | same | same | https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en · 2026-11-16 |
| Rib-cage share of tidal volume (%RC) | 20–39: F 46 ± 15, M 39 ± 10 %; 40–59: F 40 ± 9, M 32 ± 7 %; 60–80: F 45 ± 18, M 37 ± 14 % | same | same | https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en · 2026-11-16 |
| Rib cage vs abdomen phase angle | 20–39: 11 ± 6°; 40–59: 14 ± 7°; 60–80: 15 ± 8° (larger in older adults) | same | Parreira 2010, Table 3 | https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en · 2026-11-16 |
| Chest-wall volume change per quiet breath, **standing** | women 0.93 ± 0.18 L; men 1.21 ± 0.17 L (sitting 0.92 / 1.16; supine 0.48 / 0.64) | 28 adults (14 F, 14 M; 31.7 ± 6.2 y); optoelectronic plethysmography; 15 s of quiet breathing per posture | Shaghayeghfard et al. 2024, *J Biomed Phys Eng* 14(1):67–78, Table 5 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10862111/ · 2026-11-16 |
| Standing compartment split (women / men) | upper (pulmonary) rib cage 0.34 / 0.40 L; lower (abdominal) rib cage 0.24 / 0.32 L; abdomen 0.34 / 0.42 L | same | same, Tables 2–4 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10862111/ · 2026-11-16 |
| Surface displacement, upper rib cage (3rd–5th costal cartilage), per quiet breath | mostly ventral (forward) and cranial (upward), **3–5 mm**; lateral 1–2 mm | 5 healthy adults, **seated**, 36 markers, ELITE motion analyser | De Groote et al. 1997, *J Appl Physiol* 83:1531–1537, abstract | https://pubmed.ncbi.nlm.nih.gov/9375316/ · 2026-11-16 |
| Surface displacement, umbilicus level | almost entirely ventral (amount not given in the abstract) | same | same | https://pubmed.ncbi.nlm.nih.gov/9375316/ · 2026-11-16 |
| Posture effect on rib cage vs abdomen | rib-cage dominance is greatest **standing** and least supine | 120 healthy subjects, 10 to >60 y, RIP | Verschakelen & Demedts 1995, *Am J Respir Crit Care Med* 151:399–405, abstract | https://pubmed.ncbi.nlm.nih.gov/7842198/ · 2026-11-16 |

Generator notes (derived from the rows above, not separate facts): breath period ≈ 60/16 ≈ 3.75 s (range from ±1 SD of rate: about 3.2–4.5 s); inhale ≈ 40–42 % of the cycle and exhale ≈ 58–60 %. When standing, the upper and lower chest move a bit more than the belly. Visible front-surface motion is a few millimetres per quiet breath. [https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://pubmed.ncbi.nlm.nih.gov/21180867/ · accessed 2026-11-16]

## 2. Postural sway in quiet standing (centre of pressure, COP)

### 2a. Computed from the open Human Balance Evaluation Database (HBEDB)

Data: Santos & Duarte 2016, *PeerJ* 4:e2648; PhysioNet `hbedb` v1.0.0 (DOI 10.13026/C2WW2W). 163 subjects: young 18–59 y (n = 87; 56 F, 31 M; 27.8 ± 7.8 y) and older 60–86 y (n = 76; 60 F, 16 M; 71.3 ± 6.5 y). Trial footwear not checked (the metadata records each subject's usual footwear). Feet on marked positions, rigid force plate, **60 s trials at 100 Hz**, already filtered by the authors at 10 Hz; x = anterior–posterior (AP), y = medio-lateral (ML). [https://physionet.org/content/hbedb/1.0.0/ · accessed 2026-11-16]
**How I computed it:** for each subject, the first rigid-surface trial with eyes open and the first with eyes closed. I removed the mean, then computed RMS, range, mean speed (path length ÷ 60 s), mean and median power frequency (0.05–5 Hz), and the 95 % confidence-ellipse area. The table gives mean ± SD across subjects, with the 5th–95th percentile in brackets. 16 subjects reported a disability (4 young, 12 older); they were kept in. [https://physionet.org/content/hbedb/1.0.0/ · accessed 2026-11-16]

| Measure | Young, eyes open | Young, eyes closed | Older, eyes open | Older, eyes closed | Source URL · accessed |
|---|---|---|---|---|---|
| RMS AP (mm) | 4.79 ± 2.46 [2.35–8.81] | 5.28 ± 2.86 [2.57–8.25] | 4.89 ± 2.32 [2.66–9.39] | 4.84 ± 2.11 [2.84–7.92] | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |
| RMS ML (mm) | 2.89 ± 1.44 [1.13–5.38] | 3.06 ± 1.51 [1.45–6.12] | 3.05 ± 1.21 [1.65–5.49] | 3.08 ± 1.48 [1.33–5.91] | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |
| Range AP (mm) | 24.0 ± 10.5 [12.5–40.1] | 27.6 ± 12.8 [13.0–44.2] | 27.0 ± 11.3 [14.0–44.5] | 28.4 ± 13.3 [14.7–48.2] | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |
| Range ML (mm) | 15.9 ± 7.0 [6.6–26.2] | 16.8 ± 7.8 [8.2–33.4] | 17.0 ± 7.1 [9.2–32.0] | 17.7 ± 8.4 [8.1–36.3] | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |
| Mean COP speed, total (mm/s) | 9.05 ± 3.01 [5.36–15.37] | 10.12 ± 4.14 [6.08–18.69] | 12.74 ± 6.02 [6.90–25.21] | 13.99 ± 6.80 [6.55–26.68] | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |
| Mean speed AP / ML (mm/s) | 6.59 ± 2.23 / 4.78 ± 1.94 | 7.46 ± 3.16 / 5.21 ± 2.37 | 9.94 ± 4.85 / 5.95 ± 2.92 | 11.14 ± 5.62 / 6.24 ± 3.26 | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |
| Mean frequency AP / ML (Hz) | 0.33 ± 0.13 / 0.39 ± 0.14 | 0.34 ± 0.13 / 0.39 ± 0.14 | 0.43 ± 0.18 / 0.45 ± 0.19 | 0.43 ± 0.15 / 0.47 ± 0.18 | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |
| Median frequency AP / ML (Hz) | 0.20 ± 0.14 / 0.29 ± 0.15 | 0.23 ± 0.13 / 0.30 ± 0.15 | 0.33 ± 0.22 / 0.40 ± 0.25 | 0.32 ± 0.19 / 0.40 ± 0.23 | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |
| Frequency below which 80 % of AP power lies (Hz) | 0.48 ± 0.23 | 0.50 ± 0.23 | 0.65 ± 0.29 | 0.66 ± 0.26 | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |
| 95 % ellipse area (mm²) | 274 ± 289 (median 204) [51–685] | 318 ± 366 (median 217) [75–815] | 295 ± 295 (median 218) [89–583] | 284 ± 268 (median 216) [74–607] | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-16 |

Reading for a generator: an idle figure's centre of mass wanders a few millimetres (RMS about 3–5 mm front–back and 2–3 mm side to side). Most of the power is below about 0.5 Hz. The total range over a minute is about 1.5–3 cm front–back and 1–2 cm side to side. Older bodies sway at about the same size but faster and at higher frequency. Area is very skewed (log-normal-like), so seed it from the median and the percentiles rather than mean ± SD. [https://physionet.org/content/hbedb/1.0.0/ · accessed 2026-11-16]

### 2b. Published reference values (COP speed)

| Measure | Value (mean ± SD) | Population | Source | Source URL · accessed |
|---|---|---|---|---|
| Total COP speed, eyes open | women 9.2 ± 1.6 mm/s; men 9.2 ± 2.0 mm/s | 100 young adults (50 F, 50 M), 19–28 y (21 ± 2 y), Kistler plate, 40 Hz, 5 Hz low-pass, 5 × 60 s trials | Błaszczyk & Beck 2023, *J Hum Kinet* 86:7–15, Table 1 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10170532/ · 2026-11-16 |
| Total COP speed, eyes closed | women 11.2 ± 2.4; men 12.4 ± 3.5 mm/s | same | same | https://pmc.ncbi.nlm.nih.gov/articles/PMC10170532/ · 2026-11-16 |
| AP speed, EO / EC | women 6.39 ± 1.0 / 7.95 ± 1.6; men 6.5 ± 1.35 / 9.14 ± 2.6 mm/s | same | same, Results text | https://pmc.ncbi.nlm.nih.gov/articles/PMC10170532/ · 2026-11-16 |
| ML speed, EO / EC | women 5.2 ± 1.2 / 6.2 ± 1.8; men 5.2 ± 1.4 / 6.4 ± 2.1 mm/s | same | same | https://pmc.ncbi.nlm.nih.gov/articles/PMC10170532/ · 2026-11-16 |
| Quiet-stance COP signal bandwidth (not a sway frequency) | Components of the CP signal in quiet standing of normal subjects are "below 10 Hz" (p. 187). The "about 0.5 Hz" figure on p. 186 is the **cut-off of the low-pass filter** used to estimate the centre-of-gravity displacement from the COP (inverted-pendulum filtering method), **not** the sway frequency | review | Duarte & Freitas 2010, *Rev Bras Fisioter* 14(3):183–192, pp. 186–187 | https://www.scielo.br/j/rbfis/a/hFQTppgw4q3jGBCDKV9fdCH/?format=pdf&lang=en · 2026-11-17 |

## 3. Weight shifts and fidgeting in prolonged unconstrained standing

| Measure | Value (mean ± SD) | Population | Source | Source URL · accessed |
|---|---|---|---|---|
| "Shifting" (a fast move of the average COP position to a new place), AP | one every **316 ± 292 s**; amplitude **17 ± 15 mm** | 10 young healthy adults, >30 min free standing | Duarte & Zatsiorsky 1999, *Motor Control* 3:12–27, abstract | https://pubmed.ncbi.nlm.nih.gov/9924098/ · 2026-11-16 |
| Shifting, ML | one every **199 ± 148 s**; amplitude **22 ± 38 mm** | same | same | https://pubmed.ncbi.nlm.nih.gov/9924098/ · 2026-11-16 |
| "Fidgeting" (a fast large COP move and return to about the same place), AP | one every **59 ± 15 s** | same | same | https://pubmed.ncbi.nlm.nih.gov/9924098/ · 2026-11-16 |
| Fidgeting, ML | one every **49 ± 16 s** | same | same | https://pubmed.ncbi.nlm.nih.gov/9924098/ · 2026-11-16 |
| "Drifting" (slow continuous move of the average position), drift-to-drift interval | AP **319 ± 173 s**; ML **529 ± 333 s** | same | same | https://pubmed.ncbi.nlm.nih.gov/9924098/ · 2026-11-16 |

For a generator: layer the three patterns on top of the quiet sway in §2. Fidget about once a minute, shift weight to a new stance every 3–5 minutes, and add a slow drift with a period of several minutes. The SDs are large, so draw intervals from a skewed distribution. [https://pubmed.ncbi.nlm.nih.gov/9924098/ · accessed 2026-11-16]

## 4. Small hand movements (physiological tremor)

| Measure | Value | Population | Source | Source URL · accessed |
|---|---|---|---|---|
| Postural hand tremor, peak acceleration at the dominant frequency | range 0.0015–0.035 g; most common 0.01 g; visible threshold about 0.007 g | 97 normal subjects, 15–80 y, accelerometer | Wade, Gresty & Findley 1982, *Arch Neurol* 39:358–362, abstract | https://pubmed.ncbi.nlm.nih.gov/7092612/ · 2026-11-16 |
| Dominant tremor frequency (same study) | about 7 Hz up to age 70, then about 6 Hz; within-subject variability range 2.5 Hz | same | same | https://pubmed.ncbi.nlm.nih.gov/7092612/ · 2026-11-16 |
| Hand tremor frequency | mean 7.7 Hz unloaded; 5.2 Hz with added weight; finger tremor bands 1–4, 6–11 and 15–30 Hz (whole arm, hand and finger resonances) | 117 normal subjects, 20–94 y | Raethjen et al. 2000, *Clin Neurophysiol* 111:1825–1837, abstract | https://pubmed.ncbi.nlm.nih.gov/11018499/ · 2026-11-16 |
| Central tremor component | 9–12 Hz EMG peak during 300 g mass loading — seen in **only 8 of 100 young adults** (those 8 of the 12 with a well-defined tremor-coherent EMG peak whose peak frequency fell < 1 Hz with loading); only 2 of 100 older adults showed 9–12 Hz (the other 5 such older adults had 5–7 Hz). No tremor-related EMG peak in 59 young and 65 older | 100 young (20–42 y; 59 F, 41 M) and 100 older (70–92 y; 50 F, 50 M) adults | Elble 2003, *Clin Neurophysiol* 114:624–635, abstract | https://pubmed.ncbi.nlm.nih.gov/12686271/ · 2026-11-17 |

Tremor is far too small to see on a game-scale figure (≈0.01 g at 6–8 Hz gives a displacement of only tens of micrometres; this is my arithmetic, not a source value). It belongs in the "Not found" list for anything visible. Visible small hand movements come from fidgeting (§3), not tremor. [https://pubmed.ncbi.nlm.nih.gov/7092612/ ; https://pubmed.ncbi.nlm.nih.gov/11018499/ · accessed 2026-11-16]

## 5. Blinks

| Measure | Value | Population | Source | Source URL · accessed |
|---|---|---|---|---|
| Blink rate at rest | 17 blinks/min (mean; SD not in abstract) | 150 healthy volunteers (70 M, 80 F), 35.9 ± 17.9 y (5–87 y), videotaped | Bentivoglio et al. 1997, *Mov Disord* 12:1028–1034, abstract | https://pubmed.ncbi.nlm.nih.gov/9399231/ · 2026-11-16 |
| Blink rate in conversation | 26 blinks/min (+99.7 % vs rest) | same | same | https://pubmed.ncbi.nlm.nih.gov/9399231/ · 2026-11-16 |
| Blink rate while reading | 4.5 blinks/min (−55 % vs rest) | same | same | https://pubmed.ncbi.nlm.nih.gov/9399231/ · 2026-11-16 |
| Distribution | log-normal (best fit) | same | same | https://pubmed.ncbi.nlm.nih.gov/9399231/ · 2026-11-16 |
| Individual patterns | conversation > rest > reading in 67.3 %; rest > conversation > reading in 22.7 %; conversation > reading > rest in 8.0 % | same | same | https://pubmed.ncbi.nlm.nih.gov/9399231/ · 2026-11-16 |
| Age / sex | no age effect; women blink more than men only while reading | same | same | https://pubmed.ncbi.nlm.nih.gov/9399231/ · 2026-11-16 |
| Spontaneous blink duration | closing about 100 ms, opening about 220 ms, total about 320 ms (averages; no SD in abstract) | 11 healthy men, 1 kHz camera, 109 blinks (39 complete, 70 incomplete closures) | Nakamura et al. 2008, *Nippon Ganka Gakkai Zasshi* 112:1059–1067, abstract | https://pubmed.ncbi.nlm.nih.gov/19157026/ · 2026-11-16 |
| Peak lid speed, closing / opening | about 180 / 60 mm/s (complete blinks); 120 / 45 mm/s (incomplete) | same | same | https://pubmed.ncbi.nlm.nih.gov/19157026/ · 2026-11-16 |
| Share of incomplete blinks | 70 of 109 (64 %) | same | same | https://pubmed.ncbi.nlm.nih.gov/19157026/ · 2026-11-16 |

## 6. Sources and licences

| # | Source | Licence / use status | Link (accessed 2026-11-16) | Status | Accessed |
|---|---|---|---|---|---|
| T1 | Tobin MJ, Chadha TS, Jenouri G, Birch SJ, Gazeroglu HB, Sackner MA. *Breathing patterns. 1. Normal subjects.* Chest 84(2):202–205 (1983), doi 10.1378/chest.84.2.202 | Closed paper; facts cited. Table read from a third-party PDF copy | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; copy read: https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf | OK | 2026-11-16 |
| T2 | Parreira VF, Bueno CJ, França DC, Vieira DS, Pereira DR, Britto RR. *Breathing pattern and thoracoabdominal motion in healthy individuals: influence of age and sex.* Rev Bras Fisioter 14(5):411–416 (2010), doi 10.1590/S1413-35552010000500010 | ⚠ **CC BY-NC 4.0 International** (stated on the SciELO article page, checked 2026-11-17: "This work is licensed under a Creative Commons Attribution-NonCommercial 4.0 International License"); non-commercial, facts cited only | https://pubmed.ncbi.nlm.nih.gov/21180867/ ; PDF: https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?format=pdf&lang=pt ; article page with licence: https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en | OK | 2026-11-17 |
| T3 | Shaghayeghfard B, Karimi MT, Abbasi L, Razeghi M. *Respiratory function assessment through kinematic analysis of chest wall movements: effects of position and gender.* J Biomed Phys Eng 14(1):67–78 (2024), doi 10.31661/jbpe.v0i0.2105-1335 | ⚠ **CC BY-NC 4.0** (non-commercial) | https://pmc.ncbi.nlm.nih.gov/articles/PMC10862111/ | OK | 2026-11-16 |
| T4 | De Groote A, Wantier M, Chéron G, Estenne M, Paiva M. *Chest wall motion during tidal breathing.* J Appl Physiol 83(5):1531–1537 (1997) | © APS; abstract facts cited | https://pubmed.ncbi.nlm.nih.gov/9375316/ ; https://doi.org/10.1152/jappl.1997.83.5.1531 (publisher page 403 to this box) | OK (PubMed) | 2026-11-16 |
| T5 | Verschakelen JA, Demedts MG. *Normal thoracoabdominal motions.* Am J Respir Crit Care Med 151:399–405 (1995) | Closed; abstract facts | https://pubmed.ncbi.nlm.nih.gov/7842198/ | OK | 2026-11-16 |
| T6 | Santos DA, Duarte M. *A public data set of human balance evaluations.* PeerJ 4:e2648 (2016) | Article **CC BY 4.0** | https://doi.org/10.7717/peerj.2648 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC5101613/ | OK (PMC) | 2026-11-16 |
| T6-data | HBEDB on PhysioNet, v1.0.0, doi 10.13026/C2WW2W (files `BDSinfo.txt` and 326 trial records `.dat/.hea`, read in /tmp then deleted) | **Open Data Commons Attribution (ODC-By)**: commercial use and derived results are allowed with attribution | https://physionet.org/content/hbedb/1.0.0/ | OK | 2026-11-16 |
| T7 | Błaszczyk JW, Beck M. *Posturographic standards for optimal control of human standing posture.* J Hum Kinet 86:7–15 (2023), doi 10.5114/jhk/159452 | **CC BY 4.0** | https://pmc.ncbi.nlm.nih.gov/articles/PMC10170532/ | OK | 2026-11-16 |
| T8 | Duarte M, Freitas SM. *Revision of posturography based on force plate for balance evaluation.* Rev Bras Fisioter 14(3):183–192 (2010), doi 10.1590/S1413-35552010000300003 | SciELO open access, licence not verified; fact cited | https://www.scielo.br/j/rbfis/a/hFQTppgw4q3jGBCDKV9fdCH/?format=pdf&lang=en | OK | 2026-11-16 |
| T9 | Duarte M, Zatsiorsky VM. *Patterns of center of pressure migration during prolonged unconstrained standing.* Motor Control 3(1):12–27 (1999), doi 10.1123/mcj.3.1.12 | Closed; abstract facts | https://pubmed.ncbi.nlm.nih.gov/9924098/ | OK | 2026-11-16 |
| T10 | Wade P, Gresty MA, Findley LJ. *A normative study of postural tremor of the hand.* Arch Neurol 39:358–362 (1982) | Closed; abstract facts | https://pubmed.ncbi.nlm.nih.gov/7092612/ | OK | 2026-11-16 |
| T11 | Raethjen J et al. *Determinants of physiologic tremor in a large normal population.* Clin Neurophysiol 111:1825–1837 (2000) | Closed; abstract facts | https://pubmed.ncbi.nlm.nih.gov/11018499/ | OK | 2026-11-16 |
| T12 | Elble RJ. *Characteristics of physiologic tremor in young and elderly adults.* Clin Neurophysiol 114:624–635 (2003) | Closed; abstract facts | https://pubmed.ncbi.nlm.nih.gov/12686271/ | OK | 2026-11-16 |
| T13 | Bentivoglio AR et al. *Analysis of blink rate patterns in normal subjects.* Mov Disord 12(6):1028–1034 (1997), doi 10.1002/mds.870120629 | Closed; abstract facts | https://pubmed.ncbi.nlm.nih.gov/9399231/ | OK | 2026-11-16 |
| T14 | Nakamura Y et al. *Measurement of spontaneous blinks with a high-speed blink analyzing system* (in Japanese). Nippon Ganka Gakkai Zasshi 112(12):1059–1067 (2008) | Closed; English abstract facts | https://pubmed.ncbi.nlm.nih.gov/19157026/ | OK | 2026-11-16 |

**Licence red flags for H3:** T3 is CC BY-NC (cite facts only). Most other breathing, tremor and blink numbers come from closed-paper abstracts (fine to cite as facts). The only open dataset used (HBEDB, ODC-By) allows commercial use with attribution.

## Not found

- **Breathing rate and tidal volume measured standing** (as opposed to supine). T1 and T2 are supine. T3 has standing volumes but no rate. No standing breathing-rate norm with SD was confirmed.
- **Chest and abdomen surface excursion in mm, with SD, standing:** only De Groote 1997 (seated, n = 5, a range of 3–5 mm with no SD). Kaneko & Horie 2012 (*Respir Care* 57:1442, 100 subjects, supine and sitting) has these numbers, but the full text is closed; not read.
- **Head sway amplitude in quiet standing** (mm RMS at the head): no open source with values was confirmed. Head sway is larger than COP sway in an inverted-pendulum body, but no number was verified.
- **Small head movements and gaze shifts at rest** (frequency, amplitude) and **hand fidget rates**: not found in a citable source.
- **SD of blink rate and blink duration:** the abstracts give means only (Bentivoglio 1997; Nakamura 2008).
- **Visible tremor displacement in mm:** only acceleration values were found (T10). The displacement estimate in §4 is my arithmetic and is not sourced.
- **Weight-shift data in older adults:** Duarte & Zatsiorsky 1999 is young adults only (n = 10).

## Sources disagree

| Quantity | Source A | Source B | Note | Source URL · accessed |
|---|---|---|---|---|
| HBEDB sway frequency cells (§2a mean, median and 80 % power frequency) | This file's computation (0.05–5 Hz after mean removal) | Library Verify's independent recompute: e.g. older eyes-closed AP F80 0.70 ± 0.28 Hz vs 0.66 ± 0.26 here; other cells differ by 0.01–0.04 Hz | Spectral estimator, window and padding were not fixed in either method; treat frequency cells as ±0.04 Hz. Spatial and speed cells match exactly | https://physionet.org/content/hbedb/1.0.0/ · 2026-11-17 |
| Tidal volume at rest | Tobin 1983: 383 ± 91 mL (supine) | Shaghayeghfard 2024: chest-wall volume change 0.48–0.64 L supine and 0.93–1.21 L standing | T3's values are much larger than other quiet-breathing norms even supine; its OEP volumes may include more than tidal air, or the breathing was not truly quiet. Use T1/T2 for volume and T3 only for the *ratio* standing/supine (≈1.9×) | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf ; https://pmc.ncbi.nlm.nih.gov/articles/PMC10862111/ · 2026-11-16 |
| Breathing rate, young men | Parreira 2010: 13 ± 4 breaths/min | Tobin 1983: 16.7 ± 2.7 (young, mixed sex) | Small samples, different labs; both supine | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf ; https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en · 2026-11-16 |
| Rib-cage share of breathing | Tobin 1983: 42 ± 17 % (supine) | Parreira 2010: 32–46 % by group (supine); Verschakelen 1995: rib cage dominates when standing | Posture changes the split strongly | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf ; https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en ; https://pubmed.ncbi.nlm.nih.gov/7842198/ · 2026-11-16 |
| COP speed, young, eyes open | HBEDB (my computation): 9.05 ± 3.01 mm/s (100 Hz, 10 Hz filter, ages 18–59) | Błaszczyk 2023: 9.2 ± 1.6–2.0 mm/s (40 Hz, 5 Hz filter, 19–28 y) | They agree on the mean; HBEDB has a wider spread because its "young" group spans 18–59 y. COP speed depends on sampling rate and filter (Duarte & Freitas 2010) | https://physionet.org/content/hbedb/1.0.0/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC10170532/ ; https://www.scielo.br/j/rbfis/a/hFQTppgw4q3jGBCDKV9fdCH/?format=pdf&lang=en · 2026-11-16 |
| Effect of eye closure on AP sway in older adults | HBEDB: RMS AP unchanged (4.89 vs 4.84 mm) while speed rose | Usual finding: sway grows with eyes closed (Błaszczyk 2023 speed +22–34 %) | RMS over 60 s is dominated by slow drift; speed is the more sensitive measure | https://physionet.org/content/hbedb/1.0.0/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC10170532/ · 2026-11-16 |
| Dominant physiological tremor frequency | Wade 1982: about 7 Hz (6 Hz after age 70) | Elble 2003: 9–12 Hz loaded EMG peak, but only in 8 of 100 young adults; Raethjen 2000: 7.7 Hz mean, and no age effect | Different measures (mechanical resonance peak vs EMG-coherent component) and different hand loading | https://pubmed.ncbi.nlm.nih.gov/7092612/ ; https://pubmed.ncbi.nlm.nih.gov/11018499/ ; https://pubmed.ncbi.nlm.nih.gov/12686271/ · 2026-11-16 |
