# B5 verification: blinks, eye–head coordination, pupil size, eyelid position

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `B5-eyes.md`. B5 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/B5-eyes.md`
- Size: 21,318 bytes, 242 lines
- File mtime: **2026-11-16 10:12:20 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 10:37 AEDT**. Bit-for-bit identical to a start-of-check snapshot.
- Grok CLI: tried first at the start of this B2–B5 pass (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`). It exited at once with **“Not signed in”**. **No login was attempted.** Checks used curl, WebFetch (for the JOV page, which returns 403 to curl), local copies under `/tmp/b5verify/`, and the already-verified Ostermann / Kwon 2021 sources. Access date: **2026-11-16**.

## Summary

Unit of counting: **one data row** (or one separately stated methods claim in a table preamble). Multi-number rows were checked number-by-number, including population, sex, age and n where claimed. Licence findings are in the source table and in “Most serious problems,” not all in the row counts.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **54** | The cited source opens (or an open copy does), and it states that number, units, and (where claimed) population/sex/age/n. |
| **FIX** | **2** | A stated methods fact is wrong. |
| **UNVERIFIABLE** | **0** | Items left unused or marked second-hand / not transcribed (Watson Eq. 21, Winn 1994 primary, Zangemeister primary papers, spontaneous-blink duration SD, etc.) are not counted. |
| **Total data rows** | **56** | Table 1: 6 + 1 methods-duration claim; Table 2: 12 + 1 “5.8×” arithmetic + 1 frame-rate claim; Table 3a: 4; Table 3b: 7; Table 4: 9; Table 5: 6 + 4 age-group rows + 4 correlations + 1 Kwon 2021 cross-ref. |

**Most serious problems**

1. **FIX — Kwon et al. 2013 frame rate written “1000 frames/s.”** The paper records blinks **“at 600 frames per second”** (abstract, Methods, Discussion, and Table 1: “600 fps sampling rate”). The “1000” on the figures is a time-axis tick (ms), not the camera rate. **Correction: 600 frames/s.** The kinematics themselves (mean ± SE) match: 572 ± 25, 76 ± 2, 58 ± 4, 438 ± 24, 273 ± 23, 299 ± 8 ms; peak 243 ± 9 / 157 ± 5 mm/s; average 134 ± 4 / 26 ± 2 mm/s; aperture 9.8 ± 0.2 mm (range 7.4–12.8; Table 1 prints 9.83 ± 0.17). Population is 25 healthy volunteers, **11 male and 14 female**, aged 25–63 (text “mean of 35 years”; Table 1 **34.7 ± 1.1**), and **“Volunteers of oriental origin were excluded.”** B5’s 34.7 is the table figure; sex and the exclusion were omitted, not contradicted. 438/76 ≈ 5.76, written “about 5.8×,” is correct half-up arithmetic. Fox (7–13 mm; 61.7% **of subjects** 9–10 mm) and Choi 2003 (n = 36, 125 ± 29 / 205 ± 32 ms) match Kwon’s Table 1 / text (B5’s “61.7% of eyes” is a wording slip).
2. **FIX — Bentivoglio et al. 1997 “Videotaped for 5 min per condition.”** Methods (printed p. 1029): **“Each videotape segment lasted for 2 min 30 s.”** Two observers then **“measured the number of blinks for 2 min during each segment. The first 20 s of recording was not analyzed.”** **Correction: 2 min 30 s recorded, 2 min counted, first 20 s dropped.** All Table 1 rates, critical values, sex-split means, −55.08% / +99.70%, and the Discussion’s “74%” / “100%” match the rendered p. 1031 table and text (checked on the page image, because OCR of this scan is unreliable). Population 150 (70 M, 80 F), 35.9 ± 17.9 y (range 5–87), Rome / Catholic University, first 20 s, two-sided 5% and 10% critical values (not SD), women significant only while reading, no age correlation, 134/150 (89.3%), and 101/150 (67.3%) “conversation > rest > reading” all check out. “Italy” is inferred from the enrolment site; ethnicity is printed as 145 non-Jewish Caucasian and 5 Ashkenazi Jewish. B5 already flags the unreconciled −55.08% vs “decreased by 74%.”

