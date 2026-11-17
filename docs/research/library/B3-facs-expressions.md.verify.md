# B3 verification: FACS action units, prototypes, magnitudes, timing

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `B3-facs-expressions.md`. B3 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/B3-facs-expressions.md`
- Size: 18,530 bytes, 234 lines
- File mtime: **2026-11-16 10:03:20 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 10:29 AEDT**. Bit-for-bit identical to a start-of-check snapshot.
- Grok CLI: tried first at the start of this B2–B5 pass (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`). It exited at once with **“Not signed in”**. **No login was attempted.** Checks used curl, WebSearch, and local copies under `/tmp/b3verify/`. Access date: **2026-11-16**.

## Summary

Unit of counting: **one data row**. Multi-number rows were checked number-by-number, including population, sex, age and n. Licence and FACS-wording findings are in the source table and in “Most serious problems,” not all in the row counts.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **76** | The cited source opens (or an open copy does), and it states that number, units, and (where claimed) population/sex/age/n. |
| **FIX** | **2** | A stated figure or population label is wrong or more specific than the source. |
| **UNVERIFIABLE** | **0** | The Du, Tao & Martinez 2014 basic-six AU list is correctly left unused, so it is not counted. |
| **Total data rows** | **78** | Table 1: 30; Table 2a: 6; Table 2b: 7; Table 3: 17; Table 4: 11; Table 5: 7. |

**Most serious problems**

1. **FIX — da Cruz & Velasco e Cruz 2004, brow-raise range “7.57 to 9.02 mm.”** That range is the **abstract** (Portuguese and English). Table 1 in the Results gives group means **7.57, 8.70 and 9.20 mm** (children / adults / elderly), and the Discussion says “em média de 7,5 a 9,2.” **Correction: group means 7.57 to 9.20 mm; flag the abstract’s 9.02 as inconsistent with the table.** F = 1.73, p = 0.18, n = 20 / 33 / 28, ages 6–12 / 20–38 / 60–80, right-eye 2D photos scaled to a 12 mm corneal diameter, landmark halfway between the canthi — all of that checks out. “Brazilian” is inferred from the Ribeirão Preto clinic, not a printed nationality.
2. **FIX — Kwon et al. 2023 population written “Korean adults, n = 30.”** The paper says **“Thirty healthy participants aged 30–62 years (median age: 40.5 years) … male-to-female ratio was 1:6.5.”** It never calls the sample Korean. Authors and clinic are in Korea; that is not the same as a stated ancestry/nationality. The displacement figures (59.4 mm, 5.6 mm, 63.4 / 33.3 / 3.3%, 86.7%) are right.
3. **Licences are correctly flagged, and none is wrongly marked as free.** Du & Martinez 2015 is **CC BY-NC-ND 3.0**; Schutte 2023 is **CC BY-NC-ND 4.0**; da Cruz 2004 is **CC BY-NC 4.0** on the current ABO/journal site; Kwon 2023 is **CC BY-NC 4.0**. Clark 2020 is CC BY 4.0. Schmidt and Yan are © Springer, facts cited (Schmidt via the NIH author manuscript). The FACS manual was not used.
4. **FACS wording:** Table 1’s “Main visible movement” column is **not** in Clark et al. 2020 (that table is only AU number, name, and muscle). The descriptions are short anatomical paraphrases in the author’s own words, not scoring-criteria language from the copyrighted FACS manual. That matches B3’s own claim. AU names and muscles match Clark Table 1; B3 correctly flags the source typo “Upper lid raiser” for AU10 and silently corrects “Masetter” → Masseter (noted “as listed”) and “Buccinnator” / “supercilli” spelling.

