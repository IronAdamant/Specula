# H3 verification: idle life motion

Verifier: Library Verify (adversarial re-check). Written 2026-11-16; finalised 2026-11-17, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `H3-idle-life-motion.md`. H3 itself was not edited.

## Final status (2026-11-17, completes the 2026-11-16 partial pass)

**This section replaces the 2026-11-16 Summary and row statuses below wherever they differ. It also retracts one of my own earlier FIXes (R1, Tobin).** Times are AEDT (UTC+11).

### Version verified against
- Snapshot at the start of today's work: mtime **2026-11-16 10:14:42 AEDT** (unchanged since yesterday).
- Motion Data edited H3 at **2026-11-17 08:53:51 AEDT** (27,240 bytes). The edit added per-row access dates, a Changelog, the Tobin rewrite, the Parreira licence change to CC BY-NC 4.0, and new Sources-disagree rows (Tobin pooling, HBEDB frequency ±0.04 Hz).
- I diffed the new file against the snapshot and re-checked only those rows.
- Re-stat before writing: **2026-11-17 08:53:51 AEDT**, byte-identical to my post-edit copy. **This is the final mtime verified against.**

### Counts (50 data rows)

| Status | Rows | Rows |
|---|---:|---|
| **VERIFIED** | **44** | 2026-11-16 VERIFIED rows except Q4 (26). Added today: W1–W6, Q1–Q3, B1–B9 (18). |
| **FIX** | **3** | R1 (Tobin, now wrong in the other direction), P5 (Duarte & Freitas 0.5 Hz), Q4 (Elble scope) |
| **UNVERIFIABLE** | **3** | V7–V9 (HBEDB spectral cells: method not stated, so they cannot be reproduced exactly; now disclosed in H3 "Sources disagree") |

Note: V7–V9 were FIX yesterday. H3 now discloses the 0.01–0.04 Hz gap and the unstated method in "Sources disagree", so I re-classed them as UNVERIFIABLE, not wrong. The older-EC F80 cell (0.66 vs my 0.70) is still the largest gap.

### Worst problems
1. **R1 Tobin 1983: my 2026-11-16 FIX was wrong, and H3 has now copied it.** I re-read the PDF page as an image today (p. 203, Results, column 2): *"The average f for all the normals was **16.6±2.8 breaths/min**, VT 383±91 ml and V̇E 6.01±1.39 L/min."*
   - The paper **does** print a pooled SD. H3's current "no pooled SD is printed" and the new Sources-disagree row ("earlier versions … wrongly attached the older-group SD") are **incorrect**.
   - **Correction:** "All adults: **16.6 ± 2.8 breaths/min** (Results, p. 203); 16.7 ± 2.7 young, 16.6 ± 2.8 older (Table 1)."
   - Remove the Tobin pooling row from Sources disagree, or rewrite it to say the pooled and older values coincide.
   - Yesterday's error came from an OCR text layer that scrambled that sentence.
2. **P5 Duarte & Freitas 2010.** The "about 0.5 Hz" sentence describes the **cutoff of the CP→CG low-pass filter** in the inverted-pendulum method: *"The cutoff frequency of this lowpass filter is determined by the anthropometric characteristics of the body, and the frequency is often about 0.5 Hz."* It is not a typical sway frequency. The same paper says *"For the quiet standing posture in normal subjects, the components of the signal frequency are below 10 Hz."* The sentence is on printed **p. 186** (PDF page 4), not p. 185. The licence is CC BY-NC 4.0 International on the SciELO page.
3. **Q4 Elble 2003.** The 9–12 Hz EMG peak under load is reported *"in all 8 young adults"* whose peak frequency fell by less than 1 Hz with loading.
   - Of the 100 young adults, 59 had no tremor-related EMG peak, 29 had inconsistent peaks and 12 had a well-defined coherent peak.
   - Only 2 of the 7 older adults showed 9–12 Hz; the other 5 were at 5–7 Hz.
   - About 8% of adults show this pattern.
   - **Correction:** "9–12 Hz under load in the 8 young adults (of 100) with a load-stable coherent EMG peak; about 8% of adults show such a peak."
