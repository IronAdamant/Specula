# B4 verification: visemes and phoneme-to-viseme maps

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `B4-visemes.md`. B4 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/B4-visemes.md`
- Size: 12,248 bytes, 226 lines
- File mtime: **2026-11-16 10:06:24 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 10:32 AEDT**. Bit-for-bit identical to a start-of-check snapshot.
- Grok CLI: tried first at the start of this B2–B5 pass (`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`). It exited at once with **“Not signed in”**. **No login was attempted.** Checks used curl, local copies under `/tmp/b4verify/`, and previously fetched pages. Access date: **2026-11-16**.

## Summary

Unit of counting: **one data row** (or one separately stated factual claim in table notes/headings). Multi-number rows were checked number-by-number, including population, sex, age and n where claimed. Licence findings are in the source table and in “Most serious problems,” not all in the row counts.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **81** | The cited source opens, and it states that grouping, number, units, and (where claimed) population/material. |
| **FIX** | **3** | A stated claim is wrong or more general than the source. |
| **UNVERIFIABLE** | **0** | Fisher 1968, Preston Blair drawings, per-viseme millimetre tables, and the paid ISO text are correctly left unused, so they are not counted. |
| **Total data rows** | **84** | Table 1: 15 visemes + 1 after-text blend claim; Table 2: 15; Table 3: 11 ID rows + 1 heading (“22 + silence”); Table 4: 17; Table 5a: 5; Table 5b: 12; Table 5c: 1; Table 6: 6. |

**Most serious problems**

1. **FIX — “two visemes can be blended by a weight (Section 2.2).”** Ostermann et al. 1998, Section 2.2 / 2.3, says **“Two facial expressions can be blended with a weighting factor”** (the example is joy/sadness). That sentence is about expressions, not visemes. The same paper does say there are **“only 14 static visemes that are clearly distinguished”** — that part is exact. The MPEG-4 neutral-face definition also matches (gaze along Z, muscles relaxed, eyelids tangent to the iris, pupil one third of the iris diameter unit, lips in contact, upper teeth touch the lower teeth; B4’s “teeth touching” is a fair shortening).
2. **FIX — Table 3 heading “Microsoft Azure Speech viseme IDs (22 + silence).”** The Azure table is IDs **0–21**, and **ID 0 is silence**. That is **22 IDs including silence**, not 22 plus silence. B4’s own Summary already says “Microsoft Azure uses 22 IDs,” which is right. The 22 IPA mappings, the 100-ns tick offset, 60 FPS, 55 facial positions, and the locale caveat all match the Learn page.
3. **FIX — Schwartz & Savariaux 2014 population written “French speakers.”** The paper recorded **“a small database of 6 repetitions of 8 syllables /pa ta ka ba da ga ma na/ uttered by a French speaker”** (singular). Sex and age of that speaker are not stated in the recording sentence (a later “male French speaker” refers to a separate /aba, ada, aga/ toy-model corpus). The asynchrony figures themselves match the abstract: **20 ms audio lead to 70 ms audio lag** for those CV syllables, and **40 ms audio lead to 200 ms audio lag** as the range more complex material “should result” in (B4’s “modelled from the above” is fair). Lips were extracted every 20 ms, as written.

**What is in good shape.** All 15 MPEG-4 viseme rows match Ostermann Table 1 (phonemes and example words). Viseme 0 is correctly taken from the CMU 18-796 slides (“none”; example “na”; B4 writes “none (neutral)” and “—”). All 15 Meta / Oculus Lipsync names and phoneme lists match the developer page (the page does say the 15 visemes follow MPEG-4). All 17 Polly en-US viseme codes and their IPA sets match the AWS table (the Summary’s “~16” is a loose count; the table correctly has 17 codes). All five Cappelletta Table 6 map sizes match (silence excluded). The full Jeffers Table 1 ranks, occurrence percentages, and TIMIT sets match, including **/ao/ listed under both /G and /I**, which B4 correctly flags under Sources disagree. The compact Lee & Yook list and the “many-to-many for 5 vowels / first viseme” note match Table 5 and the surrounding text. Chandrasekaran 2009: 100–300 ms time-to-voice, 2–7 Hz modulation, Fig. 8F word-initial means ± SD and n, and Fig. 9B VCV means ± SD and n all match; the paper does use English and a French spontaneous-speech database plus the Wisconsin X-ray microbeam corpus. Sources disagree on visual lead is a fair reading of Schwartz’s argument versus Chandrasekaran. Not found / unused items (Fisher 1968, Blair drawings, ISO text, per-viseme mm tables) are honest.

**Licences.** None of the used sources is wrongly marked as free. Ostermann is treated as © ISCA/authors, facts cited (the ISO standard itself was not read). CMU slides © author, cross-check only. Meta, Microsoft, and AWS docs are cited as facts; B4 honestly says no licence notice was checked on the Azure page. Cappelletta PDF carries “Copyright c SciTePress.” Chandrasekaran 2009 and Schwartz & Savariaux 2014 are **CC BY 4.0** (licence URL https://creativecommons.org/licenses/by/4.0/ on the PLOS XML). Blair unused.

**Minor notes, not counted as FIX.** The Meta viseme-reference page sits in an end-of-life Unity docs tree; the prominent EOL banner on the fetched HTML is for the **Oculus Spatializer Plugin**, not clearly for the Lipsync viseme table itself — B4’s “marked end-of-life” is therefore a little broader than the banner. Polly “~16” in the Summary should be 17. B4’s verification note mentions Cappelletta Table 4 (Bozkurt), which was not copied into B4.

## Row table

Access date 2026-11-16 throughout.

### Table 1 — Ostermann et al. 1998, Table 1, plus CMU viseme 0

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| Visemes 0–14 (15) | Phonemes and example words as tabulated | VERIFIED | 1–14 match Ostermann Table 1 exactly (including T, D / think, that; tS, dZ, S / chair, join, she; A:, e, I, Q, U). Viseme 0 “none (neutral)” is from the CMU slides (“none” / “na”). ISO/IEC 14496-2 itself was not read, as B4 says. | [ISCA PDF](https://www.isca-archive.org/icslp_1998/ostermann98_icslp.pdf) (200); [CMU slides](https://course.ece.cmu.edu/~ece796/mpeg4.pdf) (200) |
| After-text: two visemes blended by a weight (Section 2.2); 14 static visemes; MPEG-4 neutral face | Blend + count + neutral-face definition | FIX | **Correction:** Section 2.2/2.3 blends **two facial expressions**, not two visemes. “Only 14 static visemes that are clearly distinguished” is exact. Neutral-face definition matches; “teeth touching” shortens “upper teeth touch the lower teeth.” | same Ostermann PDF |

### Table 2 — Meta / Oculus Lipsync (15 visemes)

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| sil, PP, FF, TH, DD, kk, CH, SS, nn, RR, aa, E, I, O, U (15) | Names and phonemes as listed | VERIFIED | Matches the developer-page list, including TH “th” only (not ð), I “ih”, O “oh”, U “ou”, sil “neutral.” Page states the 15 visemes follow MPEG-4. Reference images were not used, as B4 says. | [Meta docs](https://developers.meta.com/horizon/documentation/unity/audio-ovrlipsync-viseme-reference/) (200) |

### Table 3 — Azure Speech viseme IDs

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| Heading “22 + silence” | Count of IDs | FIX | IDs are **0–21**; 0 is silence. **22 IDs including silence**, not 22 + silence. Summary line “22 IDs” is already correct. 100-ns ticks, 60 FPS, 55 positions, locale caveat: VERIFIED on the same page. | [Microsoft Learn](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/how-to-speech-synthesis-viseme) (200) |
| IDs 0–21 (11 table rows) | IPA sets as listed | VERIFIED | 0 silence; 1 æ ə ʌ; 2 ɑ; 3 ɔ; 4 ɛ ʊ; 5 ɝ; 6 j i ɪ; 7 w u; 8 o; 9 aʊ; 10 ɔɪ; 11 aɪ; 12 h; 13 ɹ; 14 l; 15 s z; 16 ʃ tʃ dʒ ʒ; 17 ð; 18 f v; 19 d t n θ; 20 k g ŋ; 21 p b m. | same |

### Table 4 — Amazon Polly en-US

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| p, t, S, T, f, k, i, l, r, s, u, @, a, e, E, o, O (17) | IPA phonemes mapped to each code | VERIFIED | Inverting the AWS en-US table yields exactly these 17 groups (including k ← ɡ, h, k, ŋ; u ← w, u, ʊ; E ← ɝ, ɛ, ʌ; a ← æ, aɪ, aʊ, ɑ). Summary “~16” is loose. | [AWS Polly en-US](https://docs.aws.amazon.com/polly/latest/dg/ph-table-english-us.html) (200) |

### Table 5 — Cappelletta & Harte 2012, Tables 1, 5, 6, pp. 322–329

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| 5a Jeffers / Neti / Hazen / Bozkurt / Lee (5) | Phonemes, visemes, vowel visemes, silence excluded | VERIFIED | Table 6: 43/11/4, 42/12/4, 52/14/5, 45/15/7, 39/13/7. Caption says silence viseme and phonemes are not taken into consideration. | [SciTePress PDF](https://www.scitepress.org/Papers/2012/37319/37319.pdf) (200); DOI 200 |
| 5b /A–/S (12) | Rank, occurrence %, TIMIT phonemes | VERIFIED | Table 1 matches, including /D occurrence “.70” written 0.70, /S rank/occurrence “–”, and **/ao/ under both /G (with /oy/) and /I**. B4 already flags that clash. | same |
| 5c Lee & Yook compact list + many-to-many note (1) | 14 classes as written; 5 vowels assigned to the first viseme | VERIFIED | Matches Table 5 (P, T, K, CH, F, W, IY, EH, AA, AH, AO, UH, ER, S). Surrounding text: they “claim this is a many-to-one map, [but] some phonemes are mapped into 2 visemes… associated with the first viseme proposed. This affects 5 vowel phonemes.” /ah/ appears under both AA and AH in the printed table, which is that ambiguity. | same |

### Table 6 — Timing

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| Time-to-voice general, 100–300 ms; English and French corpora; X-ray microbeam | consistently 100–300 ms | VERIFIED | Abstract and Discussion. The paper uses English material, a French spontaneous-speech database, and the Wisconsin (American English) X-ray microbeam corpus. | [PLOS / Europe PMC XML](https://doi.org/10.1371/journal.pcbi.1000436); PMC2700967 |
| Word-initial /p m b f/ means ± SD and n (PROBLEM, MAJOR, BUT, FORM) | 195 ± 60 (n=32); 137 ± 64 (n=33); 205 ± 69 (n=31); 240 ± 38 (n=27) ms | VERIFIED | Results, Fig. 8F text, Wisconsin X-ray microbeam, mean range 137–240 ms. | same |
| VCV /p m b f/ (APA, AMA, ABA, AFA) | 127 ± 21 (n=20); 129 ± 19 (n=22); 128 ± 20 (n=20); 188 ± 29 (n=18) ms | VERIFIED | Results, Fig. 9B text (source typo “Figures 9B”). Range 127 to 188 ms. | same |
| Mouth-area and voice envelope 2–7 Hz | both modulated at 2–7 Hz | VERIFIED | Abstract (and “between 2 and 7 Hz” in the introduction). | same |
| CV asynchrony pa, ta, ka, ba, da, ga, ma, na; “French speakers, lips tracked every 20 ms” | 20 ms audio lead to 70 ms audio lag | FIX | **Correction: one French speaker**, 6 repetitions of 8 syllables, isolated /Ca/ or sequenced /aCa/. Figures and 20 ms lip sampling match the abstract and Methods. Sex/age of that speaker not stated there. | [PLOS / Europe PMC XML](https://doi.org/10.1371/journal.pcbi.1003743); PMC4117430 |
| Expected range for more complex speech | 40 ms audio lead to 200 ms audio lag; “modelled from the above” | VERIFIED | Abstract: more complex material “should result in a range typically varying between 40 ms audio lead and 200 ms audio lag.” | same |

## Source table

| Source | URL tried | HTTP / copy used | Licence as checked 2026-11-16 | Honest in B4? |
|---|---|---|---|---|
| Ostermann et al. 1998 | isca-archive.org ICSLP PDF | 200, local `/tmp/b4verify/ostermann.pdf` | © ISCA/authors; facts cited. ISO/IEC 14496-2 not read. | Yes |
| CMU 18-796 slides (Chen 1999) | course.ece.cmu.edu/~ece796/mpeg4.pdf | 200 | © author; cross-check only | Yes |
| Meta Oculus Lipsync viseme reference | developers.meta.com/horizon/…/audio-ovrlipsync-viseme-reference/ | 200 | © Meta; groupings as facts; images unused | Yes (EOL wording slightly broad; see notes) |
| Azure Speech viseme page | learn.microsoft.com/…/how-to-speech-synthesis-viseme | 200 | © Microsoft; B4 says no licence notice was checked — treat as facts | Yes |
| AWS Polly en-US table | docs.aws.amazon.com/polly/latest/dg/ph-table-english-us.html | 200 | © Amazon; facts cited | Yes |
| Cappelletta & Harte 2012 | scitepress.org/Papers/2012/37319/37319.pdf ; DOI 10.5220/0003731903220329 | 200 | PDF: “Copyright c SciTePress”; facts cited | Yes |
| Chandrasekaran et al. 2009 | DOI 10.1371/journal.pcbi.1000436 ; PMC2700967 | PLOS DOI can fail; Europe PMC XML + PLOS PDF used | **CC BY 4.0** | Yes |
| Schwartz & Savariaux 2014 | DOI 10.1371/journal.pcbi.1003743 ; PMC4117430 | same pattern | **CC BY 4.0** | Yes |
| Preston Blair / Disney charts | — | unused | copyrighted drawings | Yes, unused |
| Fisher 1968 | — | not accessed | — | Yes, Not found |

No used source is wrongly marked as free.
