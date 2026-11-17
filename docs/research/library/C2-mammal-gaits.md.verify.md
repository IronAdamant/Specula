# C2 verification: gaits of quadruped mammals

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `C2-mammal-gaits.md`. C2 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/C2-mammal-gaits.md`
- Size: 39,836 bytes, 295 lines
- File mtime: **2026-11-16 10:03:43 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 10:20 AEDT**. The file was unchanged while this check ran.
- Grok CLI: tried first (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`). It exited at once with **“Not signed in”** (`auth.json` absent; no `XAI_API_KEY`). **No login was attempted.** All checks used WebSearch, WebFetch, curl, NCBI E-utilities, OpenAlex, and independent re-extraction of PDFs/HTML already readable outside any repo (`/workspace/c2work/` for some earlier copies; fresh WebFetch of JEB HTML, HAL PDF, J-STAGE PDF, PMC/Europe PMC, PubMed abstracts). Access date for live URLs: **2026-11-16**.
- Secondhand rows (Alexander & Jayes 1983; Heglund, Taylor & McMahon 1974 equations; Heglund & Taylor 1988 coefficient table): the originals were sought (Wiley, Science, JEB archives, OpenAlex, Europe PMC, Unpaywall, institutional copies). **Full text of A&J 1983, Heglund 1974, Hoyt & Taylor 1981, and the Heglund & Taylor 1988 tables was not obtained.** Heglund & Taylor 1988 and Heglund 1974 abstracts were read. Coefficients quoted from Hutchinson 2006 Table 6 were checked against that table and, where possible, against Alexander 1993 and Iriarte-Díaz 2006.

## Summary

Unit of counting: **one data row** (a table row, or a clearly bounded bullet/claim). Multi-number rows were checked number-by-number; a single wrong figure makes the whole row FIX. Derived (D) rows were recomputed from the checked inputs. The source list is not in the row counts; it is in the source table.

| Status | Rows | What it means |
|---|---:|---|
| VERIFIED | 104 | Number, units, species/population, and (where given) spread match the cited source, or a stated derivation recomputes correctly. Multi-number table blocks (e.g. Robilliard timing ratios, Hutchinson observed ranges) were counted as one row when every figure in the block checked out. |
| FIX | 2 | A wrong or misleading figure or wording that should be corrected from a source I opened. |
| UNVERIFIABLE | 4 | Could not confirm in a primary source: the Heglund & Taylor 1988 coefficient row C2 already marked **U**; the two Hildebrand 1959 rows C2 already marked **U**; and Hoyt & Taylor 1981, for which C2 correctly gave no number. |

Of the 104 VERIFIED rows, **15** are quotations of Alexander & Jayes 1983 or Heglund 1974 / Heglund & Taylor 1988 coefficients (or of Hildebrand / Benninger figures cited inside a paper I did open). Those 14 match the secondary source I opened (mostly Hutchinson 2006 Table 6, also Alexander 1993, Iriarte-Díaz 2006, Schilling, Belyaev). **The originals of A&J 1983, Heglund 1974, Heglund & Taylor 1988 tables, Hildebrand 1959, Alexander 1980, and Benninger 2004 / Pylypchuk 1975 / Wachs 2016 were not opened.** I do not upgrade those 15 to primary-source V. If a later pass opens the originals, treat them as unchecked against those originals.

**Worst problems**

1. **Schilling & Hackert 2006 speeds are wrong as stated.** C2 says the five small mammals were filmed at “about 1.15–1.18 m/s”. The paper gives species-specific means: kowari **0.92±0.06**, grey short-tailed opossum **1.15±0.19**, tree shrew **1.48±0.15**, pika **1.18±0.05**, cui **1.14±0.12** m/s. The quoted band omits the kowari and the tree shrew. The joint-angle figures on the same rows check out.
2. **“Sources disagree” item 2 mis-states the Alexander 1993 length.** Griffin 2004 does use leg length *L*. Alexander 1993 Eq. 2 and the surrounding text also use **leg length L** in Fr = v²/gL, not hip height. Hip height is the usual A&J 1983 convention; it should not be attributed to the 1993 paper.
3. **Alexander 1993 prints the trot–gallop Froude number as 2.53**, with a citation (A&J 1983), and also “between 0.5 and 2.5” then “above 2.53”. C2’s “≈2.5” is a rounding, not a hard error; the exact printed figure should be kept. Not counted as a FIX in the table (see the note on row T-AJ-g).
4. **Maes 2008 Table 2 vs results text.** Fast rotary gallop HL is **–15±2** in Table 2 and **–15±1** in the results sentence. C2 used the table, which is the right choice; this is a source inconsistency, not a C2 error.

**Dead or blocked links (from this box, 2026-11-16)**

- Almost all `journals.biologists.com` PDF and DOI URLs return **403** to curl (Cloudflare). The same articles opened by WebFetch as HTML, and PubMed abstracts returned **203**. This matches what C2 reported. The papers exist.
- The RVC repository PDF for Hutchinson 2006 returned **403**.
- Original Wisdom pages (S24) returned **406** to curl here; C2 reported 200/WebFetch OK on the same day. Not treated as dead.
- No 404s were found on the listed source URLs.
- J-STAGE (S10), HAL (S17), Frontiers/PMC (S15), AVMA DOIs (S12, S13, S19), Renous PDF (S20), Nature abstract (S22), Wikimedia and Digital Commonwealth (S26) returned **200**.