4. The **HBEDB spectral cells** (V7–V9) depend on the method used, as noted above.

**Dead links:** none among the H3 URLs re-checked today. PubMed or PMC HTML sometimes shows a captcha, but Europe PMC REST serves the same abstracts.

### Rows completed or changed 2026-11-17

| Row | Claim | Status | Note / correction | URL checked (2026-11-17) |
|---|---|---|---|---|
| R1 | Tobin: all adults 16.6, "no pooled SD is printed"; young 16.7±2.7; older 16.6±2.8 | **FIX** | Results p. 203 prints **16.6±2.8** for all normals. Restore the SD. See problem 1. | https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf (page 2 image) ; https://pubmed.ncbi.nlm.nih.gov/6872603/ |
| T2 licence | Parreira 2010 is CC BY-NC 4.0 International | VERIFIED | SciELO: "This work is licensed under a Creative Commons Attribution-NonCommercial 4.0 International License." | https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en |
| P5 | Duarte & Freitas: sway "often about 0.5 Hz", p. 185 | **FIX** | This is the filter cutoff, not the sway frequency; the sentence is on p. 186; quiet-stance components are "below 10 Hz". | https://www.scielo.br/j/rbfis/a/hFQTppgw4q3jGBCDKV9fdCH/?format=pdf&lang=en |
| W1 | Shifting AP: one per 316±292 s, 17±15 mm | VERIFIED | Abstract | https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=EXT_ID:9924098%20AND%20SRC:MED&resultType=core&format=json |
| W2 | Shifting ML: 199±148 s, 22±38 mm | VERIFIED | Abstract | same |
| W3 | Fidgeting AP: 59±15 s | VERIFIED | Abstract | same |
| W4 | Fidgeting ML: 49±16 s | VERIFIED | Abstract | same |
| W5 | Drifting: AP 319±173 s; ML 529±333 s | VERIFIED | Abstract (10 young adults, >30 min) | same |
| W6 | Generator advice: fidget about every minute, shift every 3–5 min | VERIFIED | Consistent with W1–W5 (59/49 s; 199–316 s) | same |
| Q1 | Wade: 0.0015–0.035 g; modal 0.01 g; visible ~0.007 g; 97 subjects aged 15–80 | VERIFIED | Abstract | https://pubmed.ncbi.nlm.nih.gov/7092612/ (Europe PMC abstract) |
| Q2 | About 7 Hz to age 70, then about 6 Hz; within-subject variability 2.5 Hz | VERIFIED | Abstract | same |
| Q3 | Raethjen: 7.7 Hz unloaded, 5.2 Hz loaded; bands 1–4, 6–11, 15–30 Hz; 117 subjects aged 20–94 | VERIFIED | Abstract | https://pubmed.ncbi.nlm.nih.gov/11018499/ |
| Q4 | Elble: 9–12 Hz under load in young adults with a tremor-coherent EMG peak | **FIX** | Scope: only the 8 young adults whose peak was load-stable (12 had coherent peaks; 100 were tested). See problem 3. | https://pubmed.ncbi.nlm.nih.gov/12686271/ |
| B1 | Bentivoglio: rest 17/min; 150 volunteers | VERIFIED | Abstract | https://pubmed.ncbi.nlm.nih.gov/9399231/ |
| B2 | Conversation 26/min | VERIFIED | Abstract | same |
| B3 | Reading 4.5/min | VERIFIED | Abstract | same |
| B4 | Reading −55.08% and conversation +99.70% vs rest | VERIFIED | Abstract | same |
| B5 | Log-normal distribution | VERIFIED | Abstract | same |
| B6 | Sex difference (women higher) only while reading; no age effect | VERIFIED | Abstract | same |
| B7 | Pattern shares 67.3 / 22.7 / 8.0 % | VERIFIED | Abstract (the remaining ~2% is not described) | same |
| B8 | Nakamura: closing ~100 ms, opening ~220 ms, total ~320 ms; 11 men, 1 kHz | VERIFIED | Abstract | https://pubmed.ncbi.nlm.nih.gov/19157026/ |
| B9 | Speeds 180/60 and 120/45 mm/s; 70 of 109 incomplete (39 complete) | VERIFIED | Abstract | same |

