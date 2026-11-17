# H4 — Which posture and movement changes read as which emotion (recognition rates and effect sizes)

**Changelog**
- 2026-11-17 09:25 AEDT (Library Verify pass, verify file 09:19): R7 Alaerts 2011 observer count corrected "about 36" → **n = 32** (Methods: 37 tested; Emotion Recognition data lost for 2 male and 3 female; Table S1 "Sample of 32"; Europe PMC full text). D5 Roether 2009: noted that the same 1.82 ± 0.22 / 0.83 ± 0.31 m/s spreads are called SEM in one paragraph and SD in another, and that the gaits come from the Experiment 1 recordings (the sentences are printed in the Experiment 2 Results). Means unchanged.
- 2026-11-17 (AEDT): per-row access dates added. Every data row (the pipe-separated ID rows) now ends with its source URL and an access date (2026-11-16, the day each was checked); "same" in a URL cell was replaced by the URL itself. The induced-emotion block (G rows), which had a section-level URL only, gained a "Source URL" column. Numeric notes and "Sources disagree" bullets carry their URL and date too. No values were changed; H4 has no Library Verify file yet.


Access date for every URL: 2026-11-16. On that date, every PMC link returned HTTP 200 from the box, every DOI link resolved, and the JOV and Springer pages opened.
Every number was read from the cited table, text or abstract. Values are mean ± SD unless the row says SEM.
Chance level is stated where the source gives it.
Licence column:
- CC BY = commercial reuse of the paper allowed with attribution.
- Frontiers 2011 = Frontiers non-exclusive open-access licence: use, distribution and reproduction allowed with credit.
- Publisher © = not openly licensed. Only facts and numbers are taken from these sources; no wording or figures were copied.
Caution for generators: almost all of these sources use acted or posed emotion. Real spontaneous emotion is weaker (see Sources disagree).

## 1. Direction of change: what each emotion does to the body (generator rules)

ID  | Emotion  | Posture / movement change (relative to neutral)                                              | Strength of evidence / number                                            | Source                         | Licence     | URL | Accessed
D1  | Sad      | Head lowered (more head inclination); the largest posture change for sad gait                | Population mean head-angle change for sad vs neutral: −18.9 deg          | Roether 2009, Exp 3 methods    | Publisher © (ARVO), free to read | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | 2026-11-16
D2  | Sad      | Elbow flexion reduced (arms held straighter); reduced shoulder/elbow movement amplitude | Elbow posture −16.6 deg; movement-weight change shoulder −0.67, elbow −0.79 (model units) | Roether 2009, Exp 3 methods | same | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | 2026-11-16
D3  | Happy    | Elbow more flexed; head slightly raised; larger arm swing                                    | Elbow +2.5 deg; head +6.3 deg; movement weights shoulder +0.42, elbow +0.61 | Roether 2009, Exp 3 methods  | same        | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | 2026-11-16
D4  | Angry, fearful | Increased elbow flexion (named as one of the two most prominent posture findings)        | Direction only in the text read; magnitudes are in Fig 2B (not read off a graph) | Roether 2009, Results    | same        | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | 2026-11-16
D5  | Angry vs fearful | Gait speed: anger is more than twice as fast as fear                                 | Anger 1.82 ± 0.22 m/s; fear 0.83 ± 0.31 m/s (spread labelled "mean ± SEM" in one paragraph and "mean ± SD" in another for the same numbers; gaits of the 25 lay actors motion-captured in Experiment 1)  | Roether 2009, Exp 2 Results text (speed-matched classification; discriminant analysis) | same        | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | 2026-11-17
D6  | Sad, depressed | Slower walking, less arm swing, less vertical head bob, more side-to-side upper-body sway, more slumped posture | Direction only (abstract); n = 14 inpatients vs controls, plus 23 students after music mood induction | Michalak 2009, abstract | Publisher © | https://doi.org/10.1097/PSY.0b013e3181a2515c ; https://europepmc.org/abstract/MED/19414617 | 2026-11-16
D7  | Joy, anger vs sad | Walking fastest for joy and anger, slowest for sad. Neck and thoracic flexion with sadness, and trunk extension and shoulder depression with joy, held even after allowing for speed | Direction only (abstract); n = 16 walkers | Gross 2012, abstract | Publisher © | https://doi.org/10.1016/j.humov.2011.05.001 ; https://europepmc.org/abstract/MED/21835480 | 2026-11-16
D8  | Happy    | Bouncing gait, long strides, more arm movement                                               | Direction only (abstract), point-light walkers                           | Halovic & Kroos 2018, abstract | Publisher © | https://doi.org/10.1016/j.humov.2017.11.008 ; https://europepmc.org/abstract/MED/29174557 | 2026-11-16
D9  | Angry    | Long strides with more arm movement, faster cadence than happy ("stomping")                  | Direction only (abstract)                                                | Halovic & Kroos 2018, abstract | Publisher © | https://doi.org/10.1016/j.humov.2017.11.008 | 2026-11-16
D10 | Fearful  | Fast, short strides ("scurrying"); arm movement mostly in the forearms                       | Direction only (abstract)                                                | Halovic & Kroos 2018, abstract | Publisher © | https://doi.org/10.1016/j.humov.2017.11.008 | 2026-11-16
D11 | Sad      | Slow, short strides (slowest pace); reduced whole-arm movement                               | Direction only (abstract)                                                | Halovic & Kroos 2018, abstract | Publisher © | https://doi.org/10.1016/j.humov.2017.11.008 | 2026-11-16
D12 | All (posture vs kinematics) | Postural features (body contraction, limb angles, shoulder ratio) separated emotions better than kinematic ones | Classifier accuracy (chance 25%): time-averaged features 61% (angry 35, happy 62, neutral 63, fear 95); time-resolved features 84% (angry 79, happy 83, neutral 92, fear 83) | Poyo Solanas 2020, Results | CC BY 4.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC7148324/ | 2026-11-16