**What is in good shape.** Fang et al. 2015 slopes 0.23 / 0.11 and 0.44 / 0.30, the ±30° / ±12° 90% envelope, and t(6) = 3.25, p < 0.05 all match the Results / Fig. 6 text. Analysed n is **7** after **one of eight enrolled subjects (1 female, 7 male, aged 22–31) was discarded** for calibration errors — B5’s “7 subjects” is the analysed n; age/sex and the exclusion are omitted. “Head-mounted eye tracker plus motion capture” is a fair shorthand for an EMR-9 (NAC) eye tracker plus a Polhemus FASTRAK electromagnetic tracker. Freedman 2008: 25–40 ms eye lead (Fig. 4 after Bizzi; also Zangemeister & Stark 1982), ~250 ms and 15° more head travel, ~2° head contribution of a ~30°+ gaze shift with 17° total head travel, ~20 ms neck-EMG lead, head-can-lead on predictable/large shifts, human duration–amplitude slope 1.5–3 ms/deg (the 5–45° window is the review’s example axis), monkey peak-velocity saturation ~900–1000 °/s, and “3–4 per second” saccade rate all match; B5 honestly marks these as review / secondary and flags the cat-timing disagreement (Blakemore ~30 ms eye first vs Guitton ~40 ms head first). Watson & Yellott 2012: “approximately 2 and 8 mm,” all five Winn Table 1 intercepts/slopes/r², the worked examples (7.19 / 5.04 / 3.77 / 3.02 mm), and “about 1 and 0.6 mm… smallest at the largest luminances” (Fig. 20, from Winn Fig. 2) match; Winn’s 91 subjects, ages 17–83, monocular, 10° field are correctly described as secondary via Watson; Eq. 21 is honestly not transcribed. Rana et al. 2023 Tables 1, 3 and 4 match throughout (380 participants, 215 F / 165 M, 57.5 ± 18 y, 760 eyes, 83 / 7 / 7 / 2% ancestry, Royal Adelaide Hospital, calibrated photographs); the “1–2 mm inferior to the superior limbus” sentence is an uncited introductory statement, as B5 says. Kwon SH 2021 fissure heights 9.16 ± 1.18, 8.00 ± 1.31, 6.61 ± 1.24 mm and os–ps 13.42, 14.07, 15.42 mm match Table 1 (already checked for B2). MPEG-4 “eyelids tangent to the iris” and pupil = 1/3 of iris diameter match Ostermann. Sources disagree and Not found sections are honest.

**Licences.** None of the used sources is wrongly marked as free. Bentivoglio is © 1997 Movement Disorder Society (author-hosted PDF). Kwon KA 2013 is **CC BY 3.0**. Fang 2015 is **CC BY 4.0**. Freedman 2008 is © Springer, NIH author manuscript, facts only. Watson & Yellott: JOV/ARVO; the fetched page has no visible CC licence (Europe PMC lists the article as not open access), so B5’s “licence not checked” is the right stance. Winn 1994 was not opened, as stated. Rana 2023 is **CC BY 4.0** (the XML also carries “© Crown 2023,” which is compatible with that licence). Kwon SH 2021 is **CC BY-NC 4.0**. Ostermann © ISCA/authors, facts cited.

## Row table

Access date 2026-11-16 throughout.

### Table 1 — Bentivoglio et al. 1997, Table 1, printed p. 1031 (PDF p. 4)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| Preamble: 150 (70 M, 80 F), 35.9 ± 17.9 y (5–87), Italy; 5 min per condition; first 20 s dropped; mean and 5%/10% critical values, not SD | Population and methods | FIX | Population, critical-value method, and first 20 s match. **Correction: each segment lasted 2 min 30 s; blinks were counted for 2 min.** “Italy” is the enrolment site (Catholic University, Rome); ethnicity is printed. | [author-hosted PDF](https://albertoalbanese.it/wp-content/uploads/2022/06/Bentivoglio-1997-Mov.-Disord.pdf) (200). Wiley DOI 403 to curl. © 1997 Movement Disorder Society on p. 1028. |
| Rest, Reading, Conversation (3) | All / men / women means and ranges as tabulated | VERIFIED | Checked against the rendered Table 1 image: 17 (4–48, 6–40) / 15.6 / 18; 4.5 (0.7–22, 1–16) / 3.0 (0–13, 0–10) / 6.2 (0–26, 0–20); 26 (11–53, 13–47) / 24 / 26.7. OCR of this scan is not trustworthy. | same, printed p. 1031 |
| Change vs rest, reading / conversation (2) | −55.08% and +99.70% (abstract p. 1028, Results p. 1030); Discussion “74%” / “100%” (p. 1031) | VERIFIED | Exact. B5 already flags the unreconciled 74%. (111/150 = 74% is used in the same Results paragraph for a different comparison, conversation > rest.) | same |
| Earlier adult norm 19 ± 14 (SD) | p. 1028, citing Karson 1983 (ref 2), secondary | VERIFIED | “According to Karson (2), the normal rate in adults is 19 ± 14 (SD)/min.” Correctly marked secondary. | same, p. 1028 |