### Not found (additions)
- None new. The Tobin page is now read as an image, so the OCR problem is resolved.

### Sources disagree (additions and corrections)
- **Tobin pooled rate:** there is **no disagreement**. The paper's Results give 16.6±2.8 for all normals, so H3's new row should be removed.
- **Overgrowth and others:** none for H3.

### Source table (re-checked 2026-11-17)

| Key | URL | Resolves | Licence correct |
|---|---|---|---|
| T1 Tobin | https://pubmed.ncbi.nlm.nih.gov/6872603/ ; copy https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf | y | y (closed paper, third-party copy, facts cited) |
| T2 Parreira | https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en | y | y (now CC BY-NC 4.0) |
| Duarte & Freitas | https://www.scielo.br/j/rbfis/a/hFQTppgw4q3jGBCDKV9fdCH/?lang=en | y | y (CC BY-NC 4.0) |
| Duarte & Zatsiorsky 1999 | https://pubmed.ncbi.nlm.nih.gov/9924098/ | y | y (closed, abstract) |
| Wade 1982 | https://pubmed.ncbi.nlm.nih.gov/7092612/ | y | y (closed) |
| Raethjen 2000 | https://pubmed.ncbi.nlm.nih.gov/11018499/ | y | y (closed) |
| Elble 2003 | https://pubmed.ncbi.nlm.nih.gov/12686271/ | y | y (closed) |
| Bentivoglio 1997 | https://pubmed.ncbi.nlm.nih.gov/9399231/ | y | y (closed) |
| Nakamura 2008 | https://pubmed.ncbi.nlm.nih.gov/19157026/ | y | y (closed) |
| HBEDB | https://physionet.org/content/hbedb/1.0.0/ | y | y (ODC-By) |

### Grok passes
- 2026-11-16 10:47–10:56 AEDT: see below.
- **2026-11-17 08:49:22–08:57:54 AEDT ("Grok A", exit 0).**
  - It confirmed the Duarte & Freitas 0.5 Hz filter-cutoff wording and the CC BY-NC 4.0 licence; it did not recover the page number, which I took from the PDF.
  - It confirmed the Wade, Raethjen, Bentivoglio, Nakamura and Duarte & Zatsiorsky abstract numbers. I re-read each abstract from Europe PMC myself.
  - On Elble, it raised the "is 9–12 Hz stated?" question, which I settled from the abstract.
  - Grok did **not** re-check Tobin; that retraction is my own reading of the PDF image.
  - No Grok-only number is used here.

---


## Version checked (2026-11-16 partial pass)

- Path: `/workspace/specula-research/H3-idle-life-motion.md`
- Size: 19,032 bytes, **143 lines**
- File mtime: **2026-11-16 10:14:42 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 11:04 AEDT**. The file was unchanged from that mtime while the check ran.
- Access date for live URLs: **2026-11-16**.

## Grok live-search pass

A Grok CLI pass was run after sign-in: `~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve` with live search on, **10:47–10:56 AEDT**, exit 0. Numbers were not taken from Grok alone. URLs it cited were opened here (PhysioNet licence page, SciELO Parreira page, Shaghayeghfard Europe PMC XML, Kang/CISE where relevant).

**Conflicts it raised (then checked):**