**What is in good shape.** All 30 Clark AU name/muscle rows; all six Investigator’s Guide emotion variants in Clark Table 3 (PDF pp. 15–16; B3’s “p. 16” is fair); all seven CK+ Table 2 criteria (PDF p. 3), including the Fear AU5-intensity-E exception, the Sadness AU6+15 exception, the Surprise AU5 ≤ B cap, and Contempt uni-/bilateral AU14. B3 correctly flags the CK+ Figure 1 caption “Disgust – AU 1+4+15+17” (PDF p. 4, not p. 3) as contradicting Table 2. All 17 Du & Martinez 2015 Table I AU percentages match, including the caption that only the first 15 categories used 230 images (last two: 20 faces). All Schutte 2023 smile and pout millimetre values and 95% CIs match Tables 2 and 5 (Dutch sample, n and mean ages as in Table 1). Schmidt 2009 Tables 1–2 timings and the secondary 0.52 / 0.50, 0.59, 0.67 / <0.75 s (Hager & Ekman 1997) citations check out. Yan 2013 Table 4 numbers check out (printed p. **224**, not 225 — 225 is Table 5). The unused Du 2014 prototypes are honestly marked unconfirmed. Sources disagree on anger/sadness/surprise recipes is fair.

## Row table

Access date 2026-11-16 throughout.

### Table 1 — Clark et al. 2020, Table 1 (PDF p. 3), CC BY 4.0

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| AU1–AU46 (30 rows: 1, 2, 4–18, 20–28, 41–46) | Names and muscles as listed | VERIFIED | Matches Clark Table 1, including AU8 “Lips toward each other,” AU25’s “or relaxation of…,” AU26 “relaxed temporalis and internal pterygoid,” AU46 described as “as AU 45.” AU19 Tongue Show is in the source and correctly omitted (not a commonly used expression AU here). Footnote: “The coded numbers are arbitrary…” | [Clark PDF](https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2020.00920/pdf) (200); DOI 200. CC BY (Attribution License) on the PDF. |

