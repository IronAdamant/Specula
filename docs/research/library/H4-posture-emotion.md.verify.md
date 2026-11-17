# H4 verification: posture and movement cues for emotion

Verifier: Library Verify (adversarial re-check). Written 2026-11-17, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It checks `H4-posture-emotion.md`. H4 itself was not edited. All URLs below were accessed on **2026-11-17**.

## Version checked
- Snapshot at the start of the check: mtime **2026-11-16 10:46:17 AEDT**.
- Motion Data edited H4 at **2026-11-17 08:56:02 AEDT** (20,342 bytes). The diff and number-diff show that **only the access-date column and separators were added**. No values changed, so nothing was re-checked because of that edit.
- Re-stat before writing: **2026-11-17 08:56:02 AEDT**, byte-identical to my post-edit copy. **This is the final mtime verified against.**

## Summary

| Status | Rows | Meaning |
|---|---:|---|
| **VERIFIED** | **27** | Number and population match a table or sentence I opened. Roether was read as JOV full text (WebFetch), the open-access papers as Europe PMC XML, and the closed papers as abstracts. |
| **FIX** | **2** | R7 (Alaerts n), D5 (Roether spread label) |
| **UNVERIFIABLE** | **1** | R10's "55 raters" |
| **Total** | **30** | D1–D12, G1–G7 plus the η² note, R1–R10 |

**Worst problems**
1. **R7 Alaerts 2011: the sample size is wrong.** H4 says "about 36 observers (Z(35))".
   - Experiment 2 recruited 37 people (15 male, 22 female). Emotion data were lost for 2 males and 3 females.
   - The supplement table is marked "***Sample of 32 participants".
   - **Correction:** "n = 32 observers (emotion task)".
   - The percentages 54.3 / 44.2 / 45.8 / 58.6 (% correct above chance) are right.
2. **D5 Roether 2009: the spread for anger 1.82 ± 0.22 m/s and fear 0.83 ± 0.31 m/s is labelled two ways in the same paper.**
   - Experiment 1 results say *"mean ± SEM for anger: 1.82 ± 0.22 m/s; for fear: 0.83 ± 0.31 m/s"*.
   - The later feature paragraph gives the same pair as *"mean ± SD gait velocity 1.82 ± 0.22 m/s for anger; 1.31 ± 0.36 m/s for happiness"*.
   - With 25 actors, SEM and SD differ by a factor of 5, so a generator that samples from this spread could be off by that factor.
   - **Correction:** keep the means, mark the spread as "± 0.22 / 0.31 (labelled SEM in one place and SD in another)", and add this to Sources disagree.
   - Also note: the velocities come from **Experiment 1 (movement analysis)**, not "Exp 2 results", and the paper says "25 individuals" (lay actors).
3. **R10 Volkova 2014.** The 19%, 1,447 sequences and 8 actors (3 of the 11 recruited were excluded) are confirmed. The **"55 raters"** figure does not appear in the full text I read: the classification study's observer count is not stated there. It may come from a companion paper. It is UNVERIFIABLE until a source is cited.

**Dead links:** none. The JOV DOI redirect gives 403/500 to curl, but the JOV article page opens through the fetch tool, as H4 notes.

## Row table