1. **Shaghayeghfard 2024** PMC XML: “CC BY-NC 4.0 **Unported**”; journal authors’ note: “4.0 **International**.” Both point at `https://creativecommons.org/licenses/by-nc/4.0/`. H3’s CC BY-NC flag is right; the Unported/International label is inconsistent in the publisher’s own pages. `https://jbpe.sums.ac.ir/article_49514.html` 404s (Grok); another journal HTML had no on-page licence line.
2. **Parreira 2010** on SciELO is **CC BY-NC 4.0 International** (article footer, opened). H3 says the PDF licence was “not verified” and “usually CC BY or CC BY-NC.” That caution should now be replaced with **CC BY-NC 4.0** (non-commercial).
3. **PhysioNet HBEDB** licence: **Open Data Commons Attribution License v1.0 (ODC-By)** — agrees with H3. Page: https://physionet.org/content/hbedb/1.0.0/ and view-license.

`/tmp/grokpass` did not contain Motion Data’s PhysioNet scripts when inspected at 10:57 AEDT. Sway was recomputed independently from `BDSinfo.txt` and the `.dat/.hea` trials (first firm-surface EO and EC per subject; mean removed; 100 Hz, authors’ 10 Hz filter left in place).

## Summary (2026-11-16 partial pass — superseded by "Final status" above)

Unit of counting: **one data row**. Frequency metrics that do not reproduce under the stated method are FIX even when RMS/speed match. Licence wording is in the source table and in “Most serious problems.”

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **27** | Source opens (or an open copy / abstract does) and states that number, units, and population. |
| **FIX** | **4** | A stated number (or a pooled value not in the source) is wrong or does not reproduce. |
| **UNVERIFIABLE** | **19** | Full text not opened, or the exact figure was not re-quoted from a live abstract/PDF in this pass. |
| **Total data rows** | **50** | §1 breathing 17 (16 VERIFIED, 1 FIX); §2a sway 10 (7 VERIFIED, 3 FIX); §2b published COP 5 (4 VERIFIED, 1 UNVERIFIABLE); §3 weight-shift 5 UNVERIFIABLE (closed Duarte 1999); §4 tremor 4 UNVERIFIABLE; §5 blinks 9 UNVERIFIABLE. |

**Most serious problems**

1. **FIX — HBEDB spectral numbers do not fully reproduce.** RMS, range, mean speed (total and AP/ML), 5th–95th percentiles, and 95% ellipse area (χ² and Prieto almost identical) **match H3 to the stated rounding**, with n=87 young and n=76 older, first firm EO/EC trial. Mean power frequency, median frequency, and the 80% power frequency do **not** all match under a 0.05–5 Hz spectral calculation after mean removal. The method sentence is underspecified (window, estimator, zero-padding). Largest gap: older, eyes-closed, frequency below which 80% of AP power lies: H3 **0.66±0.26 Hz**, this recompute **0.70±0.28 Hz**. Other frequency cells are off by 0.01–0.04 Hz and/or a slightly different SD after rounding. Spatial and speed rows stay VERIFIED.
2. **[SUPERSEDED 2026-11-17: see Final status; the paper prints 16.6±2.8 on p. 203] FIX — Tobin 1983 “all adults” breathing rate 16.6±2.8 breaths/min.** Table 1 (OCR of the third-party PDF H3 used) gives young **16.7±2.7** and older **16.6±2.8**. The Results text clearly gives pooled **VT 383±91 mL** and **VE 6.01±1.39 L/min**, plus Ti **1.62±0.31 s** and Ti/Ttot **0.421±0.033**. A size-weighted mean of the two frequency cells (47 young, 18 older) is **16.7**, not 16.6, and the pooled SD is not the older-group SD. Do not copy the older cell as “all.”
3. **Licence upgrade for T2 Parreira:** SciELO article page (opened) states **Creative Commons Attribution-NonCommercial 4.0 International License**. H3 left this “not verified.” Flag **CC BY-NC** the same way as T3.
4. **T3 Shaghayeghfard is correctly CC BY-NC**; only the Unported vs International label disagrees, as above.
5. Closed-paper blink, tremor, De Groote, Verschakelen, Duarte 1999, Wade, Raethjen, Elble, Bentivoglio and Nakamura rows were checked at **abstract** level only (as H3 says). Claims that are actually in those abstracts are VERIFIED; claims that need the full paper remain UNVERIFIABLE.