**What holds up.** Footfall conversion to LH = 0 is stated clearly and is applied consistently for Maes (APS: f1/h1 same side; for gallop, f1 = first fore) and for Hutchinson (the paper itself uses LH = 0). Recomputed Maes phases and duty-factor examples, Robilliard duty factors (stance ÷ stride time) and stride frequencies, Biancardi phase arithmetic, Hutchinson û = 1 examples, and the Kamimura radian-to-degree conversion all hold. Froude definitions are given, and the length used (withers, hip, or leg) is named on the rows that needed it, except for the Alexander 1993 slip in the disagreement list. “Not found” and “Sources disagree” are present and, with the one length-definition slip, honest. Walk DF in Maes Appendix 1 was correctly left unused (the walk St/DF coefficients are physically impossible as printed/extracted). Robilliard typo rows (tölt 2.0 RH stance “30±67”; tölt 3.5 LH swing “28±27”) are real in the table and were correctly unused.

---

## Row table

Columns: row, claim, status, note or correction, URL checked.

### 1a. Dog (Maes et al. 2008)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1a-pop | 5 Malinois, 486 sequences, overground, 0.4–10.0 m/s; withers 0.61±0.02 m | VERIFIED | Methods and Results. Fr uses **withers height**, g = 9.81. | https://journals.biologists.com/jeb/article/211/1/138/17472 (WebFetch 200) |
| 1a-W | Lateral walk (189): FL 51±3, HL 50±4, PL 84±5 % → LF 0.16, RH 0.50, RF 0.67 | VERIFIED | Table 2. Conversion LF=1−PL, RH=HL (mod 1), RF=1−PL+FL (mod 1) holds. f1/h1 same side (APS). | same, Table 2 |
| 1a-T | Trot (180): 51±3, 49±4, 50±4 → 0.50, 0.49, 0.01 | VERIFIED | Table 2; arithmetic holds. | same |
| 1a-P | Pace (30): 51±3, 50±3, 96±3 → 0.04, 0.50, 0.55 | VERIFIED | Table 2; arithmetic holds. Paper also notes this may be a paced walk (PL 96 vs theoretical 100). | same |
| 1a-TG | Transverse gallop (29): 29±4, 20±4, 63±9 → 0.37, 0.20, 0.66 | VERIFIED | Table 2; arithmetic holds. For asymmetrical gaits f1 = first fore, h1 = ipsilateral hind. | same |
| 1a-SR | Slow rotary (39): 21±3, −18±3, 76±6 → 0.24, 0.82, 0.45 | VERIFIED | Table 2; RH = 1−0.18 = 0.82. Signed HL is the paper’s convention (h1 is not necessarily the first hind). | same |
| 1a-FR | Fast rotary (19): 18±1, −15±2, 68±3 → 0.32, 0.85, 0.50 | VERIFIED | Table 2 used correctly. Results text says HL = −15±**1**. Prefer the table. | same |
| 1a-DFall | Pooled DF: a=0.652±0.003, b=−0.210±0.003, r²=0.938; u=2→0.51, 4→0.36, 8→0.22 | VERIFIED | Appendix 1, DF=a+b ln(u). Examples recomputed (natural log). | same, Appendix 1 |
| 1a-DFt | Trot: 0.635±0.010, −0.203±0.010, 0.707; u=3→0.41 | VERIFIED | Appendix 1; 0.635−0.203 ln 3 = 0.412. | same |
| 1a-DFp | Pace: 0.711±0.022, −0.293±0.022, 0.869; u=3→0.39 | VERIFIED | Appendix 1. | same |
| 1a-DFtg | Transverse gallop: 0.668±0.060, −0.215±0.036, 0.573; u=5→0.32 | VERIFIED | Appendix 1. | same |
| 1a-DFsr | Slow rotary: 0.453±0.054, −0.102±0.031, 0.227; u=6→0.27 | VERIFIED | Appendix 1. | same |
| 1a-DFfr | Fast rotary: 1.008±0.148, −0.357±0.068, 0.621; u=9→0.22 | VERIFIED | Appendix 1. | same |
| 1a-walkDF | Lateral walk DF not used (extraction scrambled) | VERIFIED | As printed/extracted, walk St asymptote 0.656 s and DF a=0.115 are physically impossible (stance would exceed cycle; walk DF ≈ 0.12 at u=1). Correctly withheld. | same, Appendix 1 walk rows |
| 1a-DF05 | DF falls below 0.5 above ~2.0 m/s (≈ Fr 0.7) | VERIFIED | Results. Fr uses withers height. | same |
| 1a-DFfh | Forelimb DF > hind in more than 60% of sequences; at very high speed hind DF exceeds fore | VERIFIED | Exact figures: 62.5% of sequences fore>hind; in fast rotary, hind>fore in 78.9%. The paraphrase is true. | same |
| 1a-lumbar | Lumbar sagittal flexion visible above 8.0 m/s | VERIFIED | Discussion: sagittal flexing “particularly in the lumbar region… beyond 8.0 m s–1”. | same |