| Row | Claim | Status | Note / correction | URL checked |
|---|---|---|---|---|
| D1 | Sad: head −18.9 deg (largest change) | VERIFIED | "−18.9 deg for the head" (Exp 3 methods) | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 |
| D2 | Sad: elbow −16.6 deg; weights shoulder −0.67, elbow −0.79 | VERIFIED | Exact | same |
| D3 | Happy: elbow +2.5, head +6.3; weights +0.42 / +0.61 | VERIFIED | Exact | same |
| D4 | Angry and fearful: more elbow flexion, one of the two most prominent findings | VERIFIED | "The most prominent findings were the strongly reduced head angle… for sad walking, and increases of the elbow angles for fearful and angry walking." | same |
| D5 | Anger 1.82±0.22, fear 0.83±0.31 m/s (SEM); 25 lay actors; "Exp 2 results" | **FIX** | Means right. The spread is labelled SEM in one place and SD in another, and the figures come from Experiment 1. | same |
| D6 | Michalak directions; 14 inpatients; 23 students | VERIFIED | Abstract | https://europepmc.org/abstract/MED/19414617 |
| D7 | Gross: joy and anger fastest, sad slowest; neck and thoracic flexion with sadness; trunk extension and shoulder depression with joy; n=16 | VERIFIED | Abstract | https://europepmc.org/abstract/MED/21835480 |
| D8–D11 | Halovic & Kroos cue descriptions | VERIFIED | Abstract | https://europepmc.org/abstract/MED/29174557 |
| D12 | Poyo Solanas 61% (35/62/63/95) and 84% (79/83/92/83) | VERIFIED | Results | https://pmc.ncbi.nlm.nih.gov/articles/PMC7148324/ |
| G1–G7 | Homagain Table 5 (all cells); Kendall W 0.25 / 0.31; G6 p=0.67; G7 p=0.09–0.56; n=23 (16F/7M), 24 (20–32) y | VERIFIED | Table 5 and Methods | https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ |
| G-note | η² 0.64 (Halovic, quoted) and 0.395 (Park, quoted) | VERIFIED | As quoted in Homagain. H4 correctly marks these as second-hand. | same |
| R1 | Roether Table 1: 78% overall; 70.3/75.1/77.1/89.8; N=20 | VERIFIED | Table 1 "(N = 20 subjects)". 21 people were tested in Exp 2, and the table uses 20. | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 |
| R2 | Confusions 23.2 / 15.6 / 17.9 / 8.0 | VERIFIED | Responses in rows and intended emotion in columns; each column sums to 100 | same |
| R3 | Speed-matched neutral 48.8 / 42.6 / 28.1 / 54.1; fear-speed read as sad 61.2; N=20 | VERIFIED | Table 2 "(N = 20 subjects)" | same |
| R4 | Fear: female actors >90, male 60.5; sadness: male 93, female 87.3 | VERIFIED | Results (χ²=15.15) | same |
| R5 | BEAST 93.6 / 93.9 / 85.4 / 97.8; 46 actors, 254 stimuli, 19 observers, 4 s | VERIFIED | Table 1 and Methods | https://pmc.ncbi.nlm.nih.gov/articles/PMC3152787/ |
| R6 | BEAST confusions 5.7 / 5.6 / 3.3 | VERIFIED | Table 1 | same |
| R7 | Alaerts 54.3 / 44.2 / 45.8 / 58.6; "about 36 observers" | **FIX** | **n = 32** (37 recruited, 5 with emotion data lost) | https://pmc.ncbi.nlm.nih.gov/articles/PMC3111458/ |
| R8 | Ross 94.3 / 81.1; teens 90 / 68.7, n=15; adults n=14 | VERIFIED | Results 2.2 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3438163/ |
| R9 | Coulson 176 postures, 3 views, 6 emotions, 0 to >90%, happiness and surprise close | VERIFIED | Abstract: "A total of 176 computer-generated mannequin figures… ranged from zero for many disgust postures to over 90 percent for some anger and sadness postures… happiness and surprise sharing a similar position" | https://link.springer.com/article/10.1023/B:JONB.0000023655.25550.be |
| R10 | Volkova 19%; 8 actors; 1,447 clips; 11 categories; **55 raters** | UNVERIFIABLE | 19% confirmed ("The observed agreement between the intended and the perceived emotion categories is 19%"). 8 actors confirmed. The rater count is not in the text. | https://pmc.ncbi.nlm.nih.gov/articles/PMC4252031/ |

## Not found
- **Volkova 2014 rater count** (see R10).
- H4's existing "Not found" list was re-read and is accurate. Roether's angry and fear joint-angle magnitudes really are only in Fig. 2B.

## Sources disagree (additions)
- **Roether 2009 speed spread:** labelled ± SEM (Exp 1 results) and ± SD (feature-analysis paragraph) for the same 1.82 ± 0.22 m/s.
- **Alaerts n:** H4 inferred n from the test statistic Z(35). The paper's Methods and supplement give 32 analysed observers.

## Source table

| Key | URL | Resolves (2026-11-17) | Licence correct |
|---|---|---|---|
| 1 Roether 2009 | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | y (WebFetch; curl 403) | y (no CC statement in the fetched page text; H4 says © ARVO, free to read) |
| 2 Michalak 2009 | https://europepmc.org/abstract/MED/19414617 | y | y (closed) |
| 3 Gross 2012 | https://europepmc.org/abstract/MED/21835480 | y | y (closed) |
| 4 Halovic & Kroos 2018 | https://europepmc.org/abstract/MED/29174557 | y | y (closed) |
| 5 Poyo Solanas 2020 | https://pmc.ncbi.nlm.nih.gov/articles/PMC7148324/ | y | y (CC BY 4.0) |
| 6 Homagain 2023 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ | y | y (CC BY 4.0) |
| 7 BEAST 2011 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3152787/ | y | y (Frontiers open-access licence) |
| 8 Alaerts 2011 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3111458/ | y | y (CC BY 4.0) |
| 9 Ross 2012 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3438163/ | y | y (CC BY) |
| 10 Coulson 2004 | https://link.springer.com/article/10.1023/B:JONB.0000023655.25550.be | y | y (closed; abstract) |
| 11 Volkova 2014 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4252031/ | y | y (CC BY 4.0) |

## Grok pass
- **"Grok B"** (shared with H5 and H2), 2026-11-17 08:49:22–09:08:49 AEDT, exit 0.
- **It agreed on** the Roether Table 1 diagonals, the anger and fear speeds, and Homagain Table 5 velocity.
- **It independently flagged** the Roether SEM/SD label conflict, which I confirmed in the JOV text.
- No Grok-only number is used here.
