# B4 — Mouth shapes for speech (visemes) and phoneme-to-viseme maps

> **Verify fixes applied 2026-11-16:** (1) Ostermann Section 2.2 blends two facial expressions (joy and sadness), not two visemes (lines 44–45). (2) Azure heading is now 22 IDs (0–21), including silence as ID 0, not “22 + silence” (line 73). (3) Schwartz & Savariaux used one French speaker (6 repetitions of 8 CV syllables) (line 161). (4) Polly has 17 viseme codes, not ~16 (Summary, line 13). No other rows changed.

Prepared 2026-11-16. All links accessed 2026-11-16. Only phoneme-to-viseme **groupings** and timing facts are
recorded. No mouth drawings, reference images or SVGs from any source are copied. Phoneme symbols are given
as each source prints them (IPA, X-SAMPA-like SAMPA, ARPAbet/TIMIT, or informal).

## Summary

- Published sets range from 11 to 22 classes. The MPEG-4 set (14 visemes + neutral) is the common core and the
  basis of Meta's 15-viseme Oculus Lipsync set. Microsoft Azure uses 22 IDs (vowels split finely); Amazon
  Polly uses 17 letter-coded visemes.
- Every map agrees on three consonant groups: **p b m** (lips closed), **f v** (lower lip to upper teeth), and
  **ch j sh zh** (rounded, protruded). Most also separate **th/dh**, **s z**, **t d n (l)**, **k g (ng)** and **r**.
- Timing: lips start moving **100–300 ms before voice onset** at the start of an utterance (Chandrasekaran
  2009), but in running speech audio and visual events are close to synchronous (**−20 to +70 ms** for CV
  syllables; up to ~200 ms audio lag for complex material) (Schwartz & Savariaux 2014). Lead the sound with
  the mouth at phrase onsets; stay roughly in sync mid-phrase.

## Table 1 — MPEG-4 Facial Animation visemes (ISO/IEC 14496-2), as tabulated by Ostermann et al. 1998, Table 1

The standard itself is a paid ISO document and was not read; this table is from the MPEG-4 authors' open
conference paper (and matches the CMU course slides listed in Sources).

