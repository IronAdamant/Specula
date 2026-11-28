# R9-E2 Perceptual thresholds: how far a proportion can stray before people notice or object

Round 9, section E. Access date for every source: **2026-11-28**. Everything is in my own words. Numbers are reported as facts with their location; **no dataset or stimulus was taken**. Reasoning marked **(mine)** is my own. Pages are **PDF pages** unless marked otherwise.

**The brief's rule for this item:** give only what states a **measured** threshold. Opinion, theory and the shape of the "uncanny valley" curve are left out unless a number was measured.

**Short answer.** Three open sources give measured numbers:
- **lengths of a moving limb** (S1): a change of about 2.7% is detectable when attended and expected; over 20% can go unnoticed when attention is elsewhere;
- **facial proportions** (S2): the acceptable range for four face ratios, per figure, with people stricter on realistic human faces;
- **whole-body weight** (S3): where people put "the same weight as me" on a set of avatars made 5% apart in BMI, and how steeply they fall off.

No source found measures a threshold for **whole-body proportion** (leg-to-body ratio, head-to-body ratio) by a psychophysical method with free full text. That is the main gap.

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | Harrison J., Rensink R.A., van de Panne M. (2004). "Obscuring length changes during animated motion." *ACM SIGGRAPH 2004 Papers* (ACM TOG 23(3)), pp. 569-573. doi:10.1145/1186562.1015761 | Author's copy at UBC: https://www.cs.ubc.ca/~van/papers/2004-siggraph-lengthchange.pdf (box copy 2,481,402 bytes, 5 pp.) | © ACM; author-posted copy. **No open licence** | Yes, all 5 pages |
| S2 | Green R.D., MacDorman K.F., Ho C.-C., Vasudevan S. (2008). "Sensitivity to the proportions of faces that vary in human likeness." *Computers in Human Behavior* 24(5):2456-2474. doi:10.1016/j.chb.2008.02.019 | Author's copy listed at http://www.macdorman.com/kfm/writings/pubs/Green2008CHBsensitivity.pdf. The box download failed, and a header check from the box on 2026-11-28 returned HTTP 500. **Read through WebFetch** (saved as raw/round9/green08.webfetch.txt) | © 2008 Elsevier ("All rights reserved"). Author-posted copy; **whether posting is permitted is not verified** | Yes: abstract, hypotheses, methods §2.1-2.3, Tables 1, 2, 9, results §3.1, discussion §4.1-4.5. **Cited by section and table**, because the WebFetch text has no reliable page breaks |
| S3 | Thaler A., Piryankova I., Stefanucci J.K., Pujades S., de la Rosa S., Streuber S., Romero J., Black M.J., Mohler B.J. (2018). "Visual perception and evaluation of photo-realistic self-avatars from 3D body scans in males and females." *Frontiers in ICT* 5:18. doi:10.3389/fict.2018.00018 | https://www.frontiersin.org/articles/10.3389/fict.2018.00018/full | CC BY 4.0 (PDF p.14; Crossref) | Yes: abstract, methods, results 3.1-3.4, discussion (PDF p.1-12) |

**Restricted-data note.** S3's avatars were made by registering each person's scan to a statistical body model learned from thousands of scans (PDF p.3), and the "average" body came from the CAESAR set (PDF p.4). Only S3's perceptual results are used here; nothing from that model or data.

**Looked at and left out:**
- Piryankova et al. 2014 (PLoS ONE 9(8):e103428): studies ownership of a thinner or fatter virtual body and reports size *estimates*, not a detection threshold (search summary only, **not read**).
- A 2024 Frontiers in Virtual Reality paper on detecting a distorted avatar **step length** (search summary: about 9-12%): this is about motion, not proportion. Not read; out of scope.

## 1. Length of a moving limb (S1)

**Set-up** (S1 PDF p.1-2):
- A two-link "arm" seen from above: shoulder fixed, hand moving on a circle at 0.3-0.7 revolutions per second, each link about 5° of visual angle.
- Two-alternative forced choice. Thresholds are the **Weber fraction** ∆l/l, so a lower number means people are more sensitive (PDF p.1 footnote; p.2).
- Reference point (PDF p.2): for lines drawn on paper, the Weber fraction for length is **2.9%** (they cite Coren et al. 1994).

**Measured thresholds** (S1 Figure 2, PDF p.3; text PDF p.2-3), mean ± standard error:

| Condition | Threshold |
|---|---|
| 1A: attended, change expected (trials blocked by posture) | 2.7 ± 0.3% |
| 1B: attended, change not expected (trials randomly interleaved) | 5.6 ± 0.4% |
| 2A-2C: busy with a counting task on the same arm; upper link / lower link / whole arm | 10.7 ± 1.4% / 14.5 ± 0.5% / 10.5 ± 0.8% |
| 3A-3B: counting task; arm grows / arm shrinks | 6.9 ± 1.4% / 19.7 ± 1.7% |
| 4A-4D: distractor elsewhere; grows (overlapping / disjoint) and shrinks (overlapping / disjoint) | 12.7 ± 2.4% / 17.6 ± 2.4% and 19.4 ± 2.5% / 23.3 ± 1.3% |

Other measured findings:
- **Speed** (PDF p.2): under the counting task, 9.2% at 0.3 rev/s, 10.5% at 0.5 and 10.7% at 0.7.
- **Grow against shrink** (PDF p.2, footnote 3): replicated with 34 more observers (n = 46): 8.0% for growing, 18.4% for shrinking. For three observers shrinking was slightly easier to see.
- S1 notes the 2.7% attended threshold is very close to the static-line figure, so movement itself did not blunt the judgement (PDF p.2).

**S1's own guidelines** (PDF p.3):
1. up to 2.7% will likely go unseen even when attended and expected;
2. never more than 20%, even when the viewer is not attending;
3. shrinking is less noticeable than growing;
4. slower changes are harder to detect;
5. changes are harder to see during fast motion;
6. giving the viewer another task masks changes;
7. expecting a change makes it more likely to be seen.

**Limits stated or visible in S1:**
- The stimulus is a stick figure, not a rendered body. S1 suggests a rendered 3D character makes segment ends less certain, which may make changes *harder* to see (PDF p.3). That is not measured.
- **Internal mismatch, flagged:** the general method says the target performance was "chance (75% correct)" (PDF p.2), while experiment 1 says the 80% threshold was found with PEST (PDF p.2). Both statements are in the paper; which criterion applies to which experiment is not fully clear.
- These thresholds are for a **change over time** in one figure, not a comparison between two still figures. **(mine)**

## 2. Facial proportions and human likeness (S2)

**Set-up** (S2 §2.1-2.3; Table 1):
- 208 participants (126 male, 81 female as given, which sums to 207; n = 194 for cheek width, Table 2 footnote; mean age 31.9, range 17-79) from many countries, online.
- 11 faces: 2 human photographs, 3D and 2D drawn humans, and robots.
- Four ratios were varied one at a time, each from -10% to +10% in 1% frames (21 frames; frame 11 is the original):
  - cheek width (cheek-bone width / head height);
  - eye separation (pupil distance / cheek width);
  - face height (eyes-to-chin / head height);
  - jaw width (mouth-level face width / pupil distance).
- Task 1: pick the best frame. Tasks 2-3: from the best frame, move in one direction until the face stops looking acceptable. **Tolerance** = the mean width of the acceptable range. **Sensitivity** = the standard deviation of the chosen best point (smaller = more agreement) (§3.1).

**Measured results** (S2 Table 2, §3.1), in **ratio units** (not per cent):

| Figure | Tolerance: cheek / eyes / face / jaw |
|---|---|
| Female photograph | 0.028 / 0.029 / 0.028 / 0.115 |
| Male photograph | 0.045 / 0.037 / 0.027 / 0.132 |
| 3D female | 0.048 / 0.036 / 0.027 / 0.095 |
| 3D male | 0.030 / 0.030 / 0.041 / 0.100 |
| 2D female | 0.026 / 0.053 / 0.038 / 0.111 |
| 2D male | 0.045 / 0.034 / 0.048 / 0.142 |
| Robots (5 figures), range | 0.026-0.058 / 0.035-0.047 / 0.032-0.059 / 0.114-0.168 |

- S2 states that people showed more sensitivity and less tolerance for human than robot figures, and for realistic humans (photographs and 3D) than for 2D drawings (§3.1).
- **Converting to per cent (mine):** dividing tolerance by the original ratio in Table 1 gives the acceptable range as a fraction of the feature. For example, female photograph face height 0.028 / 0.66 ≈ 4%, and eye separation 0.029 / 0.48 ≈ 6% (whole range, both sides together). Jaw-width ranges are wider in ratio units, but the jaw ratio itself is large (about 1.6-2.0), so as a fraction they are of the same order (female photograph 0.115 / 1.69 ≈ 7%). **These are my arithmetic, not S2's numbers.**
- **Hypotheses** (§4.1-4.2):
  - H1 (more humanlike → narrower acceptable range) was **not supported** overall: only face-height tolerance fell as human likeness rose; cheek-width tolerance rose slightly. S2 blames weak experimental control (different figures, not one figure at different realism).
  - H2 (more humanlike → more agreement on the best point) was supported for all ratios but eye separation.
  - H3 (more attractive → narrower range) was supported.
