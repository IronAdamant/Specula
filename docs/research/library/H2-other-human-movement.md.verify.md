# H2 verification: other human movement (sit-to-stand, turning, stairs, jumping, reaching, carrying, falling)

Verifier: Library Verify (adversarial re-check). Written 2026-11-17, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It checks `H2-other-human-movement.md`. H2 itself was not edited. All URLs below were accessed on **2026-11-17**.

## Version checked
- Snapshot at the start of the check: mtime **2026-11-16 10:41:22 AEDT**.
- Motion Data edited H2 at **2026-11-17 08:54:15 AEDT** (30,675 bytes). The diff and number-diff show that **only the access-date column and separators were added**. No values changed, so nothing was re-checked because of that edit.
- Re-stat before writing: **2026-11-17 08:54:15 AEDT**, byte-identical to my post-edit copy. **This is the final mtime verified against.**

## Summary

| Status | Rows | Meaning |
|---|---:|---|
| **VERIFIED** | **80** | Number, units and population match a table or sentence I opened. Open-access papers were read as Europe PMC full-text XML; closed papers as abstracts. |
| **FIX** | **3** | S3, S4 (Greve trunk-angle reference) and S5 (Li population is all male) |
| **UNVERIFIABLE** | **0** | |
| **Total** | **83** | S1–S21, T1–T11, ST1–ST9, J1–J13 plus the J power note, R1–R11, C1–C8, F1–F9 |

**Worst problems**
1. **S3/S4 Greve 2013 "Peak trunk flexion" 36.95±7.98° (young) and 33.69±6.73° (old).** The numbers are right. In Table 2 the row is "Trunk Flexion (°)ᵃ", and the footnote says *"ᵃangle with the horizontal"*. A generator that reads 37° as forward lean from upright would be wrong if the angle is measured from the horizontal.
   - **Correction:** add "(Table 2 footnote: angle with the horizontal; the reference is not further defined)".
   - Do not mix this with Hayamizu's joint angles.
2. **S5 Li 2021: the population is "30 healthy adult male subjects"** (Methods 2.1). Age 25.7±2.01 y is in Table 1, and the 1.60 s figure is in the Discussion: *"the average STS motion time of 30 healthy subjects was 1.60 seconds"*.
   - **Correction:** "healthy young **men**, 25.7±2.0 y (n=30)".
3. **C1, C2 and C4–C6 (Huang & Kuo 2014) were confirmed from search-result snippets of the PMC/JEB full text**, because the PMC HTML hits a captcha and the publisher blocks the XML download. The snippets were verbatim: 0.662 m, 0.142 m, 0.532 s, 0.179 s; no significant step changes; step-width variability +50%, P=0.01; double support +30%, 29%→38%. C3 and C7 come from the abstract: N=8, up to 40%, about 7.6 W per kg of load. I count them VERIFIED, but this is the weakest evidence in the file.

**Dead links:** none. Every H2 URL resolved; the PMC HTML sometimes shows a captcha, but the same text is available from Europe PMC.