Other findings not given their own count rows (women significant only while reading; no age correlation; 134/150 = 89.3%; 101/150 = 67.3% conversation > rest > reading) all match pp. 1030–1031.

### Table 2 — Kwon et al. 2013 voluntary-blink kinematics (mean ± SE)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| Preamble: 25 volunteers, 25–63 (mean 34.7), 1000 frames/s, voluntary, mean ± SE | Population and methods | FIX | n, age, voluntary-only, and SE (not SD) are right. **Correction: 600 frames/s.** Also 11 M / 14 F; oriental origin excluded. | [UCL PDF](https://discovery.ucl.ac.uk/id/eprint/1396599/1/J._R._Soc._Interface-2013-Kwon-.pdf) (200). PMC XML is metadata-only. CC BY 3.0 on the PDF. |
| 12 kinematic / comparison rows | Durations, speeds, aperture, Fox 1966, Choi 2003 n = 36 | VERIFIED | All 10 Kwon values match text + Table 1. Aperture text is 9.8 ± 0.2 (range 7.4–12.8); table 9.83 ± 0.17. Fox and Choi match as secondary citations in this paper. PDF page “Table 1 on p. 6” is fair (table starts at the foot of p. 5, data on p. 6 of the 7-page UCL copy). | same |
| Opening ≈ 5.8× closing (438/76; my arithmetic) | 5.8× | VERIFIED | 438/76 = 5.763…; half-up to 1 decimal is 5.8. | same |

### Table 3 — Eye–head coordination

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 3a four rows (slopes 0.23/0.11 and 0.44/0.30; ±30°/±12°; t(6) = 3.25, p < 0.05) | Fang 2015, Fig. 6C–D text; 7 subjects, head and body free, natural search | VERIFIED | Numbers exact. Analysed n = 7 after 1 of 8 enrolled (1 F, 7 M, 22–31 y) was discarded; task and “head and body freely” match. Tracker: EMR-9 + Polhemus FASTRAK (B5’s “motion capture” is loose but not false). | [PLOS](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0121035) (200); PMC4370616 XML (200). **CC BY 4.0.** |
| 3b seven rows | Freedman 2008 review figures and secondary citations as written | VERIFIED | 25–40 ms, 250 ms / 15°, ~2° / 17° / ~30°, ~20 ms EMG, head-can-lead, 1.5–3 ms/deg, ~900–1000 °/s (rhesus), 3–4/s all in the NIH manuscript. Species and “secondary” labels are honest. The 5–45° window is the review’s example axis, paired with the human 1.5–3 ms/deg range. | [PMC2605952 efetch XML](https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=pmc&id=2605952) (200). © Springer, facts cited. |

