# H1 — Human walking and running: timing, joint angles, stride vs speed, transition, arm swing, age/sex/load

**Changelog**
- 2026-11-17 (AEDT): per-row access dates added. Every table row now ends with a "Source URL · accessed" cell (or "Accessed" in the source list), and every numeric prose line ends with its source URL and access date. Rows checked on 2026-11-16 carry 2026-11-16; rows re-checked today carry 2026-11-17.
- 2026-11-17: Library Verify FIXes applied after re-confirming in the primary source (OSF t72cw workbooks opened today; Novacheck PDF opened today): §1 fast all-adults n 244 → **245**; §3a trial count 731 → **732** (731 only where height is needed; subject HAC119 has no height); all eleven §2a headers Women ≥70 n=19 → **n=18** (19 is the fast-speed count); sprint toe-off now shows **36 % (text p. 79) and 37 % (Fig. 6 caption)**, with a new "Sources disagree" row. No angle or timing value changed.
- 2026-11-17 09:22 AEDT (second Library Verify pass, verify file 09:15): §3a reason for 731 vs 732 reworded — HAC119 is not missing a height; it is filed as HAC119A (1.83 m) in `01_Demo_PhysEx.xlsx` (confirmed in the OSF t72cw workbooks already opened today). Fits were not refitted (the original fitting script is not in my scratch area); coefficients unchanged. The earlier changelog line's "HAC119 has no height" is superseded.


Researched 2026-11-16 for the M6 motion libraries (brief: `LIBRARY_RESEARCH_BRIEF.md`, item H1). All links accessed **2026-11-16**.
Every number below was read from the source named in its row (paper text/table, or computed by me from the named open dataset files, read in `/tmp` and deleted afterwards). Nothing was downloaded into any repo. Book figures and wording were not copied.

**Method note.** The planned grok CLI research passes could not run: `~/.grok/bin/grok` exits with "Not signed in" on this box. All research was done with direct web search and by reading the primary sources (PubMed, Europe PMC full text, publisher pages, OSF dataset files). Figshare (both Fukuchi datasets and the Schreiber & Moissenet dataset) returns HTTP 403 to this box, so I could not open those dataset pages or files myself (see "Not found").

**Link status key** used in the source list: `OK` = opened from this box and the number was read there; `OK (fetch)` = opened through the web-fetch tool (plain curl from the box is bot-blocked with 403, but the page loads); `403` = blocked from this box, not verified by me.

---

## 1. Stride timing (walking): stance, swing, double support