### 1b. Elephants (Hutchinson et al. 2006)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1b-pop | 14 African + 48 Asian (60 filmed + 2 from Christian et al. 1999), 116–4632 kg, 621 trials, >2400 strides, 0.40–6.8 m/s; phase LH=0 in the paper | VERIFIED | Methods: 46 Asian + 14 African = 60, plus 2 Christian = 62 (48 Asian). 621 trials and >2400 strides in the Summary. LH=0 is the paper’s own definition. Mass 116 kg is in a figure caption (“smallest elephant (African; 116 kg)”); Table 1 lists Kedar as **119 kg**. Internal disagreement; C2 used the caption. Fr uses **hip height**. | https://journals.biologists.com/jeb/article/209/19/3812/16362 (WebFetch 200) |
| 1b-Pranges | P_LF, P_RH, P_RF, DF, β_diff observed ranges, both species | VERIFIED | Table 2, all six range pairs match. | same, Table 2 |
| 1b-Plin | Linear phase vs û equations (read as y=a+b·x) and R² | VERIFIED | Table 3 a,b,R² match. Footnote prints “linear: y=ax+b”, but y=ax+b would make P_RH ≈ 0.49·û, which is impossible against the observed 0.41–0.56 range. C2’s reading y=a+bx is right and is flagged as the reader’s. | same, Table 3 |
| 1b-DFlog | Mean DF = 0.53−0.32 log10(û) (Af, R² 0.89); 0.51−0.32 log10(û) (As, R² 0.84) | VERIFIED | Footnote y=a−b log x; only log10 fits (at û=0.10, DF≈0.85, the African maximum). | same |
| 1b-modal | LF 20–25% after LH; RH 25–30% after LF; RF 20–25% after RH; ±0.1 around modes | VERIFIED | Results, including “ranges of phases were ±0.1 from modal values” in 621 trials. | same |
| 1b-ex | At û=1.0 (African): LF 0.20, RH 0.50, RF 0.69, DF=0.53 | VERIFIED | Recomputed from the checked equations. | same |
| 1b-nogaits | Lateral-sequence footfall at every speed; never trot, pace or gallop | VERIFIED | Abstract/Summary and Results (also vs Hildebrand’s “slow trot”). | same |

### 1c. Icelandic horse (Robilliard et al. 2007)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1c-pop | 8 Icelandic horses, ridden overground, hoof accelerometers, 9687 LF strides; speed ranges and N as stated; canters/gallops transverse | VERIFIED | Results p.189 text and Methods: “both canters and gallops were of the transverse rather than rotary type”. | https://journals.biologists.com/jeb/article/210/2/187/17107 (WebFetch 200) |
| 1c-W15 | Walk @1.5 (N≈1021): 904±79 ms; DF 0.60, 0.60, 0.62, 0.61; 1.11 Hz | VERIFIED | Table 2 means. DFs = stance/stride recomputed (540/904 etc.). | same, Table 2 |
| 1c-W20 | Walk @2.0 (N≈623): 887±77; 0.59, 0.60, 0.60, 0.61; 1.13 Hz | VERIFIED | Recomputed. | same |
| 1c-To30 | Tölt @3.0 (N≈188): 571±48; 0.44, 0.44, 0.50, 0.50; 1.75 Hz | VERIFIED | Recomputed. | same |
| 1c-To40 | Tölt @4.0 (N≈824): 524±28; 0.40, 0.40, 0.44, 0.44; 1.91 Hz | VERIFIED | Recomputed. | same |
| 1c-To50 | Tölt @5.0 (N≈434): 486±21; 0.38, 0.37, 0.40, 0.40; 2.06 Hz | VERIFIED | Recomputed. | same |
| 1c-Tr35 | Trot @3.5 (N≈445): 600±39; 0.43, 0.43, 0.43, 0.42; 1.67 Hz | VERIFIED | Recomputed. | same |
| 1c-Tr40 | Trot @4.0 (N≈618): 569±36; 0.39, 0.40, 0.38, 0.40; 1.76 Hz | VERIFIED | Recomputed. | same |
| 1c-Tr50 | Trot @5.0 (N≈277): 511±36; 0.39, 0.36, 0.37, 0.37; 1.96 Hz | VERIFIED | Recomputed. | same |
| 1c-Pa60 | Pace @6.0 (N≈67): 471±27; 0.33, 0.33, 0.35, 0.35; 2.12 Hz | VERIFIED | Recomputed. | same |
| 1c-Pa80 | Pace @8.0 (N≈73): 426±25; 0.29, 0.30, 0.32, 0.32; 2.35 Hz | VERIFIED | Recomputed. | same |
| 1c-LC60 | Left canter @6.0 (N=55): 463±27; 0.35, 0.34, 0.35, 0.35; 2.16 Hz | VERIFIED | Table 3; recomputed. | same, Table 3 |
| 1c-LG70 | Left gallop @7.0 (N=39): 415±18; 0.34, 0.33, 0.35, 0.36; 2.41 Hz | VERIFIED | Table 3; recomputed. | same |
| 1c-RC60 | Right canter @6.0 (N≈232): 472±29; 0.34, 0.35, 0.35, 0.35; 2.12 Hz | VERIFIED | Table 3; recomputed. | same |
| 1c-RG70 | Right gallop @7.0 (N≈14): 434±27; 0.32, 0.33, 0.34, 0.34; 2.30 Hz | VERIFIED | Table 3 right-gallop block (HTML extract dropped the header in one fetch; the row is in the full table: 138/434, 142/434, 148/434, 146/434). | same, Table 3 |
| 1c-T4 | Left–right timing ratios, all eight gaits | VERIFIED | Table 4, every mean, s.d. and N matches. | same, Table 4 |
| 1c-order | Footfall orders for walk, tölt, trot, pace, left canter, left gallop | VERIFIED | Table 1. Left canter is “RH, RF before or at the same time as LH, LF”. | same, Table 1 |
| 1c-LHref | LH-referenced LF lag not in this paper (LF is the reference) | VERIFIED | Methods Eq. 1 sets LF_on = 0. Correctly sent to “Not found”. | same |
| 1c-hind% | Hind vs fore stance by gait: +2.7, +9.2, −1.6, +5.1, −0.9, +3.2, +3.0, +4.1 % | VERIFIED | Results sentence, complete list. | same |
| 1c-typos | Tölt 2.0 RH stance “30±67” and tölt 3.5 LH swing “28±27” unused | VERIFIED | Those cells are in the table as printed/extracted. Correctly unused. | same, Table 2 |