### Table 4 — Pupil diameter (Watson & Yellott 2012)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| ~2 to 8 mm | Introduction, para 1 | VERIFIED | “pupil diameter ranges between approximately 2 and 8 mm.” | [JOV](https://jov.arvojournals.org/article.aspx?articleid=2279420) — 403 to curl; opened with WebFetch, 2026-11-16. |
| Five Winn lines (9–4400 cd/m²) | D = intercept − slope·age, r² as written; 91 subjects 17–83, monocular, 10° field, secondary via Table 1 | VERIFIED | Table 1 matches (source prints −0.04, −0.02, 4.07; B5 writes −0.040, −0.020, 4.070). Winn population as described in Watson’s Winn section. Winn 1994 itself was not opened, as B5 says (the iovs.org URL still fails). | same |
| Worked examples (my arithmetic) | 7.19, 5.04, 3.77, 3.02 mm | VERIFIED | Recalculated from the written coefficients. | — |
| Between-person SD about 0.6–1 mm, smallest at high luminance | Watson from Winn Fig. 2; Fig. 20 | VERIFIED | “The values range between about 1 and 0.6 mm, and are smallest at the largest luminances.” | same JOV page |
| MPEG-4 pupil = 1/3 of iris diameter | Ostermann 1998 §2, convention | VERIFIED | Matches the already-checked Ostermann definition. | [ISCA PDF](https://www.isca-archive.org/icslp_1998/ostermann98_icslp.pdf) (200) |

### Table 5 — Rana et al. 2023 eyelid / fissure measures

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| Preamble: 380 (215 F / 165 M), 57.5 ± 18 y, Royal Adelaide Hospital, 83/7/7/2%, 760 eyes, calibrated 2D photos | Population | VERIFIED | Methods/Results exact, including 165 male (43%). “Patients presenting to the Royal Adelaide Hospital.” | [Europe PMC XML](https://www.ebi.ac.uk/europepmc/webservices/rest/PMC10371930/fullTextXML) (200); PMC10371930. **CC BY 4.0** (also “© Crown 2023”). |
| Table 1 six measures (MRD1, MRD2, PFH, HPA, IPD, 1–2 mm lid rest) | All / male / female means ± SD and p = 0.80 / p < 0.01 | VERIFIED | Every cell matches. “1–2 mm inferior to the superior limbus” is an uncited introductory statement, as written. | same |
| Table 3 four measures × four age groups (n = 72 / 117 / 163 / 28) | Means ± SD as tabulated | VERIFIED | Every cell matches, including MRD2 80–99 = 5.09 ± 1.1. | same |
| Table 4 correlations (4) | r = −0.09, −0.21, −0.21, −0.39 | VERIFIED | Exact (MRD1, MRD2, PFH, HPA). | same |
| Kwon SH 2021 Korean women fissure and os–ps (cross-ref) | 9.16 ± 1.18, 8.00 ± 1.31, 6.61 ± 1.24 mm; 13.42, 14.07, 15.42 mm at 20–39 / 40–59 / 60–79 | VERIFIED | Matches Kwon 2021 Table 1 (PMC7875215). **CC BY-NC 4.0.** | [PMC7875215](https://pmc.ncbi.nlm.nih.gov/articles/PMC7875215/) (200) |

## Source table

| Source | URL tried | HTTP / copy used | Licence as checked 2026-11-16 | Honest in B5? |
|---|---|---|---|---|
| Bentivoglio et al. 1997 | author-hosted PDF; Wiley DOI | PDF 200; DOI 403 | © 1997 Movement Disorder Society; facts cited | Yes |
| Kwon KA et al. 2013 | UCL PDF; PMC 4043155; Royal Society DOI | UCL PDF 200; PMC XML metadata-only; DOI historically 500 | **CC BY 3.0** on the PDF | Yes |
| Fang et al. 2015 | PLOS; PMC4370616 | both 200 | **CC BY 4.0** | Yes |
| Freedman 2008 | PMC2605952 efetch; Springer DOI | XML 200; DOI historically 500 | © Springer; NIH author manuscript; facts only | Yes |
| Watson & Yellott 2012 | JOV article; DOI 10.1167/12.10.12 | curl 403; **WebFetch succeeded** | No CC licence visible; Europe PMC: not open access. © ARVO is the conservative reading. B5: licence not checked. | Yes |
| Winn et al. 1994 | iovs.org/content/35/3/1132 (URL as given in Watson) | 500 / not opened | Not read; secondary via Watson Table 1 | Yes |
| Rana et al. 2023 | Springer DOI; PMC10371930; Europe PMC XML | XML 200 | **CC BY 4.0** (© Crown 2023 also on the XML) | Yes |
| Kwon SH et al. 2021 | PMC7875215; DOI 10.5021/ad.2021.33.1.52 | PMC 200 | **CC BY-NC 4.0** | Yes |
| Ostermann et al. 1998 | ISCA PDF | 200 (already held from B4) | © ISCA/authors; convention cited as fact | Yes |

No used source is wrongly marked as free.