- **Best points** (§4.3, Table 9): people preferred narrower cheeks and jaws and longer faces than the originals; narrower-set eyes for humans, wider-set for robots. The shift was up to about 6 frames (6%) (3D female jaw).
- **Gender** (§3.6, §4.5): female participants were more tolerant in all four ratios (p < 0.01), but the effect sizes were small.
- **Creepiness** (§4.4, Fig. 6): rated creepiness peaked when human likeness was rated neutral. This is a rating pattern, not a threshold.
- **Internal mismatch, flagged:** the text of §3.1 says the 3D female had the least tolerance for jaw width at M = 0.047, but Table 2 gives 0.095 for that cell, and no jaw tolerance in Table 2 is below 0.095. The table is probably right (the 0.047 looks like a copying slip), but this is **UNVERIFIED**.

## 3. Whole-body weight on a realistic avatar (S3)

**Set-up** (S3 PDF p.3-5):
- 26 people (13 men, 13 women), each scanned. Nine avatars per person at 0, ±5, ±10, ±15, ±20% of their own BMI, with height, arm length and inseam kept fixed.
- Two shapes (own; an average body matched to their height and weight) × two textures (own photo; checkerboard).
- 1AFC task: "Is it the same weight as you? Yes/No", each avatar 20 times (180 trials). A method-of-adjustment task in 0.05% BMI steps.
- S3 chose percentage steps because, by Weber's law, the just-noticeable difference grows with magnitude (PDF p.4).

**Measured results:**
- **Where "same as me" falls** (1AFC, PDF p.7):
  - checkerboard texture −4.42% BMI (se 1.24), significantly below their real BMI;
  - own photo texture −1.54% (se 0.99), not significant.
  - By shape: own shape −3.17% (se 1.17), average shape −2.78% (se 1.17), both significant.
- **Adjustment task** (PDF p.7): checkerboard −3.09% (se 1.55), own photo +0.36% (se 1.32); own shape −2.16%, average shape −0.57%, neither different from 0. S3 credits the finer steps.
- **Fall-off (sensitivity)** (PDF p.8): Weibull fits on each side of the peak (mean R² 0.996; 13 of 208 slopes could not be fitted). The slope parameter β was **6.92 (se 0.92) for men and 13.91 (se 1.39) for women** (p = 0.02). So men accepted a wider range of bodies as their own weight.
- **Ideal weight** (PDF p.8): women set it to −8.27% BMI (se 2.06), men −1.62% (se 2.52).
- **Uncanniness** (PDF p.11), share reporting it:
  - own shape, own photo: 69.23% of men, 23.08% of women;
  - own shape, checkerboard: 30.77% and 7.69%;
  - average shape, own photo: 61.54% and 23.08%;
  - average shape, checkerboard: 30.77% and 15.38%.
- **What is not given:** S3 reports β (slope steepness), not a threshold in per cent. Turning β into "a body x% heavier is called different" would need the fitted location parameters too, which the text does not list. **No per-cent threshold is claimed here.**
- **Limits (mine, from the set-up):** 26 adults aged 20-37 (Table 1, PDF p.3); self-judgement (own body), not judging a stranger; the average shape came from a restricted data set (above).

## 4. Where the sources disagree or don't fit together
- **Lengths** (S1): about 3% when attended. **Face ratios** (S2): acceptable ranges of roughly 4-7% of a feature (my conversion). S1 measures *noticing a change*; S2 measures *accepting a still face*. Both views stand; they are different questions.
- **Humanlike faces and tolerance** (S2): the expected narrowing with realism (H1) was not found overall, but realistic humans were judged more strictly than drawings in the per-figure table. S2 reports both.
- **Weight** (S3): the 1AFC and adjustment tasks disagree on whether people misjudge their own weight. S3 itself puts this down to step size.

## 5. What a generator can take from this (mine)
- **Thresholds for "unnoticed" change** in a limb's length while animating: 2.7% (attended) to about 20% (unattended), with growth more visible than shrinkage (S1). Note that S1 used stick figures.
- **Thresholds for "acceptable" face ratios:** about ±2-3% each side of the best value for realistic faces (half of S2's whole range; my arithmetic), looser for drawings and robots.
- **Whole-body size:** 5% BMI steps were coarse enough that people's "same as me" choices fell within about one step of their real BMI (S3). There is no stranger-viewing threshold.
- Use these as **test tolerances**, not as truth: each comes from one study and one stimulus type.

## 6. Gaps
- No free, measured threshold for whole-body **proportions** (leg-to-body ratio, head-to-body ratio, limb thickness) as judged by others. Sorokowski's leg-length work and similar studies are paywalled and were not read.
- No per-cent threshold from S3 (only slope β).
- S2's author copy is now unreachable from the box (HTTP 500); its page numbers are not given.
- Nothing measured for animals.