### 1d. Transverse vs rotary gallop (Biancardi & Minetti 2012)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1d-pop | 152 strides (31 trans., 121 rotary) of 15 species (Fig. 5 caption says 12) | VERIFIED | Text: 152 strides, 15 species. Fig. 5 area does say “12 spec”. Disagreement list item 10 is honest. | `/workspace/c2work/biancardi12.pdf` (independent pdftotext) |
| 1d-T7 | All Table 7 duty factors, lags, ratios, midtime, pair lag, means ± s.d. | VERIFIED | Table 7 matches every figure. Hindfeet ratio glossary: fraction of the stride supported by one or both hind feet. | same, Table 7 and Glossary |
| 1d-ph | Derived phases: trans. 0 / 0.16 / 0.26 / 0.48; rotary 0 / 0.87 / 0.30 / 0.47 | VERIFIED | Arithmetic holds if pair lag is used as in 1a. Glossary: pair lag = first forefoot → first hindfoot, which coincides with ipsilateral PL for the transverse means; rotary labels are a reference-hind convention (LH = 0), not necessarily anatomical left. Numbers are consistent under that convention. | same |
| 1d-spp | 37 transverse only, 35 rotary only, 17 switch (rotary at high speed) | VERIFIED | Results: T 37, R 35, S 17. | same |
| 1d-ex | Horse and camel transverse; cheetah rotary; some dogs switch | VERIFIED | Qualitative, consistent with Table 1 / discussion (canids use both; felids rotary). | same |

### 1e–1f. Cheetah/greyhound and qualitative gait definitions

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 1e-pop | 9 cheetahs, 53 strides, 8.7–17.8 m/s; 6 retired racing greyhounds, 47 strides, 9.9–19.0 m/s; adult; mass 30–45 and 28–35 kg | VERIFIED | Results and Table 1. One cheetah has no mass in Table 1; the 30–45 kg band is the weighed cheetahs. All “mature (over 2 years)”. | https://journals.biologists.com/jeb/article/215/14/2425/10852 (WebFetch 200); Table 1 also in `/workspace/c2work/hudson12.txt` |
| 1e-rot | Both used rotary gallop at these speeds | VERIFIED | Results. | same |
| 1e-DF | Cheetah DF significantly lower except lead hind; no mean DF table (Fig. 4 / LMM only) | VERIFIED | Results; correctly no digitised DF means. | same |
| 1e-swing | Min swing 0.20 s (cheetah), 0.19 s (greyhound) | VERIFIED | Discussion. | same |
| 3-che-f | Cheetah mean f 2.4→3.2 Hz over 8.7–17.8 m/s; greyhound no consistent trend; max 3.9 Hz in both; cheetah longer strides, λ linear in u | VERIFIED | Discussion. | same |
| 1f-hb | Half-bound: FL<50%, HL=0 | VERIFIED | Maes Table 1 (after Abourachid 2003). Definition only. | Maes Table 1 |
| 1f-bound | Bound: FL=0 and HL=0; PL depends on speed | VERIFIED | Maes Table 1. | same |
| 1f-sym | Symmetrical gaits: within-pair lags 50%±5% | VERIFIED | Maes Results; Renous 2004 is a methods paper on Hildebrand (© Académie des sciences / Elsevier, all rights reserved, free to read). | Maes; https://comptes-rendus.academie-sciences.fr/biologies/item/10.1016/j.crvi.2003.11.005.pdf (200) |
| 1f-zips | Tölt: 23 horses, 10 strides each, 2.9±0.28, 3.7±0.29, 4.7±0.53 m/s; at 4.7, 60% were 4-beat pace; true tölt only in a narrow range | VERIFIED | PubMed abstract. | https://pubmed.ncbi.nlm.nih.gov/11721548/ (203) |
| 1f-bik | 10 Icelandic, 27 trials, 0.89–5.98 m/s; lateral-sequence single-foot; bouncing; Fr above walk–run | VERIFIED | PubMed abstract. | https://pubmed.ncbi.nlm.nih.gov/16948593/ ; DOI 200 |