Main source: Senden et al. 2024, *Data in Brief* 53:110230, dataset on OSF (`osf.io/t72cw`). 246 healthy adults (122 men, 124 women, 18–91 years). Treadmill in a virtual-reality setup, at comfortable speed, at comfortable −30 % ("slow") and at comfortable +30 % ("fast"); 250 steps recorded per condition. [https://osf.io/t72cw/ · accessed 2026-11-16]
**How I computed it:** for each subject, from `0x_Overview_{slow,comf,fast}.xlsx` sheet `Gait_*`: stance % = left stance time ÷ left stride time × 100; total double support % = (left + right "Bipedal" time) ÷ left stride time × 100 ("Bipedal" is defined in the file `28_Description_parameters` as one double-support period). Swing % = 100 − stance %. Mean ± SD across subjects, with [min, max] of the subjects. [https://osf.io/t72cw/ · accessed 2026-11-16]

| Condition | Group | n | Speed m/s (mean ± SD) | Stance % of cycle | Total double support % | Source | Source URL · accessed |
|---|---|---|---|---|---|---|---|
| slow (−30 %) | all adults 18–91 | 242 | see §3 | 62.3 ± 1.7 [57.9, 71.0] | 24.7 ± 3.1 [17.6, 40.3] | Senden 2024, OSF `04_Overview_slow.xlsx` (computed) | https://osf.io/t72cw/ · 2026-11-16 |
| comfortable | all adults 18–91 | 245 | see §3 | 60.7 ± 1.5 [57.4, 67.4] | 21.2 ± 2.7 [14.7, 34.1] | Senden 2024, OSF `02_Overview_comf.xlsx` (computed) | https://osf.io/t72cw/ · 2026-11-17 |
| fast (+30 %) | all adults 18–91 | 245 | see §3 | 59.4 ± 1.5 [55.9, 66.2] | 18.6 ± 2.7 [12.2, 31.2] | Senden 2024, OSF `03_Overview_fast.xlsx` (computed) | https://osf.io/t72cw/ · 2026-11-17 |
| comfortable | men 18–29 | 21 | 1.30 ± 0.16 | 60.7 ± 1.3 [57.6, 63.4] | 21.2 ± 2.3 [15.2, 24.9] | as above | https://osf.io/t72cw/ · 2026-11-16 |
| comfortable | women 18–29 | 26 | 1.22 ± 0.16 | 60.1 ± 1.2 [58.2, 63.0] | 20.2 ± 2.3 [16.0, 25.3] | as above | https://osf.io/t72cw/ · 2026-11-16 |
| comfortable | men ≥70 | 21 | 1.19 ± 0.20 | 61.0 ± 1.5 [57.6, 64.3] | 22.1 ± 3.0 [15.7, 28.5] | as above | https://osf.io/t72cw/ · 2026-11-16 |
| comfortable | women ≥70 | 18 | 1.09 ± 0.22 | 61.6 ± 2.3 [58.1, 67.4] | 23.2 ± 4.4 [16.3, 34.1] | as above | https://osf.io/t72cw/ · 2026-11-16 |
| slow | men 18–29 / women 18–29 | 21 / 26 | 0.90 ± 0.12 / 0.86 ± 0.12 | 62.6 ± 1.5 / 62.0 ± 1.5 | 25.0 ± 2.7 / 24.2 ± 3.0 | as above | https://osf.io/t72cw/ · 2026-11-16 |
| fast | men 18–29 / women 18–29 | 21 / 25 | 1.68 ± 0.21 / 1.59 ± 0.21 | 59.5 ± 1.2 / 58.9 ± 1.1 | 18.6 ± 2.3 / 18.1 ± 1.9 | as above | https://osf.io/t72cw/ · 2026-11-16 |
| slow / fast | men ≥70 | 21 / 21 | 0.84 ± 0.13 / 1.54 ± 0.26 | 62.4 ± 1.9 / 59.9 ± 1.5 | 25.0 ± 3.6 / 19.7 ± 3.0 | as above | https://osf.io/t72cw/ · 2026-11-16 |
| slow / fast | women ≥70 | 18 / 19 | 0.77 ± 0.15 / 1.42 ± 0.26 | 63.1 ± 2.7 / 59.9 ± 2.4 | 26.2 ± 5.2 / 20.2 ± 4.6 | as above | https://osf.io/t72cw/ · 2026-11-16 |

Absolute times at comfortable speed (group table `05_AgeGenderGroup_comf.xlsx`, sheet `Gait_comf`, mean ± SD), left leg: stride time men 18–29 1.09 ± 0.08 s, women 18–29 1.08 ± 0.06 s, men ≥70 1.08 ± 0.07 s, women ≥70 1.06 ± 0.10 s; swing time 0.43 ± 0.03 / 0.43 ± 0.02 / 0.42 ± 0.02 / 0.41 ± 0.03 s; one double-support period ("Bipedal") 0.117 ± 0.017 / 0.108 ± 0.016 / 0.120 ± 0.023 / 0.122 ± 0.029 s. [https://osf.io/t72cw/ · accessed 2026-11-16]

### Running: toe-off timing

| Gait | Speed | Toe-off at % of cycle (= stance %) | Population | Source | Source URL · accessed |
|---|---|---|---|---|---|
| walk | 1.2 m/s | 62 | Gillette Children's lab subjects (n not stated in the passage) | Novacheck 1998, *Gait Posture* 7:77–95, text p. 79 and Fig. 6 caption | https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf · 2026-11-16 |
| run | 3.2 m/s | 39 | same | Novacheck 1998, same place | https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf · 2026-11-16 |
| sprint | 3.9 m/s | 36 (body text p. 79); 37 (Fig. 6 caption) | same; the caption says the three slower conditions are averages of 15 strides | Novacheck 1998, text p. 79 gives 36 %, Fig. 6 caption gives 37 % (see Sources disagree) | https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf · 2026-11-17 |
| elite sprint | 9.0 m/s | 22 | elite sprinters, data adapted from Mann & Hagy (cited by Novacheck) | Novacheck 1998, Fig. 6 caption | https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf · 2026-11-16 |

No SD is given for these. In running there is no double support; instead there are two "double float" phases (both feet off the ground), one at the start and one at the end of swing (Novacheck 1998, p. 79). [https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf · accessed 2026-11-16]

## 2. Joint angle curves through the walking cycle

### 2a. By age and sex, comfortable speed (primary dataset, group means ± between-subject SD)

Source: Senden et al. 2024, OSF file `05_AgeGenderGroup_comf.xlsx` (one sheet per angle, 100 points from 1 % to 100 %; right leg; HBM-II lower-limb model with trunk markers). Values are copied from the dataset at every 5 % (row 1 = the first sample). Initial contact (heel strike) is at 1 %/100 %. The sign conventions are the ones printed in each sheet's header cell. n per group is from `01_Demo_PhysEx.xlsx`. Mean ages: men 18–29 23.9 y, women 18–29 23.1 y, men ≥70 74.1 y (70–81), women ≥70 74.0 y (70–91). Mean heights: 1.820 / 1.697 / 1.767 / 1.604 m. Mean leg lengths: 0.953 / 0.894 / 0.944 / 0.857 m. [https://osf.io/t72cw/ · accessed 2026-11-16]

#### Hip flexion (+) / extension (−) (degrees; sheet `Rotation_RHipFlex_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | 27.5 ± 6.2 | 26.8 ± 5.7 | 32.2 ± 7.5 | 29.9 ± 7.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | 27.3 ± 6.5 | 26.8 ± 5.9 | 31.9 ± 7.7 | 29.3 ± 8.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | 25.9 ± 6.9 | 25.6 ± 5.9 | 30.4 ± 7.7 | 27.7 ± 8.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | 21.4 ± 6.8 | 20.8 ± 5.5 | 25.9 ± 7.6 | 22.7 ± 7.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | 15.2 ± 6.5 | 14.7 ± 5.2 | 20.3 ± 7.7 | 17.1 ± 7.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | 8.7 ± 6.1 | 8.4 ± 4.9 | 14.2 ± 7.8 | 11.1 ± 7.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 2.3 ± 5.5 | 2.3 ± 4.7 | 8.0 ± 7.8 | 4.8 ± 7.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | -3.6 ± 5.4 | -3.4 ± 4.6 | 2.4 ± 8.0 | -0.7 ± 7.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | -8.4 ± 5.6 | -8.3 ± 4.8 | -2.1 ± 8.5 | -5.5 ± 7.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | -11.9 ± 5.7 | -12.0 ± 4.9 | -5.7 ± 8.9 | -9.3 ± 8.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | -13.9 ± 5.7 | -14.3 ± 4.9 | -8.5 ± 8.8 | -11.9 ± 8.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | -12.4 ± 5.8 | -13.1 ± 4.8 | -7.7 ± 8.3 | -11.2 ± 8.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | -5.5 ± 6.1 | -6.6 ± 4.7 | -1.5 ± 7.9 | -5.2 ± 8.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | 5.2 ± 6.2 | 3.8 ± 4.8 | 8.4 ± 7.3 | 5.4 ± 9.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | 15.5 ± 6.2 | 14.5 ± 5.1 | 18.1 ± 6.7 | 16.3 ± 8.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | 23.4 ± 6.0 | 23.2 ± 5.2 | 25.8 ± 6.1 | 24.9 ± 7.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | 28.2 ± 5.6 | 28.5 ± 5.1 | 30.8 ± 5.8 | 30.3 ± 7.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | 29.7 ± 5.3 | 30.2 ± 4.8 | 33.1 ± 5.8 | 32.4 ± 7.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | 28.7 ± 5.3 | 28.9 ± 4.8 | 32.9 ± 6.4 | 31.6 ± 7.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | 27.7 ± 5.8 | 27.3 ± 5.5 | 32.5 ± 7.1 | 30.6 ± 7.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | 27.5 ± 6.2 | 26.8 ± 5.7 | 32.2 ± 7.5 | 29.9 ± 7.3 | https://osf.io/t72cw/ · 2026-11-16 |

#### Knee flexion (+) / extension (−) (degrees; sheet `Rotation_RKneeFlex_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | 4.7 ± 4.2 | 5.4 ± 3.9 | 10.7 ± 5.1 | 8.3 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | 10.9 ± 4.9 | 11.5 ± 4.6 | 16.3 ± 4.8 | 13.4 ± 4.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | 17.6 ± 5.4 | 17.7 ± 5.2 | 22.7 ± 4.9 | 19.5 ± 4.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | 18.1 ± 5.5 | 17.4 ± 5.0 | 22.8 ± 5.2 | 18.6 ± 4.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | 15.0 ± 5.1 | 14.2 ± 4.4 | 20.0 ± 5.1 | 15.3 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | 10.4 ± 4.2 | 10.0 ± 3.6 | 15.6 ± 5.2 | 11.3 ± 3.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 5.9 ± 3.4 | 6.0 ± 3.1 | 10.8 ± 5.7 | 6.9 ± 4.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 2.4 ± 3.1 | 3.1 ± 2.8 | 7.0 ± 6.3 | 3.6 ± 4.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | 0.9 ± 3.3 | 2.0 ± 2.7 | 5.3 ± 6.5 | 1.9 ± 4.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | 2.4 ± 3.7 | 3.7 ± 3.0 | 6.1 ± 6.2 | 2.7 ± 4.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | 7.9 ± 4.1 | 8.8 ± 3.5 | 10.1 ± 5.2 | 6.7 ± 4.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | 18.7 ± 5.1 | 18.9 ± 4.7 | 19.8 ± 4.7 | 15.9 ± 7.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | 35.8 ± 5.6 | 35.4 ± 5.6 | 36.4 ± 5.4 | 32.2 ± 9.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | 53.9 ± 4.9 | 53.1 ± 4.7 | 54.0 ± 4.9 | 51.1 ± 8.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | 64.0 ± 3.6 | 63.8 ± 3.4 | 63.3 ± 4.1 | 62.5 ± 5.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | 63.5 ± 3.1 | 64.0 ± 3.1 | 62.1 ± 4.7 | 62.6 ± 5.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | 53.8 ± 3.6 | 54.3 ± 3.3 | 52.3 ± 5.5 | 53.0 ± 6.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | 36.5 ± 3.9 | 36.8 ± 3.1 | 36.1 ± 6.0 | 36.1 ± 7.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | 15.0 ± 3.6 | 15.2 ± 2.6 | 17.3 ± 6.8 | 16.5 ± 6.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | 1.8 ± 3.3 | 2.3 ± 3.3 | 7.2 ± 6.5 | 5.6 ± 5.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | 4.7 ± 4.2 | 5.4 ± 3.9 | 10.7 ± 5.1 | 8.3 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |

#### Ankle dorsiflexion (+) / plantarflexion (−) (degrees; sheet `Rotation_RAnkleFlex_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | 0.8 ± 2.6 | 0.4 ± 1.8 | 4.1 ± 2.4 | -0.5 ± 2.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | -3.2 ± 2.9 | -3.1 ± 1.9 | 0.5 ± 2.8 | -3.5 ± 3.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | -2.3 ± 3.2 | -1.6 ± 2.1 | 2.0 ± 2.9 | -1.6 ± 3.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | 1.9 ± 3.3 | 2.4 ± 2.3 | 5.6 ± 3.1 | 1.8 ± 3.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | 4.6 ± 3.1 | 5.1 ± 2.2 | 8.0 ± 3.0 | 4.5 ± 3.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | 5.7 ± 2.7 | 6.3 ± 2.1 | 9.1 ± 2.8 | 6.1 ± 3.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 6.6 ± 2.3 | 7.3 ± 2.0 | 9.8 ± 2.8 | 7.3 ± 2.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 7.8 ± 2.4 | 8.8 ± 2.1 | 10.8 ± 2.9 | 8.7 ± 3.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | 9.1 ± 2.8 | 10.4 ± 2.3 | 12.3 ± 3.2 | 10.3 ± 3.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | 10.1 ± 3.1 | 11.6 ± 2.5 | 13.5 ± 3.6 | 11.5 ± 3.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | 9.2 ± 3.3 | 10.6 ± 2.4 | 13.1 ± 4.1 | 11.3 ± 2.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | 3.0 ± 4.7 | 4.1 ± 3.3 | 8.4 ± 5.7 | 7.2 ± 4.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | -10.0 ± 5.7 | -9.4 ± 4.4 | -2.7 ± 8.0 | -2.8 ± 7.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | -17.1 ± 5.1 | -17.5 ± 4.2 | -8.2 ± 5.6 | -9.4 ± 6.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | -11.0 ± 4.9 | -11.8 ± 4.0 | -4.5 ± 4.4 | -6.9 ± 5.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | -3.9 ± 3.6 | -4.2 ± 2.6 | 0.4 ± 3.7 | -2.1 ± 4.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | 0.1 ± 2.8 | 0.1 ± 2.1 | 3.5 ± 3.1 | 0.6 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | 1.3 ± 2.3 | 1.1 ± 1.9 | 4.3 ± 2.7 | 0.7 ± 3.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | 1.2 ± 2.7 | 0.9 ± 1.7 | 4.4 ± 2.4 | 0.3 ± 2.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | 1.7 ± 2.8 | 1.5 ± 1.8 | 5.0 ± 2.5 | 0.6 ± 2.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | 0.8 ± 2.6 | 0.4 ± 1.8 | 4.1 ± 2.4 | -0.4 ± 2.7 | https://osf.io/t72cw/ · 2026-11-16 |

#### Pelvic tilt, anterior (+) / posterior (−) (degrees; sheet `Rotation_PelvicTil_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | 5.8 ± 4.6 | 4.9 ± 4.3 | 7.7 ± 5.5 | 6.4 ± 6.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | 5.7 ± 4.7 | 5.1 ± 4.2 | 7.5 ± 5.6 | 6.4 ± 6.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | 5.5 ± 4.6 | 5.4 ± 4.0 | 7.2 ± 5.5 | 6.1 ± 6.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | 5.4 ± 4.4 | 5.5 ± 4.0 | 7.1 ± 5.3 | 6.1 ± 6.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | 5.5 ± 4.3 | 5.6 ± 4.1 | 7.2 ± 5.5 | 6.3 ± 5.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | 5.6 ± 4.4 | 5.8 ± 4.1 | 7.3 ± 5.5 | 6.4 ± 5.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 5.6 ± 4.4 | 5.8 ± 4.0 | 7.4 ± 5.5 | 6.4 ± 5.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 5.5 ± 4.5 | 5.6 ± 3.9 | 7.7 ± 5.4 | 6.5 ± 5.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | 5.8 ± 4.6 | 5.6 ± 4.0 | 8.1 ± 5.5 | 6.7 ± 5.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | 6.0 ± 4.7 | 5.5 ± 4.2 | 8.2 ± 5.8 | 6.6 ± 5.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | 5.7 ± 4.8 | 5.2 ± 4.2 | 7.8 ± 6.0 | 6.4 ± 5.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | 5.7 ± 4.7 | 5.3 ± 4.2 | 7.7 ± 5.9 | 6.2 ± 5.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | 5.7 ± 4.5 | 5.4 ± 4.2 | 7.4 ± 5.8 | 5.8 ± 5.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | 5.7 ± 4.3 | 5.4 ± 4.4 | 7.3 ± 5.6 | 5.9 ± 5.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | 5.7 ± 4.3 | 5.4 ± 4.6 | 7.4 ± 5.5 | 6.2 ± 5.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | 5.6 ± 4.4 | 5.5 ± 4.6 | 7.4 ± 5.3 | 6.3 ± 5.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | 5.5 ± 4.3 | 5.5 ± 4.5 | 7.5 ± 5.2 | 6.3 ± 5.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | 5.5 ± 4.3 | 5.3 ± 4.4 | 7.8 ± 5.1 | 6.5 ± 5.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | 5.7 ± 4.4 | 5.2 ± 4.3 | 8.1 ± 5.2 | 6.7 ± 5.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | 6.0 ± 4.6 | 5.2 ± 4.3 | 8.2 ± 5.4 | 6.7 ± 5.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | 5.8 ± 4.6 | 4.9 ± 4.3 | 7.7 ± 5.5 | 6.4 ± 6.2 | https://osf.io/t72cw/ · 2026-11-16 |

#### Pelvic obliquity, up (+) / down (−) (degrees; sheet `Rotation_PelvicObl_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | -0.5 ± 2.2 | 0.4 ± 2.0 | -0.2 ± 2.2 | -0.9 ± 3.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | 1.5 ± 2.5 | 3.0 ± 2.0 | 1.0 ± 2.4 | 0.4 ± 2.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | 3.7 ± 2.8 | 6.1 ± 2.2 | 2.1 ± 2.5 | 2.1 ± 2.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | 4.7 ± 2.7 | 7.8 ± 2.3 | 2.9 ± 2.7 | 3.3 ± 2.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | 3.9 ± 2.3 | 7.0 ± 2.2 | 2.4 ± 2.6 | 2.8 ± 2.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | 2.1 ± 2.0 | 4.7 ± 2.0 | 1.3 ± 2.4 | 1.7 ± 1.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 0.7 ± 1.7 | 2.6 ± 1.8 | 0.6 ± 2.4 | 0.6 ± 1.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 0.2 ± 1.7 | 1.6 ± 1.8 | 0.6 ± 2.3 | 0.2 ± 1.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | 0.7 ± 2.0 | 1.7 ± 1.8 | 0.9 ± 2.4 | 0.3 ± 1.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | 1.1 ± 2.0 | 1.9 ± 1.7 | 1.0 ± 2.5 | 0.4 ± 2.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | 0.2 ± 1.8 | 0.6 ± 1.6 | 0.6 ± 2.7 | 0.1 ± 2.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | -2.3 ± 1.8 | -2.6 ± 1.9 | -1.0 ± 2.8 | -1.5 ± 2.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | -4.6 ± 1.8 | -5.5 ± 2.3 | -2.6 ± 2.9 | -3.2 ± 2.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | -5.6 ± 1.8 | -6.9 ± 2.4 | -3.6 ± 2.6 | -4.4 ± 2.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | -4.6 ± 1.6 | -5.9 ± 2.3 | -2.9 ± 2.1 | -3.8 ± 2.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | -2.6 ± 1.4 | -3.6 ± 2.1 | -1.7 ± 1.7 | -2.6 ± 2.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | -1.1 ± 1.4 | -1.7 ± 1.8 | -0.8 ± 1.5 | -1.6 ± 2.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | -0.7 ± 1.6 | -0.9 ± 1.7 | -0.6 ± 1.6 | -1.2 ± 2.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | -1.3 ± 1.9 | -1.0 ± 1.8 | -0.7 ± 1.8 | -1.2 ± 2.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | -1.5 ± 2.1 | -1.1 ± 2.0 | -0.7 ± 2.0 | -1.3 ± 3.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | -0.5 ± 2.2 | 0.4 ± 2.0 | -0.2 ± 2.2 | -0.9 ± 3.1 | https://osf.io/t72cw/ · 2026-11-16 |

#### Pelvic rotation, internal (+) / external (−) (degrees; sheet `Rotation_PelvicRot_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | 1.9 ± 2.6 | 3.9 ± 2.9 | 1.2 ± 3.2 | 3.2 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | 1.4 ± 2.6 | 3.0 ± 2.6 | 1.2 ± 3.3 | 3.0 ± 4.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | 0.4 ± 2.5 | 1.4 ± 2.3 | 0.8 ± 3.3 | 2.1 ± 3.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | 1.0 ± 2.8 | 1.5 ± 2.2 | 0.8 ± 3.1 | 2.0 ± 3.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | 2.1 ± 3.0 | 2.7 ± 2.1 | 1.2 ± 2.8 | 2.9 ± 3.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | 2.3 ± 2.8 | 3.0 ± 1.9 | 1.2 ± 2.7 | 3.4 ± 3.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 1.5 ± 2.6 | 2.0 ± 1.8 | 0.8 ± 2.8 | 2.7 ± 3.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 0.5 ± 2.5 | 0.4 ± 1.9 | 0.2 ± 2.9 | 1.8 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | -0.5 ± 2.4 | -1.2 ± 2.2 | -0.5 ± 3.1 | 0.8 ± 3.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | -1.3 ± 2.2 | -2.5 ± 2.7 | -1.1 ± 3.2 | 0.1 ± 3.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | -2.0 ± 2.1 | -3.3 ± 2.8 | -1.7 ± 3.1 | -0.5 ± 4.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | -1.3 ± 2.2 | -2.4 ± 2.5 | -1.6 ± 3.0 | -0.3 ± 4.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | -0.4 ± 2.4 | -1.1 ± 2.3 | -1.0 ± 2.7 | 0.4 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | -1.0 ± 2.7 | -1.4 ± 2.4 | -0.9 ± 2.4 | 0.2 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | -1.9 ± 2.6 | -2.5 ± 2.5 | -1.3 ± 2.5 | -0.6 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | -1.9 ± 2.5 | -2.6 ± 2.6 | -1.3 ± 2.6 | -0.9 ± 3.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | -1.1 ± 2.4 | -1.5 ± 2.6 | -0.9 ± 2.5 | -0.3 ± 3.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | -0.2 ± 2.4 | 0.1 ± 2.6 | -0.3 ± 2.6 | 0.8 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | 0.7 ± 2.4 | 1.7 ± 2.7 | 0.3 ± 2.9 | 1.8 ± 4.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | 1.4 ± 2.5 | 3.2 ± 2.9 | 0.7 ± 3.1 | 2.6 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | 1.9 ± 2.6 | 3.9 ± 2.9 | 1.2 ± 3.2 | 3.2 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |

#### Hip adduction (+) / abduction (−) (degrees; sheet `Rotation_RHipAbAd_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | 0.1 ± 3.1 | 0.8 ± 2.4 | 1.3 ± 3.0 | 1.9 ± 4.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | 3.0 ± 3.4 | 4.8 ± 2.2 | 3.2 ± 3.1 | 4.3 ± 4.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | 6.3 ± 3.8 | 9.5 ± 2.3 | 5.1 ± 3.2 | 7.2 ± 3.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | 8.1 ± 3.7 | 12.2 ± 2.6 | 6.8 ± 3.1 | 9.4 ± 3.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | 7.6 ± 3.4 | 11.7 ± 2.7 | 6.8 ± 3.2 | 9.5 ± 3.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | 6.3 ± 2.9 | 9.7 ± 2.7 | 6.2 ± 3.2 | 8.8 ± 2.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 5.0 ± 2.5 | 7.6 ± 2.7 | 5.8 ± 3.3 | 8.1 ± 2.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 4.4 ± 2.5 | 6.3 ± 2.8 | 5.9 ± 3.3 | 7.7 ± 2.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | 4.7 ± 2.7 | 5.9 ± 2.9 | 6.0 ± 3.4 | 7.6 ± 3.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | 4.5 ± 2.8 | 5.4 ± 3.0 | 5.6 ± 3.7 | 7.4 ± 3.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | 2.7 ± 2.6 | 3.2 ± 3.3 | 4.2 ± 4.1 | 6.3 ± 3.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | -0.9 ± 2.7 | -0.6 ± 3.6 | 1.3 ± 4.1 | 3.8 ± 3.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | -5.0 ± 2.6 | -4.5 ± 3.7 | -2.0 ± 4.1 | 0.5 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | -7.8 ± 2.5 | -7.0 ± 3.3 | -4.3 ± 3.6 | -2.2 ± 4.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | -7.0 ± 2.4 | -6.0 ± 2.8 | -3.8 ± 2.9 | -2.2 ± 3.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | -4.5 ± 2.6 | -3.1 ± 2.5 | -2.3 ± 2.5 | -0.7 ± 3.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | -2.7 ± 2.9 | -1.0 ± 2.4 | -1.1 ± 2.5 | 0.6 ± 3.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | -2.0 ± 2.8 | -0.4 ± 2.4 | -0.7 ± 2.7 | 0.9 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | -2.0 ± 2.7 | -1.0 ± 2.4 | -0.6 ± 2.9 | 0.7 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | -1.8 ± 2.9 | -1.4 ± 2.5 | -0.0 ± 3.1 | 0.7 ± 4.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | 0.1 ± 3.1 | 0.8 ± 2.3 | 1.3 ± 3.0 | 1.9 ± 4.3 | https://osf.io/t72cw/ · 2026-11-16 |

#### Hip rotation, internal (+) / external (−) (degrees; sheet `Rotation_RHipRot_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | -6.1 ± 6.0 | -0.8 ± 5.0 | -8.8 ± 5.6 | -5.4 ± 5.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | -2.8 ± 5.5 | 1.6 ± 4.8 | -6.3 ± 5.2 | -2.6 ± 5.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | -0.6 ± 5.3 | 3.2 ± 4.9 | -5.3 ± 4.9 | -1.0 ± 4.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | -0.6 ± 5.2 | 3.6 ± 4.9 | -5.2 ± 5.0 | -1.0 ± 5.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | -0.3 ± 5.8 | 3.9 ± 4.5 | -4.7 ± 5.1 | -1.4 ± 5.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | 0.9 ± 6.5 | 6.1 ± 4.5 | -3.6 ± 5.4 | -0.5 ± 5.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 1.6 ± 6.7 | 8.4 ± 4.5 | -2.4 ± 5.9 | 1.1 ± 6.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 1.8 ± 6.9 | 9.6 ± 4.6 | -1.7 ± 6.1 | 2.1 ± 6.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | 1.9 ± 7.2 | 10.2 ± 4.8 | -1.6 ± 6.2 | 2.7 ± 7.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | 1.7 ± 7.6 | 10.0 ± 4.9 | -1.8 ± 6.2 | 2.9 ± 7.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | 1.1 ± 7.2 | 8.7 ± 4.6 | -2.2 ± 5.7 | 2.4 ± 7.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | -1.8 ± 6.5 | 4.9 ± 4.9 | -3.6 ± 5.3 | 0.0 ± 6.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | -5.0 ± 6.2 | 0.6 ± 5.1 | -4.9 ± 5.2 | -3.0 ± 6.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | -4.5 ± 5.9 | -0.2 ± 5.1 | -4.0 ± 5.0 | -4.0 ± 6.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | -3.5 ± 5.2 | 0.3 ± 5.2 | -3.2 ± 5.4 | -3.6 ± 5.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | -4.2 ± 4.6 | -0.3 ± 5.1 | -3.5 ± 5.6 | -3.4 ± 5.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | -5.8 ± 4.6 | -1.9 ± 5.0 | -4.3 ± 5.6 | -3.8 ± 5.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | -6.7 ± 4.9 | -2.4 ± 5.0 | -5.2 ± 5.7 | -3.9 ± 6.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | -6.5 ± 5.4 | -1.6 ± 4.7 | -6.9 ± 6.0 | -4.6 ± 6.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | -7.6 ± 5.8 | -2.1 ± 4.7 | -9.3 ± 5.9 | -6.1 ± 6.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | -6.1 ± 6.0 | -0.8 ± 5.0 | -8.8 ± 5.6 | -5.3 ± 5.7 | https://osf.io/t72cw/ · 2026-11-16 |

#### Trunk tilt, anterior (+) / posterior (−) (degrees; sheet `Rotation_TrunkTilt_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | 0.4 ± 4.2 | -1.1 ± 6.5 | -0.1 ± 8.3 | -5.9 ± 9.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | 0.0 ± 4.1 | -2.1 ± 6.4 | -0.4 ± 8.4 | -6.2 ± 9.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | -0.6 ± 4.1 | -3.8 ± 6.4 | -0.6 ± 8.3 | -6.6 ± 9.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | -0.8 ± 3.8 | -4.5 ± 6.3 | -0.6 ± 8.2 | -6.8 ± 9.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | -0.5 ± 4.0 | -3.9 ± 6.4 | -0.5 ± 8.3 | -6.8 ± 9.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | -0.3 ± 4.4 | -3.4 ± 6.4 | -0.4 ± 8.2 | -6.4 ± 8.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 0.1 ± 4.3 | -2.9 ± 6.3 | -0.3 ± 8.0 | -6.0 ± 8.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 0.6 ± 4.2 | -2.3 ± 6.2 | -0.4 ± 8.1 | -5.9 ± 8.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | 0.6 ± 4.4 | -1.8 ± 6.2 | -0.5 ± 8.3 | -5.7 ± 8.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | 0.6 ± 4.6 | -1.4 ± 6.3 | -0.5 ± 8.6 | -5.5 ± 8.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | 0.6 ± 4.6 | -1.4 ± 6.2 | -0.4 ± 8.8 | -5.7 ± 8.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | -0.1 ± 4.4 | -2.6 ± 6.3 | -0.8 ± 8.6 | -6.1 ± 8.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | -0.9 ± 4.2 | -4.0 ± 6.3 | -0.9 ± 8.3 | -6.5 ± 9.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | -1.0 ± 4.0 | -4.3 ± 6.5 | -0.8 ± 8.2 | -6.7 ± 9.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | -0.6 ± 4.1 | -3.7 ± 6.8 | -0.6 ± 8.3 | -6.7 ± 8.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | -0.3 ± 4.3 | -3.2 ± 6.8 | -0.4 ± 8.2 | -6.3 ± 8.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | 0.2 ± 4.1 | -2.6 ± 6.6 | -0.2 ± 8.1 | -6.1 ± 8.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | 0.6 ± 4.1 | -1.8 ± 6.5 | -0.2 ± 8.1 | -6.0 ± 8.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | 0.6 ± 4.2 | -1.3 ± 6.5 | -0.2 ± 8.2 | -5.9 ± 9.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | 0.5 ± 4.3 | -1.0 ± 6.5 | -0.1 ± 8.3 | -5.7 ± 9.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | 0.5 ± 4.2 | -1.1 ± 6.5 | -0.1 ± 8.3 | -5.9 ± 9.6 | https://osf.io/t72cw/ · 2026-11-16 |

#### Trunk lateral flexion, ipsilateral (+) / contralateral (−) (degrees; sheet `Rotation_TrunkFlex_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | 0.3 ± 2.9 | 0.9 ± 2.6 | -0.3 ± 2.7 | -0.1 ± 3.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | 3.1 ± 3.1 | 4.3 ± 2.6 | 1.4 ± 2.6 | 1.8 ± 3.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | 6.1 ± 3.4 | 8.0 ± 2.6 | 2.9 ± 2.6 | 3.9 ± 3.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | 7.5 ± 3.3 | 10.0 ± 2.8 | 3.7 ± 2.8 | 5.1 ± 3.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | 6.2 ± 3.0 | 8.9 ± 2.8 | 2.8 ± 2.6 | 4.4 ± 3.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | 3.7 ± 2.8 | 6.2 ± 2.7 | 1.3 ± 2.4 | 3.0 ± 3.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | 1.6 ± 2.7 | 3.6 ± 2.6 | 0.3 ± 2.4 | 1.8 ± 2.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 0.7 ± 2.7 | 2.6 ± 2.5 | 0.0 ± 2.6 | 1.4 ± 2.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | 1.1 ± 2.8 | 2.9 ± 2.5 | 0.3 ± 2.7 | 1.5 ± 2.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | 1.3 ± 2.8 | 3.1 ± 2.3 | 0.3 ± 2.9 | 1.7 ± 3.3 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | -0.1 ± 2.5 | 1.4 ± 2.4 | -0.5 ± 3.2 | 1.1 ± 3.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | -3.7 ± 2.6 | -2.6 ± 2.8 | -2.8 ± 3.5 | -1.1 ± 3.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | -6.8 ± 2.9 | -6.1 ± 3.5 | -4.8 ± 3.6 | -3.2 ± 3.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | -8.2 ± 2.9 | -7.7 ± 3.6 | -5.8 ± 3.5 | -4.4 ± 3.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | -6.7 ± 2.7 | -6.5 ± 3.3 | -4.7 ± 3.1 | -3.6 ± 2.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | -4.0 ± 2.3 | -3.8 ± 2.9 | -3.0 ± 2.7 | -2.1 ± 2.4 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | -1.9 ± 2.3 | -1.5 ± 2.4 | -1.7 ± 2.5 | -1.1 ± 2.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | -1.2 ± 2.5 | -0.6 ± 2.3 | -1.2 ± 2.5 | -0.7 ± 2.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | -1.5 ± 2.8 | -0.9 ± 2.5 | -1.2 ± 2.6 | -0.7 ± 3.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | -1.5 ± 2.9 | -1.1 ± 2.6 | -1.1 ± 2.6 | -0.8 ± 3.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | 0.2 ± 2.8 | 0.9 ± 2.6 | -0.3 ± 2.7 | -0.1 ± 3.5 | https://osf.io/t72cw/ · 2026-11-16 |

#### Trunk rotation, internal (+) / external (−) (degrees; sheet `Rotation_TrunkRot_comf`)

| cycle % | Men 18–29 (n=21) | Women 18–29 (n=26) | Men ≥70 (n=21) | Women ≥70 (n=18) | Source URL · accessed |
|---|---|---|---|---|---|
| 1 | -3.6 ± 3.5 | -3.3 ± 3.8 | -1.1 ± 3.3 | -1.4 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 5 | -4.8 ± 3.4 | -4.5 ± 4.0 | -1.8 ± 3.4 | -1.8 ± 3.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 10 | -5.3 ± 3.2 | -5.8 ± 4.3 | -2.0 ± 3.4 | -2.1 ± 3.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 15 | -5.2 ± 3.1 | -6.2 ± 4.3 | -1.6 ± 3.5 | -2.1 ± 3.6 | https://osf.io/t72cw/ · 2026-11-16 |
| 20 | -4.0 ± 3.1 | -4.6 ± 4.0 | -0.6 ± 3.5 | -1.2 ± 3.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 25 | -2.3 ± 2.8 | -1.9 ± 3.7 | 0.4 ± 3.5 | 0.4 ± 3.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 30 | -0.7 ± 2.7 | 0.4 ± 3.6 | 1.1 ± 3.4 | 2.2 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 35 | 0.3 ± 2.7 | 1.7 ± 3.6 | 1.5 ± 3.4 | 3.6 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 40 | 0.6 ± 2.7 | 2.2 ± 3.7 | 1.7 ± 3.5 | 4.2 ± 3.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 45 | 0.8 ± 2.9 | 2.5 ± 3.7 | 1.6 ± 3.5 | 4.4 ± 3.8 | https://osf.io/t72cw/ · 2026-11-16 |
| 50 | 1.8 ± 3.0 | 3.7 ± 3.6 | 2.1 ± 3.5 | 4.7 ± 3.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 55 | 3.1 ± 3.1 | 5.4 ± 3.7 | 3.0 ± 3.4 | 5.3 ± 4.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 60 | 3.6 ± 3.4 | 6.5 ± 3.9 | 3.1 ± 3.3 | 5.7 ± 4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| 65 | 3.5 ± 3.6 | 6.6 ± 4.2 | 2.7 ± 3.2 | 5.8 ± 3.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 70 | 2.2 ± 3.4 | 4.9 ± 4.3 | 1.8 ± 3.3 | 4.6 ± 3.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 75 | 0.4 ± 3.2 | 2.3 ± 4.2 | 0.8 ± 3.4 | 2.8 ± 3.5 | https://osf.io/t72cw/ · 2026-11-16 |
| 80 | -1.1 ± 3.2 | 0.1 ± 4.0 | -0.0 ± 3.4 | 1.0 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
| 85 | -1.9 ± 3.3 | -1.1 ± 3.8 | -0.6 ± 3.3 | -0.3 ± 3.9 | https://osf.io/t72cw/ · 2026-11-16 |
| 90 | -2.2 ± 3.4 | -1.7 ± 3.7 | -0.7 ± 3.3 | -1.0 ± 4.1 | https://osf.io/t72cw/ · 2026-11-16 |
| 95 | -2.4 ± 3.5 | -2.1 ± 3.7 | -0.5 ± 3.3 | -1.0 ± 4.0 | https://osf.io/t72cw/ · 2026-11-16 |
| 100 | -3.6 ± 3.5 | -3.3 ± 3.8 | -1.1 ± 3.3 | -1.4 ± 3.7 | https://osf.io/t72cw/ · 2026-11-16 |
### 2b. Key peaks by speed (Senden 2024 group files `07_…slow`, `05_…comf`, `06_…fast`; value of the group-mean curve at its peak, the SD at that point, and the cycle % where it happens)

| Variable | Group | slow | comfortable | fast | Source URL · accessed |
|---|---|---|---|---|---|
| Peak hip flexion (late swing) | men 18–29 | 27.6 (SD 5.1) at 85 % | 29.7 (5.3) at 85 % | 32.9 (6.7) at 7 % | https://osf.io/t72cw/ · 2026-11-16 |
| | women 18–29 | 28.5 (5.1) at 86 % | 30.2 (4.8) at 85 % | 32.7 (4.8) at 86 % | https://osf.io/t72cw/ · 2026-11-16 |
| | men ≥70 | 30.6 (6.3) at 87 % | 33.3 (6.0) at 87 % | 36.9 (7.8) at 6 % | https://osf.io/t72cw/ · 2026-11-16 |
| | women ≥70 | 30.4 (7.2) at 86 % | 32.4 (7.0) at 86 % | 35.9 (6.0) at 87 % | https://osf.io/t72cw/ · 2026-11-16 |
| Peak hip extension (late stance) | men 18–29 | −10.7 (5.1) at 53 % | −13.9 (5.7) at 51 % | −15.4 (5.3) at 50 % | https://osf.io/t72cw/ · 2026-11-16 |
| | women 18–29 | −11.7 (5.0) at 54 % | −14.4 (4.9) at 51 % | −17.0 (4.8) at 50 % | https://osf.io/t72cw/ · 2026-11-16 |
| | men ≥70 | −7.6 (8.8) at 53 % | −8.8 (8.6) at 52 % | −10.3 (8.2) at 51 % | https://osf.io/t72cw/ · 2026-11-16 |
| | women ≥70 | −10.1 (7.7) at 54 % | −12.2 (8.1) at 52 % | −13.3 (6.9) at 51 % | https://osf.io/t72cw/ · 2026-11-16 |
| Peak knee flexion (swing) | men 18–29 | 62.4 (4.6) at 73 % | 65.0 (3.3) at 72 % | 64.2 (3.2) at 71 % | https://osf.io/t72cw/ · 2026-11-16 |
| | women 18–29 | 63.2 (4.0) at 74 % | 65.2 (3.1) at 73 % | 64.3 (3.0) at 72 % | https://osf.io/t72cw/ · 2026-11-16 |
| | men ≥70 | 60.2 (4.2) at 73 % | 64.0 (4.2) at 72 % | 64.9 (3.9) at 71 % | https://osf.io/t72cw/ · 2026-11-16 |
| | women ≥70 | 61.3 (4.3) at 74 % | 63.9 (5.1) at 73 % | 64.9 (4.4) at 72 % | https://osf.io/t72cw/ · 2026-11-16 |
| Peak ankle plantarflexion (after toe-off) | men 18–29 | −11.6 (5.1) at 67 % | −17.1 (5.1) at 65 % | −18.5 (4.4) at 63 % | https://osf.io/t72cw/ · 2026-11-16 |
| | women 18–29 | −12.7 (4.0) at 67 % | −17.5 (4.2) at 65 % | −19.3 (3.6) at 63 % | https://osf.io/t72cw/ · 2026-11-16 |
| | men ≥70 | −5.8 (6.2) at 66 % | −8.2 (5.6) at 65 % | −10.8 (5.4) at 63 % | https://osf.io/t72cw/ · 2026-11-16 |
| | women ≥70 | −6.7 (6.6) at 67 % | −9.4 (6.2) at 66 % | −12.0 (6.6) at 64 % | https://osf.io/t72cw/ · 2026-11-16 |
| Peak ankle dorsiflexion (late stance) | men 18–29 | 11.2 (2.6) at 49 % | 10.2 (3.1) at 46 % | 8.0 (3.3) at 44 % | https://osf.io/t72cw/ · 2026-11-16 |
| | women 18–29 | 12.2 (2.4) at 49 % | 11.6 (2.4) at 46 % | 9.5 (2.9) at 44 % | https://osf.io/t72cw/ · 2026-11-16 |
| Pelvic obliquity, max up / max down | men 18–29 | 3.5 / −4.3 | 4.7 / −5.6 | 5.6 / −6.5 | https://osf.io/t72cw/ · 2026-11-16 |
| | women 18–29 | 6.8 / −6.2 | 7.9 / −6.9 | 8.9 / −8.0 | https://osf.io/t72cw/ · 2026-11-16 |
| | men ≥70 | 2.2 / −2.7 | 2.9 / −3.6 | 3.7 / −4.2 | https://osf.io/t72cw/ · 2026-11-16 |
| | women ≥70 | 2.3 / −3.6 | 3.3 / −4.4 | 3.9 / −5.3 | https://osf.io/t72cw/ · 2026-11-16 |
| Trunk rotation, max / min | men 18–29 | 3.3 / −4.3 | 3.6 / −5.3 | 4.9 / −6.4 | https://osf.io/t72cw/ · 2026-11-16 |
| | women 18–29 | 8.6 / −6.7 | 6.8 / −6.3 | 8.2 / −7.4 | https://osf.io/t72cw/ · 2026-11-16 |

(Fast-condition hip-flexion peaks at 6–7 % mean the curve is still at its plateau just after heel strike; the swing peak is about the same value.) [https://osf.io/t72cw/ · accessed 2026-11-16]
What this shows for a generator: at the same relative speed, older adults show less hip extension and much less push-off plantarflexion, and a smaller pelvic obliquity. Women show about 2–3° more pelvic obliquity than men at every speed. [https://osf.io/t72cw/ · accessed 2026-11-16]

### 2c. Second curve set: WBDS young adults, overground, comfortable speed (derived, secondary)

Primary data: Fukuchi, Fukuchi & Duarte 2018, *PeerJ* 6:e4640, WBDS dataset (figshare 10.6084/m9.figshare.5722711, CC BY 4.0 per the citing package). 24 young adults: 27.6 ± 4.4 y, 171.1 ± 10.5 cm, 68.4 ± 12.2 kg (paper, "Participants"), barefoot, Visual3D processing, 101-point curves. [https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · accessed 2026-11-16]
**Caveat:** figshare is blocked from this box, so these values come from a third-party derivative: the R package *PhysioGaitNorm* v0.2.1 (Matsui, MIT licence for the code; data marked CC-BY-4.0 "derived from WBDS"), file `inst/extdata/adult_reference_bands_101.csv`, generated 2026-08-23. It covers the right limb only. Its author mapped the sagittal axes directly but **matched the frontal and transverse axes to published curve shapes** (its MANIFEST says so). Treat the sagittal columns as reliable and the frontal and transverse columns as provisional until the raw WBDS files are checked. [https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · accessed 2026-11-16]


| cycle % | pelvic tilt | pelvic obliquity | pelvic rotation | hip flexion | hip adduction | hip rotation | knee flexion | ankle dorsiflexion | Source URL · accessed |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 11.1 ± 4.8 | 1.0 ± 1.8 | 6.3 ± 2.9 | 30.5 ± 6.3 | -1.2 ± 3.3 | -1.5 ± 5.4 | 0.1 ± 3.9 | 0.9 ± 2.7 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 5 | 11.0 ± 5.1 | 2.3 ± 1.9 | 6.4 ± 2.6 | 29.9 ± 6.7 | 0.7 ± 3.0 | -0.3 ± 4.7 | 6.2 ± 4.2 | -4.4 ± 2.8 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 10 | 10.5 ± 5.4 | 4.3 ± 1.8 | 4.7 ± 2.2 | 28.8 ± 7.2 | 4.0 ± 2.9 | 0.5 ± 4.6 | 13.5 ± 5.2 | -1.9 ± 3.2 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 15 | 10.0 ± 5.5 | 5.2 ± 2.0 | 3.9 ± 1.9 | 24.1 ± 7.1 | 6.2 ± 3.1 | 0.2 ± 4.4 | 14.3 ± 5.0 | 2.5 ± 3.3 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 20 | 10.1 ± 5.5 | 4.4 ± 1.7 | 4.0 ± 1.7 | 18.6 ± 7.1 | 6.1 ± 2.9 | -1.0 ± 4.5 | 12.0 ± 4.9 | 5.3 ± 3.1 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 25 | 10.2 ± 5.3 | 2.6 ± 1.7 | 3.1 ± 1.8 | 13.2 ± 6.8 | 5.2 ± 2.6 | -2.1 ± 4.5 | 9.1 ± 4.7 | 7.1 ± 2.9 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 30 | 10.1 ± 5.2 | 0.8 ± 1.6 | 1.2 ± 2.0 | 7.7 ± 6.6 | 4.2 ± 2.4 | -2.6 ± 4.6 | 6.2 ± 4.4 | 8.7 ± 2.8 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 35 | 10.2 ± 5.1 | -0.6 ± 1.7 | -0.7 ± 2.2 | 3.0 ± 6.7 | 3.2 ± 2.4 | -3.3 ± 4.5 | 4.3 ± 4.2 | 10.4 ± 3.0 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 40 | 10.6 ± 5.1 | -1.1 ± 1.8 | -2.6 ± 2.5 | -1.0 ± 6.9 | 2.7 ± 2.4 | -3.7 ± 4.5 | 3.5 ± 4.4 | 12.2 ± 3.2 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 45 | 10.9 ± 5.0 | -1.2 ± 1.9 | -4.6 ± 3.0 | -4.4 ± 6.7 | 2.3 ± 2.6 | -4.2 ± 4.6 | 4.5 ± 4.6 | 13.2 ± 3.4 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 50 | 10.9 ± 5.0 | -1.5 ± 2.1 | -6.2 ± 3.0 | -6.9 ± 6.8 | 1.1 ± 2.8 | -5.0 ± 4.5 | 8.3 ± 4.8 | 12.1 ± 3.7 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 55 | 10.6 ± 5.0 | -2.8 ± 2.1 | -6.4 ± 2.6 | -7.7 ± 6.9 | -1.2 ± 3.0 | -6.0 ± 4.5 | 15.8 ± 5.7 | 6.5 ± 4.7 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 60 | 10.1 ± 5.2 | -4.7 ± 2.3 | -5.2 ± 2.4 | -4.2 ± 7.3 | -4.2 ± 3.2 | -5.7 ± 4.6 | 30.0 ± 6.8 | -6.2 ± 5.2 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 65 | 9.6 ± 5.2 | -5.8 ± 2.6 | -4.3 ± 2.5 | 4.0 ± 7.1 | -6.4 ± 3.1 | -4.8 ± 4.5 | 47.9 ± 5.8 | -13.0 ± 6.0 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 70 | 9.5 ± 5.1 | -5.1 ± 2.5 | -4.5 ± 2.3 | 13.9 ± 6.8 | -5.5 ± 2.9 | -3.8 ± 4.1 | 59.9 ± 3.9 | -7.2 ± 7.5 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 75 | 9.7 ± 5.0 | -3.3 ± 2.3 | -3.8 ± 2.1 | 22.3 ± 6.3 | -3.1 ± 2.7 | -2.6 ± 4.5 | 61.7 ± 4.1 | 0.2 ± 5.8 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 80 | 9.8 ± 4.8 | -1.5 ± 2.1 | -1.8 ± 2.2 | 27.9 ± 6.0 | -1.4 ± 2.8 | -1.1 ± 4.7 | 54.0 ± 6.0 | 5.0 ± 4.2 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 85 | 10.2 ± 4.8 | -0.1 ± 1.8 | 0.4 ± 2.4 | 31.1 ± 5.9 | -0.5 ± 2.9 | -0.1 ± 4.9 | 38.8 ± 7.5 | 6.4 ± 2.9 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 90 | 10.8 ± 4.7 | 0.4 ± 1.7 | 2.4 ± 2.7 | 31.9 ± 5.8 | -0.5 ± 3.1 | -0.6 ± 5.2 | 19.4 ± 6.7 | 4.4 ± 1.9 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 95 | 11.3 ± 4.6 | 0.5 ± 1.6 | 4.3 ± 2.9 | 30.7 ± 5.6 | -0.9 ± 3.2 | -1.7 ± 5.4 | 2.4 ± 3.9 | 1.4 ± 2.5 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| 100 | 11.4 ± 4.7 | 0.9 ± 1.9 | 5.9 ± 2.7 | 30.2 ± 6.0 | -0.8 ± 3.3 | -1.5 ± 5.3 | -0.2 ± 4.1 | 1.1 ± 2.7 | https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
WBDS landmarks (same file): knee peak 62.2° at 73 %; hip range −7.7° (54 %) to 31.9° (89 %); ankle −13.0° (65 %) to 13.2° (45 %); pelvic tilt 9.5–11.4° (absolute anterior tilt, SD about 5° between subjects). [https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · accessed 2026-11-16]

### 2d. Running joint angles (peaks only; treadmill, shod)

Source: Fukuchi, Fukuchi & Duarte 2017, *PeerJ* 5:e3298, Table 4. n = 28 regular runners (>20 km/week). Demographics are only in the dataset metadata, which I could not open. Mean ± SD. [https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · accessed 2026-11-16]

| Variable | 2.5 m/s | 3.5 m/s | 4.5 m/s | Source URL · accessed |
|---|---|---|---|---|
| Stride length (m) | 1.86 ± 0.11 | 2.46 ± 0.15 | 2.96 ± 0.20 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |
| Cadence (strides/min) | 80.82 ± 4.63 | 85.68 ± 5.27 | 91.74 ± 6.69 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |
| Stride width (m) | 0.10 ± 0.02 | 0.09 ± 0.02 | 0.08 ± 0.02 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |
| Max hip flexion (°) | 43.75 ± 6.06 | 52.76 ± 5.75 | 60.50 ± 6.06 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |
| Max hip extension (°) | −3.58 ± 4.85 | −7.95 ± 4.58 | −11.75 ± 4.78 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |
| Max knee flexion (°) | 93.52 ± 10.36 | 108.68 ± 10.65 | 119.12 ± 10.37 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |
| Max ankle dorsiflexion (°) | 26.36 ± 2.93 | 26.54 ± 2.49 | 26.79 ± 2.51 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |
| Max ankle plantarflexion (°) | −16.62 ± 5.50 | −20.47 ± 4.71 | −23.17 ± 4.72 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |
| Max eversion (°) | −4.91 ± 2.74 | −6.59 ± 2.99 | −7.81 ± 3.59 | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |
| Foot strike: rearfoot / midfoot / forefoot | 68 / 14 / 18 % | 68 / 18 / 14 % | 61 / 21 / 18 % | https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ · 2026-11-16 |

(Foot-strike shares: same paper, Results, "Gait kinematics"; the shares were not changed by speed.)

## 3. Stride length and cadence against speed

### 3a. Regression from the Senden 2024 dataset (computed by me)
Data: 732 subject-trials (245 comfortable + 245 fast + 242 slow; 731 in the height- and leg-length-normalised fits: fast-trial subject **HAC119** is filed as **HAC119A** (height 1.83 m) in `01_Demo_PhysEx.xlsx`, so the ID join dropped that one trial — an ID-matching artefact, not missing data. The fits below were **not** refitted on n = 732; one extra trial out of 732 is not expected to move the coefficients at the printed precision, but this was not tested), speeds 0.5–2.3 m/s, adults 18–91 y. Fitted by least squares on per-subject means. Height and leg length are from `01_Demo_PhysEx.xlsx`. [https://osf.io/t72cw/ · accessed 2026-11-17]

| Relation | Fit | Fit quality | Source URL · accessed |
|---|---|---|---|
| Cadence (steps/min) vs speed v (m/s) | cadence = 38.8·v + 63.2 | r = 0.902; residual SD 6.7 steps/min | https://osf.io/t72cw/ · 2026-11-16 |
| Stride length ÷ body height vs v | L/H = 0.335·v + 0.344 | r = 0.939; residual SD 0.044 | https://osf.io/t72cw/ · 2026-11-16 |
| Power-law form | L/H = 0.681·v^0.549; cadence = 102.4·v^0.427 | residual SD of ln(L/H) 0.060 | https://osf.io/t72cw/ · 2026-11-16 |
| Froude number v²/(g·leg length) at comfortable speed | 0.178 ± 0.054 (all); 0.183 ± 0.049 men 18–29; 0.174 ± 0.046 women 18–29; 0.159 ± 0.056 men ≥70; 0.149 ± 0.064 women ≥70 | per-subject mean ± SD | https://osf.io/t72cw/ · 2026-11-16 |
| Stride length ÷ height | slow 0.637 ± 0.073; comfortable 0.760 ± 0.082; fast 0.884 ± 0.085 | all subjects | https://osf.io/t72cw/ · 2026-11-16 |

These are my own fits to the open data and are not published by the authors. Use them as generator rules, valid only inside 0.5–2.3 m/s on a treadmill. [https://osf.io/t72cw/ · accessed 2026-11-16]

### 3b. Senden 2024 group table, comfortable speed (`08_Table_Spatiotemporal_comf.pdf`; mean (SD))

| Age group | Sex | Speed m/s | Cadence steps/min | Stride length m (left) | Step width m (left) | Foot clearance cm (left) | Source URL · accessed |
|---|---|---|---|---|---|---|---|
| 18–29 | M | 1.30 (0.16) | 110.81 (7.57) | 1.40 (0.13) | 0.20 (0.04) | 1.98 (0.35) | https://osf.io/t72cw/ · 2026-11-16 |
| 18–29 | F | 1.22 (0.16) | 111.62 (6.01) | 1.31 (0.13) | 0.16 (0.03) | 1.88 (0.29) | https://osf.io/t72cw/ · 2026-11-16 |
| 30–39 | M | 1.30 (0.17) | 112.93 (7.18) | 1.38 (0.14) | 0.21 (0.05) | 2.11 (0.49) | https://osf.io/t72cw/ · 2026-11-16 |
| 30–39 | F | 1.25 (0.15) | 114.71 (5.84) | 1.31 (0.13) | 0.16 (0.04) | 1.79 (0.44) | https://osf.io/t72cw/ · 2026-11-16 |
| 40–49 | M | 1.28 (0.20) | 111.26 (7.47) | 1.38 (0.15) | 0.19 (0.05) | 2.20 (0.30) | https://osf.io/t72cw/ · 2026-11-16 |
| 40–49 | F | 1.23 (0.14) | 115.21 (6.42) | 1.28 (0.11) | 0.16 (0.02) | 1.75 (0.27) | https://osf.io/t72cw/ · 2026-11-16 |
| 50–59 | M | 1.26 (0.17) | 111.95 (5.80) | 1.35 (0.15) | 0.21 (0.04) | 2.08 (0.50) | https://osf.io/t72cw/ · 2026-11-16 |
| 50–59 | F | 1.28 (0.17) | 116.77 (8.62) | 1.32 (0.13) | 0.16 (0.03) | 1.73 (0.28) | https://osf.io/t72cw/ · 2026-11-16 |
| 60–69 | M | 1.27 (0.28) | 113.41 (12.20) | 1.34 (0.18) | 0.22 (0.05) | 2.06 (0.52) | https://osf.io/t72cw/ · 2026-11-16 |
| 60–69 | F | 1.25 (0.21) | 115.76 (7.93) | 1.30 (0.16) | 0.17 (0.03) | 1.81 (0.27) | https://osf.io/t72cw/ · 2026-11-16 |
| ≥70 | M | 1.19 (0.20) | 111.21 (7.21) | 1.29 (0.19) | 0.21 (0.05) | 2.09 (0.42) | https://osf.io/t72cw/ · 2026-11-16 |
| ≥70 | F | 1.09 (0.22) | 114.57 (10.47) | 1.14 (0.16) | 0.18 (0.04) | 1.75 (0.55) | https://osf.io/t72cw/ · 2026-11-16 |

Fast condition (`14_Table_Spatiotemporal_fast.pdf`), young men / young women / men ≥70 / women ≥70: speed 1.68 (0.21) / 1.59 (0.21) / 1.54 (0.26) / 1.42 (0.26) m/s; cadence 124.68 (9.14) / 124.79 (7.69) / 122.72 (9.15) / 124.66 (11.39); stride length 1.61 (0.14) / 1.53 (0.15) / 1.50 (0.19) / 1.36 (0.17) m. [https://osf.io/t72cw/ · accessed 2026-11-16]
Slow condition (`20_Table_Spatiotemporal_slow.pdf`), same order: speed 0.90 (0.12) / 0.86 (0.12) / 0.84 (0.13) / 0.77 (0.15) m/s; cadence 94.01 (6.93) / 93.45 (6.55) / 91.61 (9.04) / 94.28 (8.47); stride length 1.15 (0.10) / 1.10 (0.11) / 1.10 (0.13) / 0.98 (0.16) m. [https://osf.io/t72cw/ · accessed 2026-11-16]
Step-time asymmetry at comfortable speed: 3.3 ± 0.9 % (young men) up to 4.2 ± 1.5 % (women ≥70). Step-length asymmetry: 2.8 ± 0.9 % to 5.7 ± 4.4 %. A seeded generator can use these to keep the two legs from being perfect mirrors. [https://osf.io/t72cw/ · accessed 2026-11-16]

### 3c. Overground, by decade and sex (Öberg, Karsznia & Öberg 1993, *J Rehabil Res Dev* 30(2):210–223, Tables 2a/b, 3a/b, 5a/b, 8a/b; 5.5 m photocell walkway, indoor; 15 per sex per decade, 14 men aged 70–79)

| Age | Sex | Normal speed cm/s | Fast speed cm/s | Normal step length cm | Normal step frequency steps/s | Source URL · accessed |
|---|---|---|---|---|---|---|
| 20–29 | M | 122.7 ± 11.1 | 162.6 ± 20.1 | 61.6 ± 3.5 | 1.98 ± 0.13 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 20–29 | F | 124.1 ± 17.1 | 169.3 ± 23.0 | 59.1 ± 6.3 | 2.08 ± 0.15 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 30–39 | M | 131.6 ± 15.0 | 176.8 ± 28.5 | 64.9 ± 4.6 | 2.00 ± 0.14 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 30–39 | F | 128.5 ± 19.1 | 172.1 ± 28.0 | 59.7 ± 5.3 | 2.13 ± 0.17 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 40–49 | M | 132.8 ± 9.8 | 171.7 ± 17.6 | 64.7 ± 3.7 | 2.01 ± 0.11 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 40–49 | F | 124.7 ± 14.4 | 166.7 ± 17.9 | 57.1 ± 3.7 | 2.16 ± 0.16 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 50–59 | M | 125.2 ± 17.7 | 164.0 ± 24.6 | 63.5 ± 6.0 | 1.96 ± 0.18 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 50–59 | F | 110.5 ± 9.7 | 147.1 ± 18.1 | 53.5 ± 2.6 | 2.03 ± 0.13 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 60–69 | M | 127.7 ± 12.4 | 163.9 ± 20.2 | 65.0 ± 3.6 | 1.95 ± 0.14 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 60–69 | F | 115.7 ± 16.7 | 155.5 ± 23.2 | 55.3 ± 4.2 | 2.06 ± 0.18 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 70–79 | M | 118.2 ± 15.4 | 158.6 ± 24.9 | 61.5 ± 5.1 | 1.91 ± 0.14 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |
| 70–79 | F | 111.3 ± 12.5 | 141.8 ± 17.3 | 54.2 ± 3.7 | 2.03 ± 0.14 | https://pubmed.ncbi.nlm.nih.gov/8035350/ · 2026-11-16 |

The source text is a scanned OCR copy; I checked each row with speed ≈ step length × step frequency (for example, men 20–29: 0.616 m × 1.98 = 1.22 m/s against 1.227 m/s reported). Slow-gait speed, 20–29 y: men 83.1 ± 9.1 and women 83.7 ± 19.7 cm/s (Tables 1a/b). [https://pubmed.ncbi.nlm.nih.gov/8035350/ · accessed 2026-11-16]

### 3d. Comfortable walking speed, meta-analysis by decade and sex (Bohannon & Andrews 2011, *Physiotherapy* 97:182–189, Table 2; 41 studies, 23,111 subjects; grand mean (95 % CI) and the range of study means)

| Age | Men cm/s | Men range of study means | Women cm/s | Women range of study means | Source URL · accessed |
|---|---|---|---|---|---|
| 20–29 | 135.8 (127.0–144.7) | 121.7–147.4 | 134.1 (123.9–144.3) | 108.2–149.9 | https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |
| 30–39 | 143.3 (131.6–155.0) | 132.0–153.8 | 133.7 (119.3–148.2) | 125.6–141.5 | https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |
| 40–49 | 143.4 (135.3–151.4) | 127.0–147.0 | 139.0 (133.9–141.1, as printed) | 122.0–142.0 | https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |
| 50–59 | 143.3 (137.9–148.8) | 112.2–149.1 | 131.3 (122.2–140.5) | 110.0–155.5 | https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |
| 60–69 | 133.9 (126.6–141.2) | 103.3–159.0 | 124.1 (118.3–130.0) | 97.0–145.0 | https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |
| 70–79 | 126.2 (121.0–132.2) | 95.7–141.8 | 113.2 (107.2–119.2) | 83.0–150.0 | https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |
| 80–99 | 96.8 (83.4–110.1) | 60.8–122.1 | 94.3 (85.2–103.4) | 55.7–117.0 | https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |

These are CIs of the grand mean, not between-person SDs. For between-person spread, use Öberg 1993 or Senden 2024. [https://osf.io/t72cw/ ; https://pubmed.ncbi.nlm.nih.gov/8035350/ · accessed 2026-11-16]

## 4. Walk–run transition

| Measure | Value | Population | Source | Source URL · accessed |
|---|---|---|---|---|
| Preferred walk→run transition speed (treadmill, accelerating) | 2.06 m/s (SD not in abstract) | 20 young healthy adults, 10 M / 10 F | Hreljac 1993, *MSSE* 25(10):1158–1162, abstract | https://pubmed.ncbi.nlm.nih.gov/8231761/ · 2026-11-16 |
| Energetically optimal transition speed | 2.24 m/s | same | Hreljac 1993, abstract | https://pubmed.ncbi.nlm.nih.gov/8231761/ · 2026-11-16 |
| RPE at the preferred transition speed: walking vs running | 13.5 vs 10.0 (Borg) | same | Hreljac 1993, abstract | https://pubmed.ncbi.nlm.nih.gov/8231761/ · 2026-11-16 |
| Transition speed (treadmill, acceleration and deceleration 0.05–0.11 m/s²) | 1.88 m/s mean, range 1.30–2.55 | 18 healthy men, 23–38 y, leg length 0.86–1.09 m | Thorstensson & Roberthson 1987, *Acta Physiol Scand* 131:211–214, abstract | https://pubmed.ncbi.nlm.nih.gov/3673618/ · 2026-11-16 |
| Direction effect | deceleration gives a somewhat lower transition speed than acceleration; transition speed tends to rise with leg length | same | same | https://pubmed.ncbi.nlm.nih.gov/3673618/ · 2026-11-16 |
| Transition speed at 1 g | 1.98 ± 0.038 m/s (± SEM; SD ≈ 0.11) | 9 subjects (7 M, 2 F), leg length 0.89 ± 0.043 m | Kram, Domingo & Ferris 1997, *J Exp Biol* 200:821–826, Table 1 | https://spot.colorado.edu/~kram/walk-run.pdf · 2026-11-16 |
| Froude number at transition, 1 g | 0.45 ± 0.02 (SEM) | same | Kram 1997, Table 1 | https://spot.colorado.edu/~kram/walk-run.pdf · 2026-11-16 |
| Froude at transition, reduced gravity 0.8 / 0.6 / 0.5 / 0.4 g | 0.49 / 0.53 / 0.56 / 0.56 | same | Kram 1997, Table 1 | https://spot.colorado.edu/~kram/walk-run.pdf · 2026-11-16 |
| "Bipeds switch at about Fr = 0.5" (general statement) | ≈ 0.5 | humans and other bipeds | Kram 1997, abstract | https://spot.colorado.edu/~kram/walk-run.pdf · 2026-11-16 |

Generator rule supported by these: v_transition = √(Fr · g · L_leg) with Fr drawn from about 0.45 (SD ≈ 0.06 between people, from SEM 0.02 × √9). Allow hysteresis: run→walk happens slightly slower than walk→run (Thorstensson 1987). [https://pubmed.ncbi.nlm.nih.gov/3673618/ · accessed 2026-11-16]

## 5. Arm swing

| Measure | Value (mean ± SD) | Population / method | Source | Source URL · accessed |
|---|---|---|---|---|
| Arm swing amplitude, dominant arm, usual walk | 53.4 ± 16.6° (30–40 y); 42.5 ± 13.4° (41–50); 42.2 ± 16.0° (51–60); 35.7 ± 13.6° (61–77) | n = 12/20/14/14; wrist-worn gyroscope sensors; 1-min walks | Mirelman et al. 2015, *PLoS ONE* 10(8):e0136043, Table 2 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4549059/ · 2026-11-16 |
| Same, fast walk | 64.6 ± 19.8 / 58.3 ± 11.0 / 55.6 ± 14.8 / 53.9 ± 15.2° | same | same | https://pmc.ncbi.nlm.nih.gov/articles/PMC4549059/ · 2026-11-16 |
| Non-dominant arm, usual / fast | 49.6 ± 16.1 / 60.4 ± 14.6° (30–40 y); 43.8 ± 14.6 / 58.4 ± 9.6° (61–77 y) | same | same | https://pmc.ncbi.nlm.nih.gov/articles/PMC4549059/ · 2026-11-16 |
| Gait speed in those conditions | usual 1.43 ± 0.15 (30–40 y) … 1.24 ± 0.18 m/s (61–77); fast 1.75 ± 0.21 … 1.61 ± 0.19 m/s | same | same | https://pmc.ncbi.nlm.nih.gov/articles/PMC4549059/ · 2026-11-16 |
| Arm swing asymmetry, usual walk | 10.1 ± 6.3 % (30–40) … 6.4 ± 3.0 % (61–77) | same | same | https://pmc.ncbi.nlm.nih.gov/articles/PMC4549059/ · 2026-11-16 |
| Axial (trunk) rotation amplitude, usual / fast | 6.71 ± 2.4 / 8.21 ± 2.7° (30–40 y); 4.44 ± 1.5 / 5.2 ± 1.6° (61–77 y) | lower-back sensor | same | https://pmc.ncbi.nlm.nih.gov/articles/PMC4549059/ · 2026-11-16 |
| Shoulder flexion–extension range in gait | 24.6 ± 3.4° (peak flexion +7.05 ± 1.4° near mid-cycle; peak extension −21.74 ± 2.7° near 0 % and −23 ± 4.4° near 100 %); ab/adduction range 6.44 ± 0.2°; rotation range 4.57 ± 0.1° | 45 healthy men, 18–25 y, Vicon Plug-in Gait, speed not stated | Vorontcova et al. 2022, *Dela Press Conf Ser Med Sci* 1:011 (conference paper), abstract and Results | https://doi.org/10.56199/dpcsms.lejq3299 · 2026-11-16 |
| Elbow flexion–extension range in gait | 29.7 ± 10.2°; pronation–supination 14.2 ± 3.2° | 15 adults (8 F, 7 M), 25 ± 5 y, treadmill, markerless Azure Kinect at 30 Hz + AnyBody model | Kang et al. 2023, *Clin Shoulder Elbow* 26:126–130, Results ("ROM during a Gait Cycle") | https://pmc.ncbi.nlm.nih.gov/articles/PMC10277704/ · 2026-11-16 |

Quality warning: the two shoulder and elbow sources are weak (a conference paper, and a 30 Hz markerless study whose text mixes up shoulder and elbow labels). Use them only as rough ranges. [https://doi.org/10.56199/dpcsms.lejq3299 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC10277704/ · accessed 2026-11-16]

## 6. Variation with age, sex and load: summary of what was confirmed

- **Age:** after about 70 y, comfortable speed falls (Bohannon 2011: men 126.2 cm/s at 70–79, 96.8 cm/s at 80–99). Older adults in Senden 2024 had shorter strides, more double support, about half the push-off plantarflexion and a smaller pelvic obliquity (tables above). Arm swing amplitude falls with age (Mirelman 2015). [https://osf.io/t72cw/ ; https://pubmed.ncbi.nlm.nih.gov/21820535/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC4549059/ · accessed 2026-11-16]
- **Sex:** at the same age, women have shorter steps and a higher step frequency (Öberg 1993; Senden 2024), narrower steps (Senden: 0.16 vs 0.20 m) and more pelvic obliquity (Senden, §2b). Öberg reports significant sex differences in all gait parameters. [https://osf.io/t72cw/ ; https://pubmed.ncbi.nlm.nih.gov/8035350/ · accessed 2026-11-16]
- **Load:** see "Not found". The only open source read was in children: Ahmad & Barbosa 2019, *Sci Rep* 9:3364 (57 children aged 7–9, backpacks of 0 %, 10 %, 15 % of body mass and their usual school load). The children walked slower and at a lower cadence only with their usual school load, and stride length showed no significant main effect (abstract). Its numeric tables are images, which I did not transcribe. [https://doi.org/10.1038/s41598-019-40076-w · accessed 2026-11-16]


## 7. Sources and licences

| # | Source | What was used | Licence / use status | Link (accessed 2026-11-16) | Link status | Accessed |
|---|---|---|---|---|---|---|
| S1 | Senden R, Marcellis R, Willems P, Witlox M, Meijer K. *Normative 3D gait data of healthy adults walking at three different speeds on an instrumented treadmill in virtual reality.* Data in Brief 53:110230 (2024) | demographics, design, file descriptions | Article: **CC BY 4.0** (stated in the article) | https://doi.org/10.1016/j.dib.2024.110230 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC10912446/ | OK | 2026-11-16 |
| S1-data | Senden et al. dataset, OSF project t72cw: `01_Demo_PhysEx.xlsx`, `02/03/04_Overview_*.xlsx`, `05/06/07_AgeGenderGroup_*.xlsx`, `08/14/20_Table_Spatiotemporal_*.pdf`, `28_Description_parameters` | timing, curves, regressions (§1, §2a–b, §3a–b) | ⚠ **The OSF project shows no licence** (`node_license` is empty in the OSF API). Without a licence, reuse rights are not granted by default. Measured values cited as facts with attribution are fine; **do not redistribute the files or ship verbatim curve tables without confirming a licence with the authors** | https://osf.io/t72cw/ (files: https://osf.io/download/h8myj/ , https://osf.io/download/gd2ep/ , https://osf.io/download/a7c5h/ , https://osf.io/download/gxpu4/) | OK | 2026-11-16 |
| S2 | Fukuchi CA, Fukuchi RK, Duarte M. *A public dataset of overground and treadmill walking kinematics and kinetics in healthy individuals.* PeerJ 6:e4640 (2018) | WBDS participant data; design | Article **CC BY 4.0** | https://peerj.com/articles/4640/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC5922232/ ; https://doi.org/10.7717/peerj.4640 | OK (fetch); PMC OK | 2026-11-16 |
| S2-data | WBDS dataset, figshare 10.6084/m9.figshare.5722711 | not opened | Reported as CC BY by S3; **not verified by me (403)** | https://doi.org/10.6084/m9.figshare.5722711 | 403 | 2026-11-16 |
| S3 | Matsui Y. *PhysioGaitNorm* R package v0.2.1, `inst/extdata/adult_reference_bands_101.csv` and `MANIFEST.csv` (2026) | §2c curve table | Code MIT; data marked "CC-BY-4.0 (derived from WBDS)". Secondary derivative; frontal and transverse axis mapping inferred by the package author | https://github.com/x-biosignal/PhysioGaitNorm ; https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv | OK | 2026-11-16 |
| S4 | Fukuchi RK, Fukuchi CA, Duarte M. *A public dataset of running biomechanics and the effects of running speed on lower extremity kinematics and kinetics.* PeerJ 5:e3298 (2017) | §2d Table 4; foot strike | Article **CC BY 4.0**; the paper says the dataset (figshare 10.6084/m9.figshare.4543435) is CC BY | https://peerj.com/articles/3298/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC5426356/ | OK (fetch) | 2026-11-16 |
| S5 | Öberg T, Karsznia A, Öberg K. *Basic gait parameters: reference data for normal subjects, 10–79 years of age.* J Rehabil Res Dev 30(2):210–223 (1993) | §3c | Published paper, numbers cited as facts. The journal was a US Dept. of Veterans Affairs publication, but **public-domain status is not confirmed**. The full text was read from a third-party copy | https://pubmed.ncbi.nlm.nih.gov/8035350/ ; full-text copy read: https://www.rightsnet.org.uk/?ACT=39&aid=2655_O8ye9vGDx44qW2Bz7HlR&board_id=1&fid=2 | OK | 2026-11-16 |
| S6 | Bohannon RW, Andrews AW. *Normal walking speed: a descriptive meta-analysis.* Physiotherapy 97(3):182–189 (2011), doi 10.1016/j.physio.2010.12.004 | §3d | © Chartered Society of Physiotherapy / Elsevier, all rights reserved; numbers cited as facts. Table 2 was read from a full-text copy | https://pubmed.ncbi.nlm.nih.gov/21820535/ ; full-text copy read: https://www.rightsnet.org.uk/?ACT=39&aid=2202_w1iNWvpEYTcdHo7zLvHw&board_id=1&fid=2 | OK | 2026-11-16 |
| S7 | Hreljac A. *Preferred and energetically optimal gait transition speeds in human locomotion.* Med Sci Sports Exerc 25(10):1158–1162 (1993), doi 10.1249/00005768-199310000-00012 | §4 | Closed paper; abstract figures cited as facts | https://pubmed.ncbi.nlm.nih.gov/8231761/ | OK | 2026-11-16 |
| S8 | Thorstensson A, Roberthson H. *Adaptations to changing speed in human locomotion: speed of transition between walking and running.* Acta Physiol Scand 131(2):211–214 (1987) | §4 | Closed paper; abstract figures cited as facts | https://pubmed.ncbi.nlm.nih.gov/3673618/ ; https://doi.org/10.1111/j.1748-1716.1987.tb08228.x | OK | 2026-11-16 |
| S9 | Kram R, Domingo A, Ferris DP. *Effect of reduced gravity on the preferred walk–run transition speed.* J Exp Biol 200:821–826 (1997) | §4 Table 1 | © Company of Biologists (free to read); numbers cited as facts | https://pubmed.ncbi.nlm.nih.gov/9076966/ ; author PDF https://spot.colorado.edu/~kram/walk-run.pdf ; https://doi.org/10.1242/jeb.200.4.821 | OK (PDF) | 2026-11-16 |
| S10 | Novacheck TF. *The biomechanics of running.* Gait Posture 7:77–95 (1998), doi 10.1016/S0966-6362(97)00038-6 | §1 toe-off % | © Elsevier; facts cited. Text read from a third-party PDF copy | https://pubmed.ncbi.nlm.nih.gov/10200378/ ; copy read: https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf | OK | 2026-11-16 |
| S11 | Mirelman A et al. *Effects of aging on arm swing during gait: the role of gait speed and dual tasking.* PLoS ONE 10(8):e0136043 (2015) | §5 | **CC BY 4.0** | https://doi.org/10.1371/journal.pone.0136043 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC4549059/ | OK | 2026-11-16 |
| S12 | Vorontcova O, Udochkina L, Mazin I, Fedyulina E. *Detailed kinematic evaluation of shoulder motion during normal gait in healthy young males.* Dela Press Conf Ser Med Sci 1:011 (2022) | §5 | Licence not stated on the PDF; facts cited | https://doi.org/10.56199/dpcsms.lejq3299 ; PDF https://dpcsms.delapress.com/index.php/dpcsms/article/download/145/135 | OK | 2026-11-16 |
| S13 | Kang B, Jung GH, Kholinne E, Jeon IH, Kwak JM. *The elbow is the load-bearing joint during arm swing.* Clin Shoulder Elbow 26:126–130 (2023) | §5 | ⚠ **CC BY-NC 4.0** (non-commercial). Citing facts is fine; do not reuse its text or figures in a commercial product | https://pmc.ncbi.nlm.nih.gov/articles/PMC10277704/ (DOI 10.5397/cise.2023.00101 returned 500) | OK (PMC) | 2026-11-16 |
| S14 | Ahmad HN, Barbosa TM. *The effects of backpack carriage on gait kinematics and kinetics of schoolchildren.* Sci Rep 9:3364 (2019) | §6 load (qualitative) | **CC BY 4.0** | https://doi.org/10.1038/s41598-019-40076-w | OK | 2026-11-16 |
| S15 | Schreiber C, Moissenet F. *A multimodal dataset of human gait at different walking speeds established on injury-free adult participants.* Sci Data 6:111 (2019); dataset figshare 10.6084/m9.figshare.7734767 | population only (50 adults: 24 F, 26 M, 37.0 ± 13.6 y, 1.74 ± 0.09 m, 71.0 ± 12.3 kg; speeds 0–0.4, 0.4–0.8, 0.8–1.2 m/s, self-selected, fast); no gait numbers are in the paper | Article **CC BY 4.0**; dataset licence **not verified (figshare 403)** | https://doi.org/10.1038/s41597-019-0124-4 | OK | 2026-11-16 |
| S16 | Fukuchi CA, Fukuchi RK, Duarte M. *Effects of walking speed on gait biomechanics in healthy participants: a systematic review and meta-analysis.* Syst Rev 8:153 (2019) | checked; it gives only effect sizes, no absolute values, so none were used | CC BY 4.0 | https://doi.org/10.1186/s13643-019-1063-z | OK | 2026-11-16 |

**Licence red flags for H1:** (1) The Senden 2024 OSF data, the richest source here, has **no licence on OSF**. Cite its numbers as facts and ask the authors before shipping their curves verbatim. (2) S13 is **CC BY-NC**. (3) The WBDS curves came through a third-party package, not from the primary file. (4) The Öberg, Bohannon and Novacheck numbers were read from third-party full-text copies; the facts are citable, but do not redistribute those PDFs.

## Not found

- **Winter (*Biomechanics and Motor Control of Human Movement*, 2009) and Perry & Burnfield (*Gait Analysis*) tables:** copyrighted books; I did not open or copy them, so no number in this file comes from them. Fukuchi 2018 notes that Winter's book data come from a single healthy subject.
- **Kirtley CGA normative data** (clinicalgaitanalysis.com/data): the site returned HTTP 500. Not checked.
- **WBDS (Fukuchi 2018) and RBDS (Fukuchi 2017) raw files, and the Schreiber & Moissenet dataset:** figshare blocks this box (403). Not opened, so dataset licences were not checked first-hand, and the running joint-angle *curves* (as opposed to the peaks) are not here.
- **Running stance % with SD, and flight time vs speed:** only Novacheck's point values (no SD). Not found in an open source with spreads.
- **Hreljac 1993 transition speed SD:** the full text is closed; the abstract has no SD.
- **Load carriage in adults** (trunk forward lean in degrees, stride change per % body mass): no open-access source with numbers was confirmed. The Liew et al. 2016 J Appl Biomech meta-analysis is closed and not read.
- **Arm swing vs speed in degrees of shoulder angle from marker-based studies with speed stated** (for example Romkes & Bracht-Schweizer 2017, Gait Posture): closed; not read.
- **Children's gait (under 18):** not covered in this pass (Öberg 1993 has 10–19 y rows; they are in the source but not transcribed here).
- **Pelvis and hip frontal/transverse curves from a primary file for WBDS** (see S3 caveat).

## Sources disagree

| Quantity | Source A | Source B | Likely reason | Source URL · accessed |
|---|---|---|---|---|
| Sprint toe-off, % of cycle (3.9 m/s) | Novacheck 1998 body text p. 79: 36 % | Novacheck 1998 Fig. 6 caption: 37 % | Internal inconsistency in the same paper; use 36–37 % as the band (added 2026-11-17 after Library Verify) | https://www.niccostiff.co.uk/wp-content/uploads/2020/02/Biomechanics-of-running-gait.pdf · 2026-11-17 |
| Comfortable speed, men 20–29 | Öberg 1993: 122.7 ± 11.1 cm/s (5.5 m indoor walkway) | Bohannon 2011: 135.8 cm/s (95 % CI 127.0–144.7); Senden 2024: 1.30 ± 0.16 m/s (treadmill, men 18–29) | Short walkway and lab setting (Öberg says so himself: long walkways give higher speeds) | https://osf.io/t72cw/ ; https://pubmed.ncbi.nlm.nih.gov/8035350/ ; https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |
| Comfortable speed, women 50–59 | Öberg: 110.5 ± 9.7 cm/s | Bohannon: 131.3 cm/s; Senden: 1.28 ± 0.17 m/s | as above | https://osf.io/t72cw/ ; https://pubmed.ncbi.nlm.nih.gov/8035350/ ; https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |
| Pelvic tilt (absolute) | WBDS via S3: about 10–11° anterior, SD about 5° | Senden 2024: 5.4–6.8° | Different marker models and pelvis definitions; use the *range* of motion (about 1–2°), not the offset | https://osf.io/t72cw/ ; https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| Pelvic tilt range over the cycle | WBDS via S3: about 1.9° | Senden 2024 young men: about 0.6° | as above | https://osf.io/t72cw/ ; https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| Pelvic rotation range | WBDS via S3: about ±6° (−6.5 to 6.7°) | Senden 2024: about ±2–4° | Model differences; S3 transverse mapping is inferred | https://osf.io/t72cw/ ; https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| Peak ankle plantarflexion after toe-off | WBDS via S3: −13.0° (overground, barefoot) | Senden 2024 young: −17.1 to −17.5° (treadmill; footwear not checked) | Surface, footwear and model | https://osf.io/t72cw/ ; https://raw.githubusercontent.com/x-biosignal/PhysioGaitNorm/HEAD/inst/extdata/adult_reference_bands_101.csv · 2026-11-16 |
| Knee peak swing flexion vs speed | Senden 2024: about flat (65.0° comfortable vs 64.2° fast, young men) | Fukuchi 2019 meta-analysis: most kinematic peaks rise with speed | The Senden speed range is narrow (±30 %), and treadmill | https://osf.io/t72cw/ ; https://doi.org/10.1186/s13643-019-1063-z · 2026-11-16 |
| Walk→run transition speed | Hreljac 1993: 2.06 m/s | Thorstensson 1987: 1.88 m/s (range 1.30–2.55); Kram 1997: 1.98 m/s | Protocol (acceleration rate), sex mix and leg length; Froude-normalising reconciles most of it (Fr about 0.45) | https://pubmed.ncbi.nlm.nih.gov/8231761/ ; https://pubmed.ncbi.nlm.nih.gov/3673618/ ; https://spot.colorado.edu/~kram/walk-run.pdf · 2026-11-16 |
| Bohannon women 40–49 CI | printed as 139.0 (133.9–141.1) | — | The CI is not symmetric about the mean, which looks like a typo in the original table; it is reproduced as printed | https://pubmed.ncbi.nlm.nih.gov/21820535/ · 2026-11-16 |
| Shoulder sagittal arm swing | Vorontcova 2022: total range 24.6 ± 3.4° | Mirelman 2015: wrist "arm swing amplitude" 35.7–53.4° | Different measures: the shoulder joint angle vs the wrist sensor's angular excursion (which includes elbow and trunk); not comparable | https://pmc.ncbi.nlm.nih.gov/articles/PMC4549059/ ; https://doi.org/10.56199/dpcsms.lejq3299 · 2026-11-16 |
| Senden "Stance–Swing ratio" column | file value about 0.65 at comfortable speed | stance ÷ swing from the same file's times = 0.66/0.43 ≈ 1.5; stance ÷ stride ≈ 0.61 | The column's definition is unclear; I did not use it and computed stance % myself | https://osf.io/t72cw/ · 2026-11-16 |
| Senden stance-time definition | description file says "single support phase" | stance + swing = stride in the data, so it is the full stance | Wording error in the description file; the data are treated as full stance | https://osf.io/t72cw/ · 2026-11-16 |