## 2. Measured gait changes under induced (felt, not acted) emotion

All rows are from Homagain & Ehgoetz Martens 2023 (PLoS One), Table 5. Emotion was induced with immersive 360° VR videos, and gait was measured on a Zeno pressure walkway. [https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ · accessed 2026-11-16]
Participants: n = 23 young adults, age 24 (range 20–32) y, 16 F / 7 M. Licence CC BY 4.0. URL: https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ (accessed 2026-11-16)
Values are mean (SD).
ID  | Measure            | Neutral         | Excitement      | Happy           | Anger           | Fear            | Sad             | Main effect | Source URL | Accessed
G1  | Step length (cm)   | 64.58 (6.40)    | 66.09 (6.22)    | 64.40 (5.56)    | 65.48 (5.78)    | 63.92 (6.23)    | 61.68 (6.64)    | p<0.001; Kendall W = 0.25 (small) | https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ | 2026-11-16
G2  | Velocity (cm/s)    | 122.11 (17.99)  | 128.13 (16.99)  | 121.95 (15.59)  | 126.72 (15.09)  | 123.33 (15.88)  | 113.91 (17.35)  | p<0.001; Kendall W = 0.31 (moderate) | https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ | 2026-11-16
G3  | Step time (s)      | 0.53 (0.04)     | 0.52 (0.03)     | 0.53 (0.03)     | 0.52 (0.02)     | 0.52 (0.02)     | 0.55 (0.04)     | p<0.001 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ | 2026-11-16
G4  | Stance time (s)    | 0.66 (0.06)     | 0.64 (0.05)     | 0.66 (0.05)     | 0.64 (0.04)     | 0.64 (0.04)     | 0.68 (0.06)     | p<0.001 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ | 2026-11-16
G5  | Swing time (s)     | 0.41 (0.02)     | 0.41 (0.02)     | 0.41 (0.03)     | 0.40 (0.02)     | 0.40 (0.02)     | 0.41 (0.02)     | p<0.001 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ | 2026-11-16
G6  | Step width (cm)    | 9.62 (3.44)     | 9.34 (3.03)     | 9.49 (3.27)     | 9.56 (3.40)     | 9.59 (3.36)     | 9.39 (3.59)     | p = 0.67 (no effect) | https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ | 2026-11-16
G7  | Variability (%CV) of step length, step time, velocity, step width, swing time | no significant effect of emotion (p = 0.09–0.56) | | | | | | Table 5 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ | 2026-11-16
For generators: felt sadness changes gait by roughly −7% speed and −4.5% step length relative to neutral; that is my arithmetic from G1 and G2. Excitement and anger change it by a few percent upward. [https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ · accessed 2026-11-16]
- Acted emotion (D5) changes speed far more than this. See Sources disagree. [https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ · accessed 2026-11-16]
- The paper also quotes η² = 0.64 for gait speed from Halovic & Kroos (acted gait) and η² = 0.395 from Park et al. These are secondary quotes; the primary text was not read. [https://doi.org/10.1016/j.humov.2017.11.008 · accessed 2026-11-16]

## 3. Recognition rates: how well observers read each emotion from the body

ID  | Stimulus type                                   | Observers (n), task                          | Recognition (% correct)                                                     | Source, table                       | Licence      | URL | Accessed
R1  | Acted emotional gait, avatar of mocap (25 lay actors) | 20 observers, 4-choice (chance 25%)    | Overall 78%; anger 70.3 ± 21.4; happiness 75.1 ± 23.0; fear 77.1 ± 14.1; sadness 89.8 ± 5.7 | Roether 2009, Table 1  | Publisher © (ARVO), free to read | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | 2026-11-16
R2  | Main confusions in R1                           | same                                         | anger read as happy 23.2 ± 19.2; happy read as anger 15.6 ± 11.3; fear read as sad 17.9 ± 5.7; sad read as fear 8.0 ± 5.5 | Roether 2009, Table 1 | same | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | 2026-11-16
R3  | Speed only: NEUTRAL gait played at each emotion's speed | 20 observers, 4-choice (chance 25%)  | read as the speed-matched emotion: anger 48.8 ± 13.8; happy 42.6 ± 15.8; fear 28.1 ± 9.4; sad 54.1 ± 14.1. Neutral gait at fear speed read as sad 61.2 ± 11.4 | Roether 2009, Table 2 | same | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | 2026-11-16
R4  | R1 split by actor sex                           | same                                         | fear: female actors just over 90, male actors 60.5; sadness: male 93, female 87.3 | Roether 2009, Results     | same         | https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 | 2026-11-16
R5  | Static whole-body photos, faces blurred (BEAST; 46 non-professional actors, 254 stimuli) | 19 observers, 4-choice, 4 s per image | anger 93.6; fear 93.9; happiness 85.4; sadness 97.8  | de Gelder & Van den Stock 2011, Table 1 | Frontiers 2011 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3152787/ | 2026-11-16
R6  | BEAST main confusions                           | same                                         | happiness read as fear 5.7, as anger 5.6; anger read as happiness 3.3       | de Gelder & Van den Stock 2011, Table 1 | Frontiers 2011 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3152787/ | 2026-11-16
R7  | Point-light walking/jumping/kicking with emotion | n = 32 observers (Experiment 2: 37 tested, Emotion test data lost for 5 — 2 M, 3 F; the paper prints Z(35)), 4-choice     | "% correct above chance": neutral 54.3, happy 44.2, sad 45.8, angry 58.6     | Alaerts 2011, Results               | CC BY 4.0    | https://pmc.ncbi.nlm.nih.gov/articles/PMC3111458/ | 2026-11-17
R8  | Full-light vs point-light videos of emotional body movement | adults (n = 14 from t(1,13)), forced choice | full-light 94.3; point-light 81.1 (teenagers aged 16–17, n=15: 90 and 68.7) | Ross 2012, Results 2.2 | CC BY | https://pmc.ncbi.nlm.nih.gov/articles/PMC3438163/ | 2026-11-16
R9  | Static computer mannequin postures, 3 viewpoints (176 postures) | forced choice, 6 emotions          | from 0 (many disgust postures) to over 90 (some anger and sadness postures); happiness and surprise confused | Coulson 2004, abstract | Publisher © | https://doi.org/10.1023/B:JONB.0000023655.25550.be | 2026-11-16
R10 | Narrative and scenario mocap, 11 categories (MPI database, 8 actors, 1447 clips) | 55 raters        | Agreement between intended and perceived emotion only 19% (chance about 9% for 11 categories; my arithmetic) | Volkova 2014, Results | CC BY 4.0 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4252031/ | 2026-11-16

Takeaways for animation:
- Sadness is the most legible emotion, through head down, slow pace and reduced arm swing. [https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 · accessed 2026-11-16]
- Anger and happiness are mostly confused with each other; both are fast with big arm swing. Anger needs the faster cadence and heavier footfall to separate them. [https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 · accessed 2026-11-16]
- Fear and sadness are both slow and get confused. Fear needs the forearm-dominated, short-quick step pattern and raised or bent elbows. [https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 · accessed 2026-11-16]
- Speed alone carries about half of the signal: R3 is roughly 43–54% correct vs 70–90% for full gait, except for fear. [https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 · accessed 2026-11-16]

## Source list (all accessed 2026-11-16)

1. Roether CL, Omlor L, Christensen A, Giese MA. Critical features for the perception of emotion from gait. J Vis 2009;9(6):15. doi:10.1167/9.6.15.
   © ARVO, free to read; no CC licence shown on the page read. https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 (DOI https://doi.org/10.1167/9.6.15 redirects; the redirect returned 500 to curl, but the JOV page opened via fetch) (accessed 2026-11-16)
2. Michalak J, Troje NF, Fischer J, et al. Embodiment of sadness and depression—gait patterns associated with dysphoric mood. Psychosom Med 2009;71(5):580–587. doi:10.1097/PSY.0b013e3181a2515c.
   Publisher ©; abstract only. https://doi.org/10.1097/PSY.0b013e3181a2515c ; https://europepmc.org/abstract/MED/19414617 (accessed 2026-11-16)
3. Gross MM, Crane EA, Fredrickson BL. Effort-Shape and kinematic assessment of bodily expression of emotion during gait. Hum Mov Sci 2012;31(1):202–221. doi:10.1016/j.humov.2011.05.001.
   Publisher ©; abstract only. https://doi.org/10.1016/j.humov.2011.05.001 ; https://europepmc.org/abstract/MED/21835480 (accessed 2026-11-16)
4. Halovic S, Kroos C. Not all is noticed: kinematic cues of emotion-specific gait. Hum Mov Sci 2018;57:478–488. doi:10.1016/j.humov.2017.11.008.
   Publisher ©; abstract only. https://doi.org/10.1016/j.humov.2017.11.008 ; https://europepmc.org/abstract/MED/29174557 (accessed 2026-11-16)
5. Poyo Solanas M, Vaessen MJ, de Gelder B. The role of computational and subjective features in emotional body expressions. Sci Rep 2020;10:6202. doi:10.1038/s41598-020-63125-1.
   CC BY 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC7148324/ (accessed 2026-11-16)
6. Homagain A, Ehgoetz Martens KA. Emotional states affect steady state walking performance. PLoS One 2023;18(9):e0284308. doi:10.1371/journal.pone.0284308.
   CC BY 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ (accessed 2026-11-16)
7. de Gelder B, Van den Stock J. The Bodily Expressive Action Stimulus Test (BEAST). Front Psychol 2011;2:181. doi:10.3389/fpsyg.2011.00181.
   Frontiers 2011 open-access licence (use, distribution and reproduction with credit). https://pmc.ncbi.nlm.nih.gov/articles/PMC3152787/ (accessed 2026-11-16)
8. Alaerts K, Nackaerts E, Meyns P, Swinnen SP, Wenderoth N. Action and emotion recognition from point light displays: an investigation of gender differences. PLoS One 2011;6(6):e20989. doi:10.1371/journal.pone.0020989.
   CC BY 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC3111458/ (accessed 2026-11-16)
9. Ross PD, Polson L, Grosbras MH. Developmental changes in emotion recognition from full-light and point-light displays of body movement. PLoS One 2012;7(9):e44815. doi:10.1371/journal.pone.0044815.
   CC BY. https://pmc.ncbi.nlm.nih.gov/articles/PMC3438163/ (accessed 2026-11-16)
10. Coulson M. Attributing emotion to static body postures: recognition accuracy, confusions, and viewpoint dependence. J Nonverbal Behav 2004;28(2):117–139. doi:10.1023/B:JONB.0000023655.25550.be.
    Publisher ©; abstract only. https://link.springer.com/article/10.1023/B:JONB.0000023655.25550.be (accessed 2026-11-16)
11. Volkova E, de la Rosa S, Bülthoff HH, Mohler B. The MPI emotional body expressions database for narrative scenarios. PLoS One 2014;9(12):e113647. doi:10.1371/journal.pone.0113647.
    CC BY 4.0. https://pmc.ncbi.nlm.nih.gov/articles/PMC4252031/ (accessed 2026-11-16)
    The paper is CC BY. The database itself has its own terms, which were not checked here; see H5 for dataset licences.

## Not found

- Joint-angle magnitudes per emotion with SD for anger and fear. Roether 2009 gives these in a figure (Fig 2B, in radians, mean ± SEM), not in a table, and values were not read off a graph. Only the sad and happy population means quoted in the text are used (D1–D3).
- Numbers from Wallbott 1998 (Eur J Soc Psychol), the classic coding of posture and movement cues per emotion. It is paywalled and was not read.
- Numbers from Dael 2012 (Emotion), the BAP coding of 12 emotions. The abstract has no numbers; the full text is paywalled.
- Numbers from Atkinson 2004 (Perception). The abstract states that exaggeration raises accuracy but gives no figures.
- Effect sizes (d or η²) for Michalak 2009 sad gait: directions only. Halovic & Kroos η² = 0.64 for speed is a second-hand quote (via Homagain 2023), not verified in the primary paper.
- Emotions beyond the basic four or five (pride, shame, boredom, relief) with recognition rates for whole-body posture. The MPI database covers them, but per-emotion accuracy is in figures only.
- Idle or standing (non-walking) posture parameters per emotion (shoulder height, chest expansion in cm or deg). Not found as numbers.
- Cross-cultural recognition rates for body posture (e.g., Kleinsmith 2006). Not read.

## Sources disagree

- Acted vs felt emotion size:
  - Acted angry vs fearful gait speeds differ by more than 2× (1.82 vs 0.83 m/s, Roether 2009). [https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 · accessed 2026-11-16]
  - With felt emotion induced in VR, the spread across all six states is only about 114–128 cm/s (Homagain 2023). [https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ · accessed 2026-11-16]
  - Generators that copy acted data will look theatrical. Pick acted or felt ranges deliberately.
- Recognition level:
  - Posed or acted stimuli with 4 choices give 78–98% (Roether R1, BEAST R5, Ross R8). [https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC3152787/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC3438163/ · accessed 2026-11-16]
  - Free narrative acting with 11 categories gives only 19% intended = perceived (Volkova R10). [https://pmc.ncbi.nlm.nih.gov/articles/PMC4252031/ · accessed 2026-11-16]
  - Static mannequin postures range from 0% to over 90% depending on the emotion (Coulson R9). [https://link.springer.com/article/10.1023/B:JONB.0000023655.25550.be · accessed 2026-11-16]
  - Coulson reports disgust as the weakest emotion from static posture. Sadness is the strongest in Roether and BEAST.
- Which emotion is easiest to recognise:
  - Roether gait: sadness best (89.8), anger worst (70.3). [https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 · accessed 2026-11-16]
  - BEAST static photos: sadness best (97.8), happiness worst (85.4). [https://pmc.ncbi.nlm.nih.gov/articles/PMC3152787/ · accessed 2026-11-16]
  - Poyo Solanas classifier: fear best (95% on time-averaged features), anger worst (35%). [https://pmc.ncbi.nlm.nih.gov/articles/PMC7148324/ · accessed 2026-11-16]
- Fear:
  - Roether reports fear recognition differs strongly by actor sex (female > 90% vs male 60.5%). [https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 · accessed 2026-11-16]
  - BEAST pools both sexes at 93.9%. [https://pmc.ncbi.nlm.nih.gov/articles/PMC3152787/ · accessed 2026-11-16]
- Halovic & Kroos (abstract) say fear can be both fast and slow, while Roether reports fear as the slowest-mean gait (0.83 m/s). In Homagain's felt-fear data, speed was close to neutral (123.33 vs 122.11 cm/s). [https://jov.arvojournals.org/article.aspx?doi=10.1167/9.6.15 ; https://doi.org/10.1016/j.humov.2017.11.008 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC10501668/ · accessed 2026-11-16]