### 2. Gait-transition speeds

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| T-Gr | Walk→trot horses, 9 adults, 90–720 kg, L 0.7–1.4 m, treadmill; 1.6–2.3 m/s; Fr≈0.35 (v²/gL, L=leg length); narrow speed range | VERIFIED | Summary/abstract, word for word on the numbers. | CoLab/PubMed abstract of 10.1242/jeb.01277; PMID 15531642 |
| T-AJ-w | Walk→trot general Fr ≈0.5, via Alexander 1993 citing A&J 1983 | VERIFIED | Alexander 1993 p.315–316: “about 0.5”. V-sec as to A&J 1983 itself (not opened). Length in 1993 is **leg length L**. | J-STAGE PDF https://www.jstage.jst.go.jp/article/jrsj1983/11/3/11_3_314/_pdf (200) |
| T-AJ-g | Trot→gallop general Fr ≈2.5, same source | VERIFIED | The paper prints **2.53** (citation 3, A&J 1983) and also “between 0.5 and 2.5” / “above 2.53”. ≈2.5 is a rounding; prefer **2.53** when quoting this page. Not a FIX. | same |
| T-Ir-range | Trot→gallop general Fr 2 to 3, mice to rhinoceros, via Iriarte-Díaz 2006 citing A&J 1983 | VERIFIED | Discussion, exact wording. V-sec as to A&J 1983. Here L is called “limb length”; degu Fr used hip height from 2 individuals. | https://journals.biologists.com/jeb/article/209/20/4061/16386 (WebFetch 200) |
| T-Ir-degu | Degu Fr 2.8±0.1, 2.7±0.1, 2.8±0.1 (control, loaded/injected, overfed) | VERIFIED | Exact figures. Treatments are named Control / **Injected** / Overfed; the text also says “loaded animals”. | same |
| T-H74-v | v_tg = 1.53·M^0.24 (Heglund 1974 via Hutchinson Table 6) | VERIFIED | Correctly copied from Hutchinson Table 6 (ref. 2). Original 1974 paper not opened (abstract has no coefficients). Residual V-sec risk. | Hutchinson Table 6; PMID 4469699 abstract |
| T-H74-f | f = 4.48·M^−0.14 Hz at trot→gallop, same | VERIFIED | Table 6; predicted values check (e.g. 560 kg → 1.8 Hz). Original not opened. | same |
| T-H74-L | L = 0.35·M^0.38 m, same | VERIFIED | Table 6 (560 kg → 3.9 m). Original not opened. | same |
| T-HT88-min | Min trotting speed 0.593·M^0.249 via Hutchinson Table 6 | VERIFIED | Table 6, ref. 4. Original tables not opened. Abstract confirms 16 species, treadmill, scaling ≈M^0.2, not these coefficients. | Hutchinson Table 6; PMID 3193059 abstract |
| T-HT88-abs | Equivalent speeds all scale ≈M^0.2; max sustained = 2.6× min in trot and 2.1× min in gallop; f at equivalent speeds ≈M^−0.15 | VERIFIED | Abstract, 16 species, 30 g mice to 200 kg horses. | PMID 3193059 |
| T-HT88-U | v_tg ∝ M^0.216; f ∝ M^−0.150; mouse ≈0.8 m/s, 170 kg pony ≈5 m/s; preferred trot 1.09·M^0.222, f 3.35·M^−0.130 | UNVERIFIABLE | C2 marked **U**. I also could not open Heglund & Taylor 1988 tables or independently confirm these coefficients. Leave unused. | Original Wisdom (curl 406 here); paper full text not reached |
| T-Fa | Trot→gallop where galloping costs more energy; with load, lower speed but same peak force | VERIFIED | PubMed abstract, qualitative. n not in the abstract, as C2 says. | PMID 1857965 |
| T-el-Fr1 | Elephant aerial phase (hindquarters first) at Fr≈1, h=hip height | VERIFIED | Summary. | Hutchinson HTML |
| T-el-max | Fastest: 6.8 m/s, bull ≈2790 kg, Fr 2.8; group max Fr 3.4 (û 1.8) | VERIFIED | Results: û_max 1.8 (Fr=3.4); bull 6.8 m/s, Fr=2.8, ~2790 kg. | same |
| T-dog-Fr | Dog DF<0.5 above 2.0 m/s, Fr≈0.7 (withers); cycle-duration plateau ≈4.0 m/s, Fr≈2 | VERIFIED | Maes Results and Discussion. | Maes HTML |
| T-ice | Icelandic speed ranges: see 1c | VERIFIED | Robilliard Results. | Robilliard HTML |
| T-Hoyt | Hoyt & Taylor 1981 preferred speeds: paywalled, no number given | UNVERIFIABLE | Correctly withheld. Abstract (3 horses, 110–170 kg) has no speeds. Nature page exists (200). | https://www.nature.com/articles/292239a0 (200, abstract) |