## Row table

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| S1–S2 | Greve STS duration 1.71±0.17 (young 24.3±2), 1.78±0.17 (old 76.4±5.2); enrolled 15/11 | VERIFIED | Table 2 and Table 1. Methods give 23.8±2.2 and 76±5.1, which H2 already lists as a disagreement. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3808394/ (Europe PMC XML) |
| S3 | Peak trunk flexion, young 36.95±7.98° | **FIX** | Value right; add "angle with the horizontal" (Table 2 footnote ᵃ) | same |
| S4 | Peak trunk flexion, old 33.69±6.73° | **FIX** | Same | same |
| S5 | Li: 1.60 s, healthy young 25.7±2.0 y, n=30 | **FIX** | **All male.** The 1.60 s and the age are right. | https://pmc.ncbi.nlm.nih.gov/articles/PMC8183776/ |
| S6–S9 | Hayamizu phases 0.62±0.09 / 0.19±0.12 / 0.99±0.25 / total 1.80±0.33 s; n=17, 71.9±5.8 y, women | VERIFIED | Table 4; control group | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ |
| S10–S17 | Hayamizu angles (72.2±5.3 … 4.2±6.7) | VERIFIED | Table 3, every cell | same |
| S18–S20 | Van Roie 5-rep STS 7.86±0.71 / 8.47±0.99 / 8.59±1.01 s | VERIFIED | Table 1 | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ |
| S21 | STS transition 4.10±0.37 / 4.39±0.49 / 4.47±0.49 s | VERIFIED | Table 1. H2's "do not divide by 5 without checking" caveat is fair. | same |
| T1–T11 | Khobkhun Table 1 (all cells); n=21 + 21; 66.4±3.25 and 22.47±2.18 y | VERIFIED | Table 1. Table 2 young head onset 0.53/0.53/0.53, which H2 already lists as a disagreement. | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ |
| ST1–ST4 | Van Roie stair 6 steps 1.45±0.19 / 1.52±0.17 / 1.78±0.34; loaded 1.48/1.57/1.81 | VERIFIED | Table 1 | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ |
| ST5 | Riser 18 cm, tread 26.5 cm | VERIFIED | Methods (also chair 0.46 m, six steps) | same |
| ST6–ST9 | Protopapadaki differences 28.10 (4.08), 3.39 (7.20), 9.90 (3.80) / 8.78 (4.80); 18 cm / 28.5 cm, 4 steps; n=33, 16M/17F | VERIFIED | Abstract | https://europepmc.org/abstract/MED/17126461 |
| J1–J13 | Koivunen CMJ by age and sex, n and mean (SD), all cells | VERIFIED | Tables 1–2 (Europe PMC XML). Grok B could not reach these tables; I read them directly. | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ |
| J-note | Males 21–25 peak power 47.5 (5.9) W/kg; J13 male age 69.6 | VERIFIED | Same tables | same |
| R1–R11 | Viau healthy physical-condition values (0.68±0.17 … 18.2±12.1) | VERIFIED | Tables 2–3, n=8 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ |
| C1–C2 | Huang & Kuo unloaded 0.662 m / 0.142 m / 0.532 s; DS 0.179 s | VERIFIED (snippet) | PMC/JEB full-text snippets; see problem 3 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ ; https://doi.org/10.1242/jeb.091587 |
| C3 | Up to 40% body weight | VERIFIED | Abstract (N=8) | same |
| C4–C6 | No significant step change; DS 29→38% (+30%); step-width variability +50%, P=0.01 | VERIFIED (snippet) | As C1 | same |
| C7 | About 7.6 W per kg of load | VERIFIED | Abstract | same |
| C8 | Liew meta-analysis directions; 54 studies | VERIFIED | Abstract | https://europepmc.org/abstract/MED/27705050 |
| F1–F4 | Choi 1271±648 ms, 583±255 ms, impact velocities 2.14/2.91/2.87, impacts 25/12/21; 80±9.8 y | VERIFIED | Abstract | https://europepmc.org/abstract/MED/25769730 |
| F5–F9 | Vlaeyen 1946±639 ms; directions, landing, activity, body sites | VERIFIED | Table 2 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3850536/ |

## Not found
- No new gaps. H2's "Not found" list was re-read and is accurate. In particular, the SD for Li's 1.60 s is indeed not printed.

## Sources disagree
- H2's existing items (fall duration definitions, STS duration bands, Greve age figures, Khobkhun Table 1 vs Table 2, Khobkhun step frequency) were all re-checked and are accurate.
- **Added:** Greve's trunk "flexion" is an angle with the horizontal, while Hayamizu's hip, knee and ankle angles are joint angles. These are different references, so do not blend them.

## Source table

| Key | URL | Resolves (2026-11-17) | Licence correct |
|---|---|---|---|
| 1 Greve 2013 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3808394/ | y | y (CC BY) |
| 2 Li 2021 | https://pmc.ncbi.nlm.nih.gov/articles/PMC8183776/ | y | y (CC BY-NC 4.0) |
| 3 Hayamizu 2025 | https://pmc.ncbi.nlm.nih.gov/articles/PMC11989561/ | y | y (CC BY 4.0) |
| 4 Van Roie 2019 | https://pmc.ncbi.nlm.nih.gov/articles/PMC6336282/ | y | y (CC BY 4.0) |
| 5 Khobkhun 2021 | https://pmc.ncbi.nlm.nih.gov/articles/PMC8074235/ | y | y (CC BY 4.0) |
| 6 Protopapadaki 2007 | https://doi.org/10.1016/j.clinbiomech.2006.09.010 ; https://europepmc.org/abstract/MED/17126461 | y | y (closed) |
| 7 Lencioni 2019 | https://pmc.ncbi.nlm.nih.gov/articles/PMC6897988/ | y | y (CC BY 4.0) |
| 8 Koivunen 2026 | https://pmc.ncbi.nlm.nih.gov/articles/PMC13280190/ | y | y (CC BY 4.0) |
| 9 Viau 2004 | https://pmc.ncbi.nlm.nih.gov/articles/PMC546398/ | y | y (CC BY 2.0) |
| 10 Huang & Kuo 2014 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3922835/ | y (captcha at times) | y (© Company of Biologists) |
| 11 Liew 2016 | https://europepmc.org/abstract/MED/27705050 | y | y (closed) |
| 12 Choi 2015 | https://europepmc.org/abstract/MED/25769730 | y | y (closed) |
| 13 Vlaeyen 2013 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3850536/ | y | y (CC BY 2.0) |

## Grok pass
- **"Grok B"** (shared with H5 and H4), 2026-11-17 08:49:22–09:08:49 AEDT, exit 0.
- For H2 it confirmed Khobkhun Tables 1 and 2 and the abstracts it reached.
- It reported Koivunen's tables as **NOT FOUND** (captcha, Wiley). I read those tables myself from the Europe PMC XML.
- No Grok-only number is used here.