### Table 2 — Basic-expression AU recipes

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 2a Anger, Disgust, Fear, Happy, Sadness, Surprise (6) | Investigator’s Guide variants as tabulated by Clark Table 3 | VERIFIED | Every combination matches, including 4+5/7 and 17+24 for anger, 9/10, 20 alone for fear, 12 and 6+12, sadness 1 alone, surprise 5+26/27. “A/B means either A or B” is Clark’s caption. Table sits on PDF pp. 15–16. | same Clark PDF |
| 2b Angry … Contempt (7) | CK+ Table 2 criteria, paraphrased | VERIFIED | Exact: Angry AU23 and 24 both present; Disgust AU9 or 10; Fear AU1+2+4 unless AU5 is intensity E then AU4 may be absent; Happy AU12; Sadness AU1+4+15 or 11, exception AU6+15; Surprise AU1+2 or 5, AU5 no stronger than B; Contempt AU14 uni- or bilateral. | [CK+ PDF](https://www.jeffcohn.net/wp-content/uploads/2020/02/CVPR2010_CK2.pdf.pdf) (200), Table 2 on PDF p. 3 |

### Table 3 — Du & Martinez 2015, Table I, CC BY-NC-ND 3.0

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| C1–C17 | All 17 compound AU percentages, e.g. happily surprised 1 (95%), 12 (100%), 25 (100%) through happily sad 4 (85%), 6 (75%), 12 (100%), 25 (75%) | VERIFIED | Every percentage matches Table I. Caption: first 15 categories from 230 images each; last two (happily fearful, happily sad) from 20 faces — B3 states the 230-image part correctly. | [DOI](https://doi.org/10.31887/DCNS.2015.17.4/sdu) (T&F 403); [PMC4734882](https://pmc.ncbi.nlm.nih.gov/articles/PMC4734882/) (200); Europe PMC XML (200). Licence CC BY-NC-ND 3.0 confirmed in the XML. |

### Table 4 — How far landmarks move (mm)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| S1 | Closed smile, cheilion, Dutch women >16, n=78, mean age 40.9: 9.27 mm (8.54–9.99) | VERIFIED | Schutte Table 2. Sample is Dutch (discussion: “Dutch inhabitants”); n and mean age from Table 1. | [DOI](https://doi.org/10.1097/GOX.0000000000004762); [PMC9911205](https://pmc.ncbi.nlm.nih.gov/articles/PMC9911205/); Europe PMC XML (200). **CC BY-NC-ND 4.0** confirmed. |
| S2 | Men >16, n=63, mean 45.3: 9.42 mm (8.70–10.14); X 4.90, Y 5.72, Z 3.88 | VERIFIED | Same table, male >16 column. Axis-sign caveat in B3 is fair. | same |
| S3 | Children 8–12, F n=60 / M n=58: 10.66 / 11.82 mm | VERIFIED | | same |
| S4 | Alare, women / men >16: 2.91 / 3.14 mm | VERIFIED | | same |
| S5 | Crista philtri, women / men >16: 3.91 / 3.93 mm | VERIFIED | | same |
| S6 | Pout, cheilion, women / men >16: 16.63 (15.77–17.48) / 17.78 (16.65–18.92) mm | VERIFIED | Schutte Table 5. | same |
| S7 | Pout, crista philtri: 9.64 / 11.40 mm | VERIFIED | | same |
| S8 | Pout, alare: 2.20 / 3.01 mm | VERIFIED | | same |
| D1 | Maximal brow raise, mid-fissure, right eye, 2D scaled by 12 mm cornea; n=20 / 33 / 28 (ages 6–12 / 20–38 / 60–80); group means **7.57 to 9.02 mm**; F=1.73, p=0.18 | **FIX** | Population, method, ANOVA and “no significant age difference” are right. **Correction: Table 1 means are 7.57, 8.70 and 9.20 mm. Use 7.57–9.20 and note the abstract’s 9.02.** Journal *Arq Bras Oftalmol* 2004;67:237–40. | [DOI](https://doi.org/10.1590/S0004-27492004000200010); SciELO PDF (200); [ABO page](https://www.aboonline.org.br/details/590/en-US/quantification-of-frontalis-muscle-action) (200). Site footer: **CC BY-NC 4.0**. |
| K1 | Brow raise, transition line 59.4 mm above pupil in the 40% with bidirectional movement; “average width of eyebrow displacement” 5.6 mm (range 1–11.2); **Korean adults, n=30** | **FIX** | The millimetre figures and 40.0% bidirectional share are right (Results). **Correction: n=30 healthy participants aged 30–62 (median 40.5), M:F 1:6.5. Do not label them “Korean” unless the paper does.** Pages 440–447 confirmed. | [DOI](https://doi.org/10.3349/ymj.2022.0605); Europe PMC XML / [PMC10307676](https://pmc.ncbi.nlm.nih.gov/articles/PMC10307676/). **CC BY-NC 4.0** confirmed. |
| K2 | Direction: vertical 63.4%, lateral-oblique 33.3%, medial-oblique 3.3%; asymmetric 86.7% | VERIFIED | Tables 1–2 (19/30, 10/30, 1/30; 86.7% from Results). | same |

### Table 5 — Timing

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| T1 | Smile onset, 57 adults (16 M, 41 F), Pittsburgh: deliberate 0.544 s (SD 0.174); spontaneous 0.600 s (SD 0.245); n.s. | VERIFIED | Schmidt 2009 Table 1 (duration F=2.65, no asterisk). | [PMC2843933](https://pmc.ncbi.nlm.nih.gov/articles/PMC2843933/) (NCBI efetch XML 200). © Springer; manuscript “available for text mining / fair use.” |
| T2 | Smile max speed 0.023 vs 0.016 (p<0.01); amplitude 0.143 vs 0.116 | VERIFIED | Speed F=12.95**; amplitude F=2.29 (not starred). B3 does not claim the amplitude difference is significant. | same, Table 1 |
| T3 | Eyebrow-raise onset: deliberate 0.489 s (0.178); spontaneous 0.738 s (0.543); p<0.01; n=57, “spontaneous n=48/42 tracked” | VERIFIED | Table 2 values and p (F=12.71**) match. Text: coded spontaneous brow movement in 48 people; both deliberate and spontaneous tracked in n=42. Table 2’s own header still says n=57; B3’s caveat is the more careful reading. | same, Table 2 |
| T4 | Eyebrow-raise amplitude 0.437 vs 0.314 (deliberate larger) | VERIFIED | Table 2, F=25.61**. | same |
| T5 | Secondary: 0.52 s solitary; 0.50, 0.59, 0.67 s social; upper bound 0.75 s (Hager & Ekman 1997) | VERIFIED | Introduction and Discussion, as cited (social figures are called “average durations” in a paragraph about onset). “Under .75 seconds” is in the Discussion. | same |
| Y1 | Micro-expression total duration, 109 expressions <500 ms: M 313.91 ms (SD 85.81); onset 139.91 (SD 50.05) | VERIFIED | Yan 2013 Table 4, first row. Printed page is **224** (B3 says p. 225; 225 is Table 5). Participants neutralized their faces while watching emotional video episodes (authors at Chinese institutions). | [institutional PDF](https://ir.psych.ac.cn/bitstream/311026/10811/1/WOS_000325968300002.pdf) (200). © Springer Science+Business Media New York 2013. |
| Y2 | 142 expressions with onset <260 ms: onset M 147.30 (SD 49.01); fitted limits 65.97 / 261.93 ms; definition total <500 ms or onset <260 ms | VERIFIED | Table 4 third row and Results/abstract. | same |

## Source table

| Source | Licence as stated in B3 | Licence check | URL status (2026-11-16) | Notes |
|---|---|---|---|---|
| Clark et al. 2020, *Front Psychol* 11:920 | CC BY 4.0 | **Confirmed** on the PDF (“Attribution License (CC BY)”). | PDF 200; DOI 200 | Table 1 p. 3, Table 3 pp. 15–16. |
| Lucey et al. 2010, CK+ | © IEEE; author-hosted PDF; criteria paraphrased | Fair for a CVPR Workshops paper on an author site. | Author PDF 200 | Table 2 p. 3; Figure 1 caption typo is on p. 4. |
| Du & Martinez 2015, *Dialogues Clin Neurosci* 17(4):443 | **CC BY-NC-ND 3.0**; facts cited; no derivative of the text | **Confirmed** in article XML (creativecommons.org/licenses/by-nc-nd/3.0/). | PMC 200; XML 200; T&F DOI 403 | All Table I figures checked. |
| Du, Tao & Martinez 2014, *PNAS* | Not accessed; unconfirmed | Honest; no numbers used from it. | — | — |
| Schutte et al. 2023, *PRS Glob Open* 11:e4762 | **CC BY-NC-ND 4.0 (non-commercial)**; facts cited | **Confirmed** (CCBY-NC-ND 4.0; cannot remix). | XML 200; journal DOI 403 | Tables 1, 2, 5 checked. |
| da Cruz 2004, *Arq Bras Oftalmol* 67(2) | Journal site states **CC BY-NC 4.0** | **Confirmed** on the current ABO site footer (by-nc/4.0). A 2004 PDF itself has no licence line; B3 correctly attributes the statement to the journal site. | SciELO PDF 200; ABO 200; DOI landing flaky (500 to one SciELO URL) | Abstract vs Table 1 inconsistency: see FIX D1. |
| Kwon et al. 2023, *Yonsei Med J* 64(7):440–447 | **CC BY-NC 4.0** | **Confirmed** in XML (creativecommons.org/licenses/by-nc/4.0). Pages 440–447 confirmed. | XML 200; DOI 302 to eymj.org | Population label: see FIX K1. |
| Schmidt et al. 2009, *J Nonverbal Behav* 33:35–45 | © Springer; NIH author manuscript in PMC (fair-use / text-mining) | **Confirmed** (PMC licence-p: text mining / fair use). | NCBI efetch XML 200; PMC HTML often blocked to curl | Tables 1–2 and intro/discussion citations checked. |
| Yan et al. 2013, *J Nonverbal Behav* 37:217–230 | © Springer; institutional copy; facts cited | **Confirmed** on the PDF (“Springer Science+Business Media New York 2013”). | ir.psych.ac.cn PDF 200 | Table 4 is printed p. 224. |
| FACS manual (Ekman, Friesen & Hager 2002) | Copyrighted; not used | B3’s practice matches this: names/muscles via Clark; movement glosses are the author’s. | — | No manual text was fetched or compared here. |

## Notes that are not FIXes

- Yan Table 4 is on printed p. 224, not 225.
- CK+ “Disgust – AU 1+4+15+17” caption is on PDF p. 4.
- Kwon participants’ ages (30–62, median 40.5) and sex ratio (1:6.5) are not in B3’s table cell; only the “Korean” label is over-specific.
- No number in this check came from the grok CLI, and no number was invented.