### 3. Stride equations

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 3-AJ-w | λ/h = 2.4·Fr^0.34, walking quadrupeds, via Hutchinson Table 6 | VERIFIED | Table 6 a=2.4, b=0.34, ref. 1 (A&J 1983). At Fr=0.067 predicts 1.0, as the table says. Original not opened. | Hutchinson Table 6 |
| 3-AJ-rc | λ/h = 1.9·Fr^0.40, running cursorial | VERIFIED | Table 6. Original not opened. | same |
| 3-AJ-rnc | λ/h = 2.7·Fr^0.28, running non-cursorial | VERIFIED | Table 6. Original not opened. | same |
| 3-AJ-DFf | DF_fore = 0.52·Fr^−0.14 (walk); 0.52·Fr^−0.28 (run) | VERIFIED | Table 6. Original not opened. | same |
| 3-AJ-DFh | DF_hind = 0.51·Fr^−0.18 (walk); 0.53·Fr^−0.28 (run) | VERIFIED | Table 6. Original not opened. | same |
| 3-A77 | λ/h = 2.3·û^0.60 (=2.3·Fr^0.30), Alexander 1977 via Table 6 | VERIFIED | Table 6 a=2.3, b=0.60, ref. 3. Hutchinson’s reference list does give “Allometry of the limbs of antelopes”, J Zool 183:125. Original not opened. | same |
| 3-A93 | S/L = 2.5·(v²/gL)^0.3, Alexander 1993 Eq. 2 | VERIFIED | Eq. (2) in the J-STAGE PDF; L = leg length. | J-STAGE PDF (200) |
| 3-HT88-fmax | f at max sustained gallop = 4.7·M^−0.162 Hz | VERIFIED | Table 6 prints b=**0.162** (no minus). C2 is right that the minus is required for the predicted values (4.7·560^−0.162 ≈ 1.7 Hz). Original not opened. | Hutchinson Table 6 |
| 3-el-rel | Elephant λ/h, relative f, λ(m), f(Hz) power fits, both species, R² | VERIFIED | Table 3, all eight power-law pairs match (y=a x^b). | Hutchinson Table 3 |
| 3-dog-λ | Dog λ = 0.489+0.297u (±0.009, ±0.003; r² 0.964) | VERIFIED | Appendix 1, all gaits. | Maes Appendix 1 |
| 3-dog-f | Dog f = 1.314+0.762 ln u (±0.012, ±0.011; r² 0.910) | VERIFIED | Appendix 1. | same |
| 3-dog-λg | λ by gait: trot 0.498+0.286u; pace 0.252+0.365u; trans. 0.695+0.256u; slow rot. 0.380+0.346u; fast rot. −2.363+0.599u | VERIFIED | Appendix 1 intercepts and slopes match (s.e. present in the source, as C2 says). | same |
| 3-Hil-U | Horse ≈2¼ strides/s at ≈35 mph; cheetah ≈2½ strides/s at ≈45 mph; cheetah stride 21–26 ft (Hildebrand 1959 via Original Wisdom) | UNVERIFIABLE | C2 marked **U**. Primary paywalled; I did not confirm these numbers. | S23 DOI 403; S24 curl 406 here |