| # | Phonemes (as printed) | Example words (as printed) | Source | URL |
|---|---|---|---|---|
| 0 | none (neutral) | — | CMU 18-796 slides, "Visemes for Lip Synch" | [slides PDF](https://course.ece.cmu.edu/~ece796/mpeg4.pdf) |
| 1 | p, b, m | put, bed, mill | Ostermann et al. 1998, Table 1 | [ISCA PDF](https://www.isca-archive.org/icslp_1998/ostermann98_icslp.pdf) |
| 2 | f, v | far, voice | same | same |
| 3 | T, D | think, that | same | same |
| 4 | t, d | tip, doll | same | same |
| 5 | k, g | call, gas | same | same |
| 6 | tS, dZ, S | chair, join, she | same | same |
| 7 | s, z | sir, zeal | same | same |
| 8 | n, l | lot, not | same | same |
| 9 | r | red | same | same |
| 10 | A: | car | same | same |
| 11 | e | bed | same | same |
| 12 | I | tip | same | same |
| 13 | Q | top | same | same |
| 14 | U | book | same | same |

The same paper states only 14 clearly distinguished static visemes are in the standard set. Section 2.2
blends two facial expressions (joy and sadness) by a weight, not two visemes. It also defines the MPEG-4
neutral face (used in B5):
gaze along Z, muscles relaxed, **eyelids tangent to the iris**, pupil one third of the iris diameter unit,
lips in contact, teeth touching.

## Table 2 — Meta / Oculus Lipsync (15 visemes)

Source: [Oculus Lipsync Viseme Reference](https://developers.meta.com/horizon/documentation/unity/audio-ovrlipsync-viseme-reference/)
(Meta developer docs; marked end-of-life). The page says the 15 visemes follow the MPEG-4 standard.

| Viseme name | Phonemes (as listed) |
|---|---|
| sil | neutral |
| PP | p, b, m |
| FF | f, v |
| TH | th |
| DD | t, d |
| kk | k, g |
| CH | tS, dZ, S |
| SS | s, z |
| nn | n, l |
| RR | r |
| aa | A: |
| E | e |
| I | ih |
| O | oh |
| U | ou |

## Table 3 — Microsoft Azure Speech viseme IDs (22 IDs, 0–21, including silence as ID 0), en-US typical IPA

Source: [Microsoft Learn, "Get facial position with viseme"](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/how-to-speech-synthesis-viseme),
table "Map phonemes to visemes". Azure states visemes vary by locale; this is the listed typical IPA. The
same page notes viseme events carry an audio offset in 100-ns ticks and that blend-shape output runs at 60
frames per second with 55 facial positions per frame.

| ID | IPA | ID | IPA |
|---|---|---|---|
| 0 | silence | 11 | aɪ |
| 1 | æ, ə, ʌ | 12 | h |
| 2 | ɑ | 13 | ɹ |
| 3 | ɔ | 14 | l |
| 4 | ɛ, ʊ | 15 | s, z |
| 5 | ɝ | 16 | ʃ, tʃ, dʒ, ʒ |
| 6 | j, i, ɪ | 17 | ð |
| 7 | w, u | 18 | f, v |
| 8 | o | 19 | d, t, n, θ |
| 9 | aʊ | 20 | k, g, ŋ |
| 10 | ɔɪ | 21 | p, b, m |

## Table 4 — Amazon Polly en-US phoneme → viseme

Source: [AWS docs, "English (US) (en-US)" phoneme/viseme table](https://docs.aws.amazon.com/polly/latest/dg/ph-table-english-us.html).

| Viseme code | IPA phonemes mapped to it |
|---|---|
| p | b, m, p |
| t | d, n, t |
| S | d͡ʒ, ʃ, t͡ʃ, ʒ |
| T | ð, θ |
| f | f, v |
| k | ɡ, h, k, ŋ |
| i | j, i, ɪ |
| l | l |
| r | ɹ |
| s | s, z |
| u | w, u, ʊ |
| @ | ə, ɚ |
| a | æ, aɪ, aʊ, ɑ |
| e | eɪ |
| E | ɝ, ɛ, ʌ |
| o | oʊ |
| O | ɔ, ɔɪ |

## Table 5 — Research phoneme-to-viseme maps (Cappelletta & Harte 2012, Tables 1, 5, 6)

**5a. Map sizes (Table 6; silence excluded).**

| Map | Phonemes clustered | Visemes | Vowel visemes |
|---|---|---|---|
| Jeffers & Barley 1971 | 43 | 11 | 4 |
| Neti et al. 2000 | 42 | 12 | 4 |
| Hazen et al. 2004 | 52 | 14 | 5 |
| Bozkurt et al. 2007 | 45 | 15 | 7 |
| Lee & Yook 2002 | 39 | 13 | 7 |

**5b. Jeffers & Barley map (TIMIT symbols), with visibility rank and share of spoken English (Table 1).**

| Viseme | Visibility rank | Occurrence % | Phonemes |
|---|---|---|---|
| /A | 1 | 3.15 | f v |
| /B | 2 | 15.49 | er ow r q w uh uw axr ux |
| /C | 3 | 5.88 | b p m em |
| /D | 4 | 0.70 | aw |
| /E | 5 | 2.90 | dh th |
| /F | 6 | 1.20 | ch jh sh zh |
| /G | 7 | 1.81 | oy ao |
| /H | 8 | 4.36 | s z |
| /I | 9 | 31.46 | aa ae ah ay eh ey ih iy y ao ax-h ax ix |
| /J | 10 | 21.10 | d l n t el nx en dx |
| /K | 11 | 4.84 | g k ng eng |
| /S | – | – | sil |

**5c. Lee & Yook 2002 map (Table 5).** P: b p m · T: d t s z th dh · K: g k n ng l y hh · CH: jh ch sh zh ·
F: f v · W: r w · IY: iy ih · EH: eh ey ae · AA: aa aw ay ah · AH: ah · AO: ao oy ow · UH: uh uw · ER: er ·
S: sil. (Cappelletta & Harte note Lee's map is many-to-many for 5 vowels; they assign each to its first viseme.)

Source for 5a–5c: [Cappelletta & Harte 2012, ICPRAM, pp. 322–329, PDF](https://www.scitepress.org/Papers/2012/37319/37319.pdf); DOI https://doi.org/10.5220/0003731903220329.

## Table 6 — Timing of mouth movement relative to sound

| Measure | Population / material | Value | Source, location | URL |
|---|---|---|---|---|
| Mouth movement onset before voice onset ("time-to-voice"), general | English and French corpora; X-ray microbeam lip data | consistently 100–300 ms | Chandrasekaran et al. 2009, abstract & Discussion | [DOI](https://doi.org/10.1371/journal.pcbi.1000436) |
| Word-initial consonant (PROBLEM, MAJOR, BUT, FORM) | Wisconsin X-ray microbeam database, mean ± SD | /p/ 195 ± 60 ms (n = 32); /m/ 137 ± 64 (n = 33); /b/ 205 ± 69 (n = 31); /f/ 240 ± 38 (n = 27) | same, Results (Fig. 8F text) | same |
| VCV context (APA, AMA, ABA, AFA) | same | /p/ 127 ± 21 ms (n = 20); /m/ 129 ± 19 (n = 22); /b/ 128 ± 20 (n = 20); /f/ 188 ± 29 (n = 18) | same, Results (Fig. 9B text) | same |
| Mouth-area and voice envelope modulation | same corpora | both modulated at 2–7 Hz (syllable rate) | same, abstract | same |
| Audiovisual asynchrony for plosive-vowel syllables (pa, ta, ka, ba, da, ga, ma, na) | one French speaker (6 repetitions of 8 CV syllables), lips tracked every 20 ms | from 20 ms audio lead to 70 ms audio lag | Schwartz & Savariaux 2014, abstract | [DOI](https://doi.org/10.1371/journal.pcbi.1003743) |
| Expected range for more complex speech | modelled from the above | 40 ms audio lead to 200 ms audio lag | same, abstract | same |

## Licence

- Ostermann et al. 1998 (ICSLP, ISCA Archive) — © ISCA/authors; grouping facts cited. MPEG-4 (ISO/IEC 14496-2)
  itself — © ISO, paid; not read.
- CMU 18-796 slides (T. Chen, 1999) — course material, © author; used only to cross-check the MPEG-4 table.
- Meta Oculus Lipsync docs — © Meta; phoneme groupings cited as facts; reference images **not** used.
- Microsoft Learn docs — © Microsoft; no licence notice was checked on the page — treat the ID-to-phoneme
  mapping as cited facts.
- AWS Polly docs — © Amazon; groupings cited as facts.
- Cappelletta & Harte 2012 — "Copyright © SciTePress" on the PDF; facts cited.
- Chandrasekaran et al. 2009 and Schwartz & Savariaux 2014 (PLOS Comput Biol) — **CC BY 4.0**.
- Preston Blair / Disney mouth charts — copyrighted drawings; **nothing used**.

## Verification note

- Confirmed on source by me: Ostermann Table 1 and neutral-face text (ISCA PDF); CMU slides table; Meta page
  table (names and phonemes); Azure table; Polly table; Cappelletta Tables 1, 4, 5, 6; Chandrasekaran values
  (Europe PMC XML); Schwartz & Savariaux abstract (Europe PMC XML).
- grok CLI could not run (not signed in); nothing here comes from grok.
- Links checked by fetch/curl on 2026-11-16: all returned content except the PLOS DOI pages, which were read
  through Europe PMC (PMC2700967, PMC4117430).

## Not found

- **Fisher 1968** consonant viseme groups (J Speech Hear Res) — not accessed; not in Cappelletta's tables.
- **Preston Blair / "Disney 12" phoneme groupings** from a verifiable primary source — not found; only
  copyrighted drawings and fan summaries exist. Not used.
- **Measured lip aperture, lip width and protrusion (mm) per viseme** for adults — no open table found.
- **Per-viseme minimum hold durations / coarticulation weights** from a measured study — not found this pass.
- **ISO/IEC 14496-2 text itself** — paid; not read.

## Sources disagree

- **Visual lead:** Chandrasekaran 2009 reports a consistent 100–300 ms mouth lead; Schwartz & Savariaux 2014
  argue this holds only at utterance onsets/isolated syllables and that running speech is near-synchronous
  (−20 to +70 ms; up to 200 ms audio lag). Use lead only at phrase starts.
- **Number of visemes:** 11 (Jeffers) to 22 (Azure). Vowel handling differs most (4 to 15 vowel classes).
- **h:** Polly maps h to the k viseme; Azure gives h its own ID (12); MPEG-4 has no h class.
- **l:** MPEG-4 / Oculus group l with n; Azure gives l its own ID (14); Lee groups l with k g ng; Jeffers with
  d t n.
- **w:** Azure and Polly put w with u (rounded); Lee pairs r w; Jeffers puts w with r and rounded vowels.
- **s z vs t d:** Lee merges s z th dh with t d; all other maps keep s z separate.
- **Jeffers table as printed** lists /ao/ in both /G and /I — an error or ambiguity in the reproduced table.

## Sources (all accessed 2026-11-16)

1. Ostermann J, Beutnagel M, Fischer A, Wang Y. Integration of talking heads and text-to-speech synthesizers
   for visual TTS. ICSLP 98, Sydney, 1998. https://www.isca-archive.org/icslp_1998/ostermann98_icslp.pdf . © ISCA/authors.
2. Chen T. MPEG-4 (course slides, 18-796, Spring 1999, Carnegie Mellon University).
   https://course.ece.cmu.edu/~ece796/mpeg4.pdf . © author.
3. Meta. Oculus Lipsync — Viseme Reference (Unity docs, end-of-life).
   https://developers.meta.com/horizon/documentation/unity/audio-ovrlipsync-viseme-reference/ . © Meta.
4. Microsoft. Get facial position with viseme (Azure AI Speech).
   https://learn.microsoft.com/en-us/azure/ai-services/speech-service/how-to-speech-synthesis-viseme . © Microsoft.
5. Amazon Web Services. Amazon Polly Developer Guide — English (US) (en-US) phoneme/viseme table.
   https://docs.aws.amazon.com/polly/latest/dg/ph-table-english-us.html . © Amazon.
6. Cappelletta L, Harte N. Phoneme-to-viseme mapping for visual speech recognition. Proc. ICPRAM 2012,
   pp. 322–329. https://doi.org/10.5220/0003731903220329 ; PDF https://www.scitepress.org/Papers/2012/37319/37319.pdf .
   © SciTePress.
7. Chandrasekaran C, Trubanova A, Stillittano S, Caplier A, Ghazanfar AA. The natural statistics of audiovisual
   speech. *PLoS Comput Biol* 2009;5(7):e1000436. https://doi.org/10.1371/journal.pcbi.1000436 ;
   https://pmc.ncbi.nlm.nih.gov/articles/PMC2700967/ . CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/).
8. Schwartz JL, Savariaux C. No, there is no 150 ms lead of visual speech on auditory speech, but a range of
   audiovisual asynchronies varying from small audio lead to large audio lag. *PLoS Comput Biol*
   2014;10(7):e1003743. https://doi.org/10.1371/journal.pcbi.1003743 ;
   https://pmc.ncbi.nlm.nih.gov/articles/PMC4117430/ . CC BY 4.0.