**What is in good shape.** HBEDB population (163; young 18–59 n=87, older 60–86 n=76), ODC-By licence, trial selection, and every spatial/speed/area cell. Błaszczyk 2023 Table 1 / Results COP speeds. Parreira Table 2/3 cells that were grepped (rates, VT, Ti/Ttot, %RC, phase angle 11±6 / 14±7). Kang is not in H3. § Not found and § Sources disagree are present and, where checked, honest: Shaghayeghfard standing/supine volumes really are much larger than Tobin’s supine VT; HBEDB vs Błaszczyk mean COP speed really is close, with a wider HBEDB spread; older-adult AP RMS not rising with eye closure is in the recomputed data.

## Row table

Access date 2026-11-16 throughout.

### §1 — Breathing

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| R1 | Tobin rate 16.6±2.8 (all); 16.7±2.7 young; 16.6±2.8 older. 65 adults (47 young 28.6±5.3 y, 18 older 68.9±6.5 y), supine, RIP | ~~FIX~~ SUPERSEDED 2026-11-17 (16.6±2.8 is printed, p. 203) | Young/older Table 1 match. **Correction: do not report 16.6±2.8 as the pooled rate** unless the paper states that exact pooled cell (not found in the OCR). Weighted mean of the two groups is 16.7. | [third-party PDF used by H3](https://metodorta.com.br/wp-content/uploads/2016/09/Breathing-patterns-tobin-in-normal-subjects.pdf) (opened here earlier); PMID 6872603 |
| R2 | VT 383±91 mL (all); young 383±85; older 382±108 | VERIFIED | Results text + Table 1. | same |
| R3 | Minute ventilation 6.01±1.39 L/min | VERIFIED | Results (OCR also shows a nearby 6.02 in the table; 6.01 is the Results sentence). | same |
| R4 | Ti 1.62±0.31 s | VERIFIED | Results. | same |
| R5 | Ti/Ttot 0.421±0.033 | VERIFIED | Results. | same |
| R6 | Rib-cage share 42±17% (Results) | VERIFIED | Present in the Results/table region of the same PDF (OCR messy but the figure is there). | same |
| R7 | Age effect: means unchanged, rhythm more irregular in older subjects | VERIFIED | Abstract. | PMID 6872603 |
| R8 | Parreira rates 20–39 y F 15±2, M 13±4; 40–59 F 14±2, M 16±3; 60–80 F 15±2, M 15±3; 104 adults (63 F, 41 M), supine, RIP | VERIFIED | Table 2. | [SciELO](https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en) ; PDF opened |
| R9 | VT 20–39 F 325±127, M 441±114; 40–59 F 309±111, M 325±115; 60–80 F 283±83, M 383±124 mL | VERIFIED | Table 2 (20–39 and 40–59 grepped; remaining cells present in the same table). | same |
| R10 | Ti/Ttot 20–39 F 0.39±0.03, M 0.42±0.04; 40–59 F 0.39±0.03, M 0.41±0.02; 60–80 F 0.38±0.04, M 0.39±0.04 | VERIFIED | Table 2. | same |
| R11 | %RC 20–39 F 46±15, M 39±10; 40–59 F 40±9, M 32±7; 60–80 F 45±18, M 37±14 | VERIFIED | Table 2. | same |
| R12 | Phase angle 20–39 11±6°; 40–59 14±7°; 60–80 15±8° | VERIFIED | Table 3 (11±6 and 14±7 grepped; 15±8 in the same table). | same |
| R13 | Shaghayeghfard standing chest-wall volume F 0.93±0.18, M 1.21±0.17 L (sitting 0.92/1.16; supine 0.48/0.64); 28 adults, 14 F/14 M, 31.7±6.2 y | VERIFIED | Table 5, Europe PMC XML (PMC10862111). | https://www.ebi.ac.uk/europepmc/webservices/rest/PMC10862111/fullTextXML |
| R14 | Standing compartment split F/M 0.34/0.40, 0.24/0.32, 0.34/0.42 L | VERIFIED | Tables 2–4, same XML. | same |
| R15 | De Groote 1997: upper rib cage mostly ventral/cranial 3–5 mm, lateral 1–2 mm; 5 adults, seated | VERIFIED | Abstract (PMID 9375316). | https://pubmed.ncbi.nlm.nih.gov/9375316/ (abstract stored) |
| R16 | Umbilicus almost entirely ventral (amount not in abstract) | VERIFIED | H1-style honesty: amount not in the abstract, as written. | same |
| R17 | Verschakelen 1995: rib-cage dominance greatest standing, least supine; 120 subjects | VERIFIED | Abstract, PMID 7842198. | abstract |

### §2a — HBEDB sway (computed)

Data: PhysioNet hbedb v1.0.0, ODC-By v1.0; 163 subjects as stated.

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| V1 | RMS AP mm: Y EO 4.79±2.46 [2.35–8.81]; Y EC 5.28±2.86 [2.57–8.25]; O EO 4.89±2.32 [2.66–9.39]; O EC 4.84±2.11 [2.84–7.92] | VERIFIED | Recomputed: 4.79±2.46; 5.28±2.86; 4.89±2.32; 4.84±2.11. n=87/76. | https://physionet.org/content/hbedb/1.0.0/ |
| V2 | RMS ML mm: 2.89±1.44 [1.13–5.38] / 3.06±1.51 [1.45–6.12] / 3.05±1.21 [1.65–5.49] / 3.08±1.48 [1.33–5.91] | VERIFIED | Match. | same |
| V3 | Range AP mm: 24.0±10.5 [12.5–40.1] / 27.6±12.8 [13.0–44.2] / 27.0±11.3 [14.0–44.5] / 28.4±13.3 [14.7–48.2] | VERIFIED | Match. | same |
| V4 | Range ML mm: 15.9±7.0 [6.6–26.2] / 16.8±7.8 [8.2–33.4] / 17.0±7.1 [9.2–32.0] / 17.7±8.4 [8.1–36.3] | VERIFIED | Match. | same |
| V5 | Mean COP speed mm/s: 9.05±3.01 [5.36–15.37] / 10.12±4.14 [6.08–18.69] / 12.74±6.02 [6.90–25.21] / 13.99±6.80 [6.55–26.68] | VERIFIED | Match (path length / 60 s). | same |
| V6 | Speed AP/ML mm/s as written (6.59±2.23 / 4.78±1.94, etc.) | VERIFIED | Match. | same |
| V7 | Mean frequency AP/ML Hz: 0.33±0.13 / 0.39±0.14 (Y EO); 0.34±0.13 / 0.39±0.14 (Y EC); 0.43±0.18 / 0.45±0.19 (O EO); 0.43±0.15 / 0.47±0.18 (O EC) | **FIX** | This recompute (MPF, 0.05–5 Hz): **0.32±0.11 / 0.38±0.13**; **0.33±0.11 / 0.39±0.13**; **0.42±0.17 / 0.43±0.18**; **0.45±0.16 / 0.46±0.16**. Method may differ. | same, independent recompute in `/workspace/tmp/hverify/compute/hbedb_summary.json` (then raw trials to be deleted) |
| V8 | Median frequency AP/ML: 0.20±0.14 / 0.29±0.15 (Y EO); 0.23±0.13 / 0.30±0.15 (Y EC); 0.33±0.22 / 0.40±0.25 (O EO); 0.32±0.19 / 0.40±0.23 (O EC) | **FIX** | This recompute: **0.19±0.11 / 0.27±0.14**; **0.21±0.10 / 0.28±0.14**; **0.32±0.23 / 0.37±0.25**; **0.35±0.21 / 0.36±0.22**. | same |
| V9 | F80 AP Hz: 0.48±0.23 / 0.50±0.23 / 0.65±0.29 / **0.66±0.26** | **FIX** | This recompute: 0.49±0.22 / 0.50±0.21 / 0.65±0.28 / **0.70±0.28**. Young EO and older EO are close; **older EC does not match**. | same |
| V10 | 95% ellipse area mm², mean±SD (median) [p5–p95], four conditions | VERIFIED | χ² and Prieto areas both round to H3’s cells (e.g. Y EO 274±289, median 204, [51–685]). | same |

### §2b — Published COP speed

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| P1 | Błaszczyk 2023 total speed EO: women 9.2±1.6, men 9.2±2.0 mm/s; 100 young adults 19–28 y | VERIFIED | Table 1, PMC10170532 (text also has 9.24±2 mm/s for men). | https://pmc.ncbi.nlm.nih.gov/articles/PMC10170532/ |
| P2 | EC: women 11.2±2.4, men 12.4±3.5 mm/s | VERIFIED | Table 1 (text 12.38 for men, table 12.4). | same |
| P3 | AP speed EO/EC women 6.39±1.0 / 7.95±1.6; men 6.5±1.35 / 9.14±2.6 | VERIFIED | Results text. | same |
| P4 | ML speed EO/EC women 5.2±1.2 / 6.2±1.8; men 5.2±1.4 / 6.4±2.1 | VERIFIED | Results. | same |
| P5 | Duarte & Freitas 2010: typical sway frequency “often about 0.5 Hz,” p. 185 | **UNVERIFIABLE** | SciELO PDF was opened earlier in the project; this pass did not re-quote p. 185. Licence on that article was also left unverified by H3. | https://www.scielo.br/j/rbfis/a/hFQTppgw4q3jGBCDKV9fdCH/?format=pdf&lang=en |

### §3–§5 — Weight shift, tremor, blinks

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| W1–W3 | Duarte & Zatsiorsky 1999 prolonged-standing patterns (n=10 young), as given from the abstract | mixed | Abstract reached (PMID 9924098). Claims that are in the abstract: VERIFIED. Any duration, amplitude or count that is only in the full *Motor Control* paper: **UNVERIFIABLE** (closed). Counted as 3 UNVERIFIABLE if they go beyond the abstract; H3 should be read as abstract-level. | https://pubmed.ncbi.nlm.nih.gov/9924098/ |
| W4–W6 | other §3 weight-shift / fidget rows as written | **UNVERIFIABLE** | Not re-extracted against full text in this pass; treat as abstract-level pending a quoted sentence. | — |
| Q1 | Wade 1982: peak acceleration range 0.0015–0.035 g, most common 0.01 g, visible ~0.007 g; 97 subjects 15–80 y | **UNVERIFIABLE** | Abstract stored; this pass did not re-quote those exact figures from a PDF. Leave as abstract-only until the abstract text is shown to contain them. | PMID 7092612 |
| Q2 | Dominant frequency ~7 Hz to age 70, then ~6 Hz | **UNVERIFIABLE** | Same. | same |
| Q3 | Raethjen 2000: 7.7 Hz unloaded, 5.2 Hz with weight; bands 1–4, 6–11, 15–30 Hz; n=117, 20–94 y | VERIFIED as population/title; **frequency figures UNVERIFIABLE from abstract snippet stored** (abstract reached, full numbers not re-quoted here). Split: mark **UNVERIFIABLE** for the hertz values unless re-opened. | PMID 11018499 |
| Q4 | Elble 2003: 9–12 Hz EMG peak under load, 100 young (20–42) and 100 older (70–92) | VERIFIED | Abstract states those age bands and n (59 F / 41 M young; 50/50 older) and discusses tremor-related EMG peaks under a 300 g load. The exact “9–12 Hz” phrase should be confirmed in the full abstract/PDF; counted VERIFIED only if present — **UNVERIFIABLE** if not re-quoted. Conservative: **UNVERIFIABLE** for the 9–12 Hz band. | PMID 12686271 |
| B1 | Bentivoglio 1997 rest blink rate 17 /min; 150 volunteers, 70 M / 80 F, 35.9±17.9 y (5–87) | **UNVERIFIABLE** | Closed; abstract not successfully re-quoted in this pass (no local abstract file). H3 correctly says SD is not in the abstract. | PMID 9399231 |
| B2–B7 | Conversation 26 /min, reading 4.5 /min, log-normal, pattern shares 67.3/22.7/8.0%, no age effect | **UNVERIFIABLE** | Same; typical abstract content for this paper, but I did not open a copy this pass. | same |
| B8–B9 | Nakamura 2008: closing ~100 ms, opening ~220 ms, total ~320 ms; speeds 180/60 and 120/45 mm/s; 70/109 incomplete; 11 men, 1 kHz, 109 blinks | **UNVERIFIABLE** | English abstract reached (PMID 19157026) and describes the 11 men and 1 kHz camera; the millisecond and mm/s figures were not in the stored abstract excerpt. | PMID 19157026 |

Conservative recount for §3–§5 that I could not re-quote: those rows stay UNVERIFIABLE rather than invented as VERIFIED. §3–§5 are counted row-by-row in the summary (5+4+9 UNVERIFIABLE). Tremor and blink numeric rows should not be treated as freshly confirmed in this pass.

## Not found (this check)

- A printed licence in the Parreira PDF (H3 was right about the PDF); the **HTML** page does carry CC BY-NC 4.0.
- Motion Data’s `/tmp/grokpass` sway scripts (path reused).
- Full text of Wade, Elble (beyond abstract), Bentivoglio, and Nakamura (Japanese paper).
- [SUPERSEDED 2026-11-17: found on p. 203] An exact pooled Tobin frequency cell 16.6±2.8 for all 65 subjects.

## Sources disagree

- **Tidal volume:** Tobin 383±91 mL supine vs Shaghayeghfard chest-wall change 0.48–0.64 L supine and 0.93–1.21 L standing — real, and correctly flagged by H3.
- **Young male breathing rate:** Parreira 13±4 vs Tobin young mixed-sex 16.7±2.7 — real.
- **COP speed:** HBEDB 9.05±3.01 vs Błaszczyk 9.2±1.6–2.0 mm/s — means agree; spreads differ, as H3 says.
- **Eye closure vs older AP RMS:** HBEDB RMS AP unchanged (4.89 vs 4.84) while speed rises — reproduced here.
- **Tremor frequency:** Wade ~7 Hz vs Elble 9–12 Hz vs Raethjen 7.7 Hz — correctly flagged; this pass did not newly open those PDFs.
- **Shaghayeghfard licence name:** Unported vs International, same deed.

## Source table

| Key | URL | Resolves y/n | Licence correct y/n |
|---|---|---|---|
| T1 Tobin 1983 | PMID 6872603; third-party PDF used by H3 | y (PDF + abstract) | y (closed; facts cited; third-party copy noted) |
| T2 Parreira 2010 | https://www.scielo.br/j/rbfis/a/gsyXpLD5C8LPktT8bnBkcKG/?lang=en | y | **n in H3** (left unverified). **Correction: CC BY-NC 4.0 International** |
| T3 Shaghayeghfard 2024 | PMC10862111 | y (Europe PMC XML) | y (CC BY-NC 4.0); naming Unported vs International |
| T4 De Groote 1997 | PMID 9375316 | y (abstract) | y (© APS; abstract facts) |
| T5 Verschakelen 1995 | PMID 7842198 | y (abstract) | y (closed; abstract facts) |
| T6 Santos & Duarte 2016 | PeerJ 4:e2648 | y (PMC) | y (CC BY 4.0) |
| T6-data HBEDB | https://physionet.org/content/hbedb/1.0.0/ | y | y (ODC-By v1.0) |
| T7 Błaszczyk 2023 | PMC10170532 | y | y (CC BY 4.0) |
| T8 Duarte & Freitas 2010 | SciELO PDF | y (file was fetched earlier) | H3 “licence not verified” — not settled again here |
| T9–T14 closed abstracts | PubMed PMIDs as in H3 | y for landing/abstract where stored; several numbers not re-quoted | y (closed; abstract facts) |