### 4. Spine

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 4-F00-FE | Horse walk 1.6 m/s: FE ROM caudal to T10 ≈7°; 5 Dutch Warmblood, bone-fixated T6–S3 | VERIFIED | PubMed abstract. | PMID 10772104 |
| 4-F00-LBAR | LB up to 5.6° (cranial thoracic and pelvis); AR 4° (T6) to 13° (tuber coxae) | VERIFIED | Abstract. | same |
| 4-F01t | Trot 4.0 m/s: FE 2.8–4.9°; LB 1.9–3.6°; AR 4.6–5.8° (T10 3.1°, T13 3.3°); 5 young Dutch Warmblood | VERIFIED | PubMed abstract. | PMID 11341399 |
| 4-F01c | Canter 7.3 m/s: FE 15.8±1.3°; LB 5.2±0.7°; AR 7.8±1.2°; L5–S3 up to 8.6°; 5 Dutch Warmblood, Steinmann pins | VERIFIED | PubMed abstract, including ± s.d. | PMID 11721556 |
| 4-Kam | Cheetah 15–18 m/s: 0.47±0.029 rad = 26.9±1.7° (D); ± s.e., 8 strides; 4 adult males 40–50 kg (Kamimura et al. 2021 data) | VERIFIED | PMC/Frontiers full text: δφ = 0.47±0.029 rad (s.e.), 8 strides, 4 adult males 40–50 kg, 15–18 m/s. Definition is the relative angle between neck-root–T12 and T12–tail-root. 0.47×180/π = 26.9; 0.029×180/π = 1.7. **CC BY**. | https://pmc.ncbi.nlm.nih.gov/articles/PMC9049215/ (200); doi 200 |
| 4-Bel-gh | Greyhound fast straight gallop: lumbosacral curvature change 97.5°; 1 sequence, weak method | VERIFIED | Fig. 10 caption: 97.5°. C2’s “weak method” flag is fair. **CC BY 4.0**. | Europe PMC XML PMC10780164 |
| 4-Bel-wolf | Wolf chasing hare: 56° and 89°; BBC footage | VERIFIED | Fig. 10 caption: 56° and 89°. | same |
| 4-Bel-dog | Dog walk/trot: used 3–4° vs available 28–37° (citing Benninger 2004; Pylypchuk 1975; Wachs 2016) | VERIFIED | Text, V-sec as to those three papers (not opened). | same |
| 4-Sch-n | 7±1 presacral joints; 1 individual per species; high-speed cineradiography | VERIFIED | Abstract and Methods. | HAL PDF https://hal.science/hal-00365971/file/3925.pdf (200) |
| 4-Sch-spd | Speeds “about 1.15–1.18 m/s” for all five species | FIX | **Correction (Schilling & Hackert 2006, Methods, speeds of the analysed trials, access 2026-11-16):** kowari **0.92±0.06** m/s; grey short-tailed opossum **1.15±0.19**; tree shrew **1.48±0.15**; pika **1.18±0.05**; cui **1.14±0.12**. Do not use 1.15–1.18 m/s as a common band. URL: https://hal.science/hal-00365971/file/3925.pdf | HAL PDF (200) |
| 4-Sch-PS1 | PS1 effective angular movement 7–22° (pika 7–8°; tree shrew and kowari 17–22°) | VERIFIED | Results, p.3932 area. | same |
| 4-Sch-cr | Joints cranial to PS3: 1–9° each | VERIFIED | Results. | same |
| 4-Sch-pel | Pelvic-movement amplitude: stance 29–44°; swing 40–51° | VERIFIED | Results/Discussion, “29° to 44°” (stance) and “40° to 51°” (swing). | same |
| 4-Sch-cite | Dog 50°; cheetah 65°; horse 20° “pelvic movement”, citing Alexander et al. 1980 and Hildebrand 1959 | VERIFIED | Discussion, those exact figures and citations. V-sec as to the 1959/1980 papers (not opened). | same |
| 4-Hil-U | Cheetah ≈130°, horse ≈60° pelvis-to-scapula (Hildebrand 1959 via Original Wisdom) | UNVERIFIABLE | C2 marked **U**. Primary not opened. Disagreement item 4 (65°/20° vs 130°/60°) is properly left unresolved. | S23/S24 |
| 4-Mae-sp | Malinois lumbar flexion visible above 8.0 m/s | VERIFIED | Same passage as 1a-lumbar. | Maes HTML |
| 4-Sch-time | Max flexion between last third of swing and touch-down; max extension in first third of swing | VERIFIED | Abstract. | HAL PDF |

### Not found / Sources disagree

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| NF | Gaps listed (horse LH=0 phases, Hildebrand ranges, cat, cheetah numeric DF/phases, half-bound measurements, gaited-horse ambles, Hoyt speeds, H&T 1988 Table 5, A&J 1983 full text, Hutchinson 2003, dog/greyhound bone-marker ROM, Muybridge as numbers) | VERIFIED | These are genuine gaps relative to what I could open. I also could not open those paywalled primaries. | — |
| SD1 | Stride-length k = 2.3 vs 2.5 vs the A&J split | VERIFIED | 2.3 (Table 6 / Alexander 1977 quote), 2.5 (Alexander 1993 Eq. 2), and the three A&J forms in Table 6. At Fr=1 the five λ/h values are 2.3, 2.5, 2.4, 1.9, 2.7 as stated. | Hutchinson Table 6; Alexander 1993 |
| SD2 | Walk–trot Fr 0.35 (Griffin, leg length) vs ≈0.5 (Alexander 1993, “hip height”) vs dog DF<0.5 at Fr≈0.7 (withers) | FIX | The three numbers are real, and the length-used warning is right for Griffin (leg length) and Maes (withers). **Correction:** Alexander 1993 uses **leg length L**, not hip height. Hip height is the A&J 1983 convention and should not be attributed to the 1993 paper. Access 2026-11-16, J-STAGE PDF. | J-STAGE PDF; Griffin abstract; Maes HTML |
| SD3–SD12 | Remaining disagreement items (trot–gallop Fr; Hildebrand spine vs Kamimura; Faber vs Hildebrand horse; elephant fore/hind DF vs Hildebrand; elephant “trot”; H&T 1988 200 kg vs secondary masses; Biancardi 15 vs 12 species; Robilliard typos; Hutchinson Table 3 footnote) | VERIFIED | Each item is supported by the sources I opened, or is properly left open where the primary is still closed (Hildebrand 1959; H&T 1988 tables). Item 9: the 1988 abstract does say “200 kg horses”; the secondary mass conflict remains unchecked against the tables. | as cited above |

---

## Source table

| key | URL | resolves y/n | licence correct y/n |
|---|---|---|---|
| S1 Robilliard 2007 | https://doi.org/10.1242/jeb.02611 · https://journals.biologists.com/jeb/article-pdf/210/2/187/1260108/187.pdf · https://pubmed.ncbi.nlm.nih.gov/17210956/ · HTML https://journals.biologists.com/jeb/article/210/2/187/17107 | y, with the same pattern C2 noted: DOI/PDF curl **403**; PubMed **203**; HTML WebFetch **200** and full tables readable | y — © Company of Biologists, free to read, not an open licence; cited-fact is right |
| S2 Hutchinson 2006 | https://doi.org/10.1242/jeb.02443 · PDF 403 · RVC https://researchonline.rvc.ac.uk/id/eprint/1143/1/1143.pdf **403** · HTML https://journals.biologists.com/jeb/article/209/19/3812/16362 WebFetch **200** | y (HTML); RVC PDF blocked here | y — © CoB, cited facts. OpenAlex tags the RVC copy CC BY; C2 correctly says that was not checked on the copy (the copy was 403 here too) |
| S3 Biancardi 2012 | https://doi.org/10.1242/jeb.073031 (403) · PubMed 203 · PDF already on disk, valid 13-page JEB PDF | y (PDF in `/workspace/c2work/biancardi12.pdf`; DOI 403) | y — cautious “OpenAlex tags CC BY, but no licence line in the PDF, treat as cited facts” is the right stance |
| S4 Hudson 2012 | https://doi.org/10.1242/jeb.066720 (403) · PDF 403 · HTML https://journals.biologists.com/jeb/article/215/14/2425/10852 WebFetch **200** | y (HTML) | y — © CoB, cited facts |
| S5 Maes 2008 | https://doi.org/10.1242/jeb.008243 (403) · PDF 403 · HTML https://journals.biologists.com/jeb/article/211/1/138/17472 WebFetch **200** | y (HTML) | y — © CoB, cited facts |
| S6 Griffin 2004 | https://doi.org/10.1242/jeb.01277 (403) · PDF 403 · PubMed 203 · Summary readable via abstract/CoLab | y (abstract/Summary) | y — © CoB, cited facts |
| S7 Heglund & Taylor 1988 | Publisher HTML 403 · PubMed 203; abstract fetched via NCBI E-utilities | y (abstract only; full PDF still blocked) | y — © CoB, cited facts |
| S8 Heglund et al. 1974 | DOI 403; PubMed abstract OK (qualitative only) | y (abstract); full text not reached | y — paywalled AAAS; cited facts |
| S9 Alexander & Jayes 1983 | DOI 403; Wiley landing exists; full text not reached | y as a record; **n** as full text | y — paywalled Wiley/ZSL; cited facts; C2 did not pretend to have the PDF |
| S10 Alexander 1993 | https://www.jstage.jst.go.jp/article/jrsj1983/11/3/11_3_314/_pdf **200**; PDF read | y | y — © Robotics Society of Japan, free on J-STAGE, not an open licence; cited facts |
| S11 Iriarte-Díaz 2006 | DOI 403; HTML https://journals.biologists.com/jeb/article/209/20/4061/16386 WebFetch **200** | y (HTML) | y — © CoB, cited facts |
| S12 Faber 2000 walk | DOI **200**; PubMed 203; abstract read | y | y — published paper (AVMA), cited facts |
| S13 Faber 2001 trot | DOI **200**; PubMed abstract OK | y | y — cited facts |
| S14 Faber 2001 canter | DOI 403; PubMed 203; abstract read | y (abstract) | y — Wiley; cited facts |
| S15 Kamimura 2022 | DOI **200**; PMC **200**; full text read | y | y — **CC BY** (Frontiers/PMC licence line). C2’s “CC BY 4.0” is the standard Frontiers licence; the page I opened says “Creative Commons Attribution License (CC BY)” |
| S16 Belyaev 2024 | DOI 403; Europe PMC page 403 from curl; full-text XML already obtained and re-read | y (XML) | y — **CC BY 4.0**, licence statement in the XML |
| S17 Schilling & Hackert 2006 | DOI 403; HAL PDF **200**, independently re-extracted | y | y — © CoB; author copy on HAL, no CC line; cited facts |
| S18 Zips 2001 | PubMed 203; DOI 403; abstract read | y (abstract) | y — cited facts |
| S19 Biknevicius 2006 | DOI **200**; PubMed abstract OK | y | y — cited facts |
| S20 Renous 2004 | PDF **200**, read | y | y — © Académie des sciences / Elsevier, “All rights reserved”, free to read |
| S21 Farley & Taylor 1991 | PubMed 203; abstract read | y (abstract) | y — paywalled; cited facts |
| S22 Hoyt & Taylor 1981 | https://www.nature.com/articles/292239a0 **200** (abstract) | y (abstract only) | y — paywalled Springer Nature |
| S23 Hildebrand 1959 | DOI 403; primary not read | n as full text | y — paywalled; C2 used it only for U rows and for citations via S17/S24 |
| S24 Original Wisdom | curl **406** here (C2 reported 200). Secondary web pages | uncertain from this box | y — © the site; C2 used it only as leads and marked every number U |
| S25 Cartmill et al. 2002 | DOI 403; not used for numbers | n as full text (abstract only, as C2 said) | y — published paper, cited facts (definitions only) |
| S26 Muybridge 1887 | Wikimedia **200**; Digital Commonwealth **200** | y | y — US public domain (1887); Wikimedia’s “may not be PD outside the US” caveat is correctly stated. No numbers were extracted |

Every number I upgraded or corrected is tied to a source above. I did not invent a value. Nothing was posted, committed, or sent outside this private check.
