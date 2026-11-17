# D2: what makes a machine read as heavy, light, old or precise

Item D2 of `LIBRARY_RESEARCH_BRIEF.md` (§D Machines). Brief scope: "What makes a machine read as heavy, light, old or precise: panel lines, fasteners, wear patterns, proportion habits of industrial design". Form: "a list of conventions with examples named". Suggested source: "industrial design and concept art practice (practitioners' guides: say so)". Compiled 2026-11-17 (Australia/Sydney, AEDT) by World Data. Private working file: nothing here was posted, published or committed.

## Fixes after verify (2026-11-17, Australia/Sydney, AEDT)

Applied from `D2-machine-read.md.verify.md`. Library Verify checked this file at 40,578 bytes and did not edit it (47 verified, 5 fix, 15 unverifiable). After the fixes below the 67 data rows are **52 verified** and **15 unverifiable**.

1. **Rows 5.18–5.19.** Holm & Mattsson 1982, as quoted by Scott 2002, is on printed book page 53. PDF page 70 of the official Getty file is that book page. Cited as **p. 53 (PDF p. 70)** in the rows, the source table, §7 and the reference list.
2. **Row 5.21.** Fig. 2 gives the average time interval *between* frames, so the copper-patina span is **(34 − 1) × 31.6 = 1,042.8 min ≈ 17.4 h**, not 34 × 31.6 min.
3. **Row 3.8.** The "three experiments", "better than chance but well below what the information allows", and "3 diameters" wording is from the **NASA NTRS 1994 abstract**. The 1996 journal abstract (doi 10.3758/BF03206833) reports **five** experiments and says observers were much more sensitive to average velocity than to gravitational acceleration. Both claims are now attributed to the right abstract. The difference is also under 'Sources disagree'.
4. **Row 3.4.** Ujitoko et al. 2023 recruited 128 people (mean age 40.43, SD 10.93, for all 128) and analysed **78**, after excluding people whose PCs were not at 60 fps and 4 pixels/mm.
5. **Licences.** Todd & Norman 2026 remains **CC BY-NC-ND 4.0**. Buckingham & Goodale 2013 is **CC BY (no version stated)**.
6. **Rows 2.12 and 5.8** are now Verified: official Springer abstract for Alexander & Shansky 1976; official FAA AC 43.13-1B Change 1 Fig. 4-5, p. 4-20. Rows 4.2, 4.3, 5.6, 5.17, 5.22 and 6.1–6.10 stay Unverified.

## 0. Read this first

- **Source keys.** Every number has a key (D2-S1 …) with a full entry in §10, each with a URL and access date 2026-11-17.
- **Status labels.** **Verified** = I opened the primary text (or the official preview of a standard) and read the number myself. **Unverified** = not confirmed in a primary text. Extra tags: **[IMG]** read from a rendered page image; **[abstract-only]**; **[search-index-only]**; **[own-arithmetic]**; **[secondary-citation]** (the number is quoted by the source I opened, from an earlier study I did not open); **[practitioner]** (concept-art / game-dev practice, not measurement).
- **Kind.** **meas** measured on people or objects; **std** a value set by a standard or code (a rule, not a measurement); **desc** qualitative finding.
- **What exists and what doesn't.** There is good psychophysics on what makes an object *look and move* heavy, and good public standards for what "precise" means in millimetres and what rust rates are. There is **no measured study** I could find of panel-line density, fastener count or "greeble" amount against perceived heaviness, age or precision. Those conventions are listed in §7 as practice, tied to the nearest measured rows, and marked Unverified where they rest on practice alone.
- **Method.** WebSearch, WebFetch, curl, pdftotext and page images. Scratch copies are in `/tmp/d2/` (never in a repo). One lean **Grok live-search pass** was run today (Grok CLI, grok-4.6, reasoning effort xhigh, live web search; prompt `/tmp/grokrun/d2.txt`, raw output `/tmp/grokrun/d2.out`, process exited with code 0 before this file was written). Each Grok claim used here was re-opened in its source; rows using one say "via Grok". Grok errors I corrected are in §8. Grok leads I could not open stay Unverified in §6.
- **Licences, short form.** ISO standards are paid and copyrighted, so only facts (numbers) are used here, read from ISO's official free previews hosted by iTeh. FAA and FHWA documents are US Government works (public domain). Peters 2015 (PLoS ONE) is CC BY 4.0. Buckingham & Goodale 2013 (PLoS ONE) is CC BY (no version stated). Frontiers papers are CC BY 4.0. The Vibroengineering Procedia paper is CC BY. Todd & Norman 2026 is **CC BY-NC-ND 4.0 (non-commercial, no derivatives)**: numbers cited as facts only. ACM, APA, Elsevier, SAGE, Springer, AMA and Getty items are copyrighted: facts only, no figures or wording.

## 1. Sources and licences

| Key | Source | Gives | Licence |
|---|---|---|---|
| D2-S1 | ISO 2768-1:1989, official preview (iTeh) | General tolerances, classes f/m/c/v | © ISO, paid standard. Facts only |
| D2-S2 | ISO 9223:2012, official preview (iTeh) | First-year corrosion rates by corrosivity category | © ISO, paid standard. Facts only |
| D2-S3 | FAA AC 43.13-1B Change 1 (27 Sep 2001), par. 4-57 and Fig. 4-5 (p. 4-20). Official FAA PDF; par. 4-57 also in an NTSB docket excerpt | Rivet edge distance and pitch | US Government work, public domain |
| D2-S4 | FHWA Steel Bridge Design Handbook Vol. 14, *Splice Design*, FHWA-IF-12-052 (Nov 2012) | Bolt spacing rules quoted from AASHTO LRFD | US Government work, public domain (the AASHTO rules it quotes are © AASHTO; facts only) |
| D2-S5 | Duraiswamy, Campean, Harris, Munive-Hernandez (2018), DESIGN 2018 conference | Car body gap sizes and appeal ratings, n = 47 | Design Society proceedings, free download. No open licence printed: facts only |
| D2-S6 | Pang, Huang, Chen (2024), *Vibroengineering Procedia* 54 | Fender gap reference values by car class | CC BY (printed on p. 1) |
| D2-S7 | Peters, Balzer, Shams (2015), *PLoS ONE* 10(3):e0119794 | Density against volume for 195 man-made objects; people expect smaller to be denser | CC BY 4.0 |
| D2-S8 | Buckingham & Goodale (2013), *PLoS ONE* 8(1):e54709 | Size-weight and material-weight illusion sizes, n = 28 | CC BY (no version stated) |
| D2-S9 | Buckingham, Cant, Goodale (2009), *J Neurophysiol* 102:3111–3118 | Metal vs wood vs polystyrene look changes initial lift force | © APS. [abstract-only] |
| D2-S10 | Deng & Kahn (2009), *J Marketing Research* 46(6):725–738; accepted manuscript (Wharton) | Bottom and right placement reads heavier, n = 139 | © AMA. Author manuscript read; facts only |
| D2-S11 | Walker, Scallon, Francis (2017), *Perception* 46(7):772–792 | Felt heaviness maps to darker and lower pitch | © SAGE. [abstract-only] |
| D2-S12 | Alexander & Shansky (1976), *Perception & Psychophysics* 19(1):72–74 | Colour lightness and chroma against perceived heaviness | © Springer. [abstract-only] |
| D2-S13 | Cholewiak, Fleming, Singh (2013), *J Vision* 13(4):12 | Perceived stability follows the least-stable tilt direction | ARVO, open access. [abstract-only]: full text blocked (403/captcha) |
| D2-S14 | Runeson & Frykholm (1981), *JEP:HPP* 7(4):733–740 | Box weight seen from a lifter's motion | © APA. [abstract-only] (PubMed) |
| D2-S15 | Hamilton, Joyce, Flanagan, Frith, Wolpert (2007), *Psychological Research* 71:13–21 | Lift duration is the main cue to seen weight | © Springer. [abstract-only] |
| D2-S16 | Zhou, Hadfield-Menell, Nagabandi, Dragan (2017), HRI '17 | Faster robot arm motion makes the carried object look lighter | © owner/authors, rights licensed to ACM. Author PDF read; facts only |
| D2-S17 | Ujitoko, Kaneko, Yokosaka, Kawabe (2023), *Frontiers in Psychology* 14:1042188 | Fall speed after release and judged heaviness; 128 recruited, 78 analysed | CC BY |
| D2-S18 | Reitsma & Pollard (2003), *ACM TOG* 22(3):537–542 (SIGGRAPH 2003) | Detection thresholds for gravity and velocity errors in ballistic motion | © ACM. Author PDF (CMU) read; facts only |
| D2-S19 | Hecht, Kaiser, Banks (1996), *Perception & Psychophysics* 58:1066–1075; NASA NTRS record 20020006041 | Gravity as a cue to absolute size and distance | NTRS 1994 record is a NASA-hosted abstract; 1996 journal abstract also opened. The two abstracts disagree (see §10). [abstract-only] |
| D2-S20 | Watson, Banks, von Hofsten, Royden (1992), *Perception* 21(1):69–76 | Gravity as a monocular cue to size | © SAGE. [abstract-only] |
| D2-S21 | Hamrick, Battaglia, Griffiths, Tenenbaum (2016), *Cognition* 157:61–76 | Mass inferred from how a block tower falls, 10:1 ratio | © Elsevier. Author PDF (Princeton) read; facts only |
| D2-S22 | Todd & Norman (2026), *J Vision* 26(2):1 | Reanalysis of collision mass judgements, incl. Todd & Warren 1982 | **CC BY-NC-ND 4.0 (non-commercial, no derivatives)**. Facts only |
| D2-S23 | Gu, Tu, Ramamoorthi, Belhumeur, Matusik, Nayar (2006), *ACM TOG* 25(3):762–771 (SIGGRAPH 2006) | Time-varying appearance database: 26 samples incl. rust and copper patina | © ACM. Columbia author PDF read; the STAF data were not downloaded and their terms were not found (§9) |
| D2-S24 | Scott (2002), *Copper and Bronze in Art*, Getty Conservation Institute, p. 53 (PDF p. 70) | Holm & Mattsson (1982) 16-year copper-alloy exposure | © J. Paul Getty Trust (Getty offers a free PDF). Facts only. [secondary-citation] |

## 2. Heavy vs light: size, material, colour, placement (static cues)

| # | Finding | Value (spread) | Who / what measured | Kind | Status | Source |
|---|---|---|---|---|---|---|
| 2.1 | Smaller man-made objects are denser. Density falls as a power function of volume | Pooled log volume vs log density R = −0.5721 (p << .001). Per dataset R = −0.4673, −0.6290, −0.7917 | 195 liftable man-made everyday objects, volume by computer vision, weighed | meas | Verified | D2-S7, Results, "Environmental object data" |
| 2.2 | Same relation does **not** hold for natural or unliftable objects | Natural liftable R = −0.0048 (p = .981). Unliftable man-made R = 0.0396 (p = .8416) | Natural and unliftable object sets in the same study | meas | Verified | D2-S7, same section |
| 2.3 | Fitted log ratios for pairs of objects | log weight ratio = 0.613 · log volume ratio + 0.114. log density ratio = −0.387 · log volume ratio + 0.114 | Small/large pairs from the 195-object set | meas | Verified | D2-S7, Fig. 5 text |
| 2.4 | People expect the smaller of two objects to be denser, beyond misjudging volume. Their expected weight ratios match the environment fit | Expected-weight ratios consistently above perceived-volume ratios. n = 10 per group | Human observers judging novel objects without lifting | meas / desc | Verified | D2-S7, Figs. 3–5 text and Methods |
| 2.5 | **Engine reading of 2.1–2.4:** doubling a machine's visible volume should not double its expected mass. Weight scales about as volume^0.61 for hand-sized made objects | exponent 0.613 (from 2.3) | — | calc | Verified source; the "volume^0.61" restatement is **[own-arithmetic]** (exponentiating the log–log line) | D2-S7 |
| 2.6 | Size-weight illusion (SWI): of two equal-mass cubes, the small one feels heavier | Normalised heaviness rating: small +0.81, large −0.81. Size effect F(1,27) = 1371.7, partial η² = .98 | n = 28 (21 F, 7 M; mean age 22.5, SD 4.7). Cubes 5 cm and 10 cm, all 700 g | meas | Verified | D2-S8, Methods and Results |
| 2.7 | Material-weight illusion (MWI): the light-looking (polystyrene) cube feels heavier than the metal-looking one at equal mass. Smaller than the SWI | Polystyrene +0.1, metal −0.1. Material effect F(1,27) = 19.6, partial η² = .42. No size × material interaction (p = .28) | Same n = 28. Aluminium look (natural 2.7 g/cm³) vs expanded polystyrene look (0.1 g/cm³), all 700 g | meas | Verified | D2-S8, Results |
| 2.8 | Metal-looking blocks are first gripped and lifted with more force than polystyrene-looking ones. The look sets the *expectation* of heavy | Direction only in the abstract | Metal, wood and expanded-polystyrene cubes adjusted in mass | desc | Verified [abstract-only] | D2-S9 (PubMed abstract) |
| 2.9 | Bottom and right placement read heavier. Bottom-heavy is the stronger effect | Within-subject means on a −4…+4 heaviness index: top −0.72 vs bottom 0.81; left −0.30 vs right 0.65; top-left −1.12 vs bottom-right 0.93. Index α = .87 | Study 1, N = 139 (US university students). Product image placed in 6 locations on a package façade | meas | Verified (manuscript p. 12) | D2-S10 |
| 2.10 | The right-is-heavier part depends on eye dominance | Right-eyed: left −0.58 vs right 0.55. Left-eyed: left 0.29 vs right 0.59 (n.s.). 77% of participants were right-eyed | Study 1a, N = 117 | meas | Verified | D2-S10, Study 1a |
| 2.11 | Objects that feel heavier are judged darker and lower-pitched (the correspondence runs both ways) | No numbers in the abstract | Hidden objects varying independently in size and mass, lifted | desc | Verified [abstract-only] | D2-S11 (Crossref abstract) |
| 2.12 | Perceived heaviness of a colour falls with lightness (value) and rises with chroma. Hue matters little | No numbers in the abstract | Magnitude estimation of colour samples | desc | Verified [abstract-only] | D2-S12 (official Springer abstract) |
| 2.13 | Overall perceived stability is pulled toward the **least** stable tilt direction (minimum critical angle) | No numbers in the abstract | Asymmetric skewed conical frustums, 3 experiments | desc | Verified [abstract-only] | D2-S13 (Europe PMC abstract) |

## 3. Heavy vs light: motion cues (animation)

| # | Finding | Value (spread) | Who / what measured | Kind | Status | Source |
|---|---|---|---|---|---|---|
| 3.1 | Box weight can be seen from the lifter's movement alone (point lights) | Judged weight against true weight: average slope 0.87, pooled SD 3.8 kg (Exp. 1, videotape, 21 patches). Live action: visual slope 1.00, SD 3.1 kg. Haptic slope 1.20, SD 2.0 kg | Observers watching a person lift and carry a box. n not in the abstract | meas | Verified [abstract-only] | D2-S14 (PubMed abstract) |
| 3.2 | The main cue observers use is the **duration of the lift**. Grasp and place durations matter less. In real lifting, grasp duration best predicts true weight | No numbers in the abstract | Manipulated full-figure box-lifting videos | desc | Verified [abstract-only] | D2-S15 (Europe PMC abstract) |
| 3.3 | A robot arm that moves **faster** makes the object it carries look **lighter**. Pauses did not change perceived weight | Speed effect on weight F(1,163) = 19.97, p < .0001. 7-point scales | n = 40 (within-subjects, 4 conditions; 80% M, median age 29; US Mechanical Turk). Exploratory study n = 61 | meas | Verified | D2-S16, §4 results ("Weight") |
| 3.4 | An object that stays put after release is judged **lighter**. Among fall accelerations above zero, fall speed made no significant difference | Accelerations tested: 0, 2.45, 4.9, 9.8, 19.6, 39.2 m/s² (0, 0.25×, 0.5×, 1×, 2×, 4× of 9.8). Simulated drop 0.54 m at 6.53 m camera distance, 50 mm on screen | 128 recruited (16 M + 16 F in each age band 20s–50s; mean age 40.43, SD 10.93, for all 128; Japan, online). 78 analysed, after excluding people whose PCs were not at 60 fps and 4 pixels/mm. Object lifted with a mouse, then released | meas | Verified | D2-S17, Method and Abstract |
| 3.5 | Detection thresholds for wrong gravity in a ballistic jump (sensitivity d′ ≤ 0.25): effective gravity between −12.7 and −9.0 m/s² is hard to detect | Gravity interval [−12.7, −9.0] m/s². Horizontal velocity change over 0.1 s: [−0.13, +0.10] m/s. Vertical: [−0.32, +0.22] m/s | Study 2: n = 11 (9 F, 2 M, 19–29 y). Study 1: n = 12 (5 F, 7 M, 18–42 y). Human jumps, initial velocities about 2 m/s vertical and 1.5 m/s horizontal | meas | Verified | D2-S18, Table 2 |
| 3.6 | **Low** gravity (floaty) is easier to spot than **high** gravity. Added acceleration is easier to spot than deceleration | Mean ratings (SEM): low g 2.7 (0.2) vs high g 4.7 (0.2), F(1,196) = 41.9. Acceleration 2.6 (0.2) vs deceleration 4.1 (0.2), F(1,430) = 38.2. Lower rating = more often judged wrong | As 3.5 | meas | Verified | D2-S18, §3 results |
| 3.7 | Gravity sets the scale of a falling or bouncing object: time to fall tells the eye how big and far it is | Formal analysis only, no data in the abstract | — | desc | Verified [abstract-only] | D2-S20 |
| 3.8 | Observers use free-fall when judging size and distance. The NASA NTRS 1994 abstract says they did better than chance but well below what the information allows, in three experiments with simulated balls of three diameters rising and falling at many distances. The 1996 journal abstract (doi 10.3758/BF03206833) reports five experiments and says observers were much more sensitive to average velocity than to gravitational acceleration | No numbers in either abstract | See Finding for the two abstracts | desc | Verified [abstract-only] | D2-S19: NTRS 1994 abstract for the three-experiment / better-than-chance wording; 1996 journal abstract for five experiments and velocity vs gravity |
| 3.9 | People infer which of two block colours is heavier from whether a tower falls | Accuracy 81.9% (95% CI 79.9–83.7%). Individual accuracy 45–100%. 9 of 40 stimuli not above chance | Exp. 1, 81 analysed of 101 recruited (US Mechanical Turk). Mass ratio 1:10 or 10:1, "approximately" steel vs oak | meas | Verified | D2-S21, §2.1 and §2.6 |
| 3.10 | Collision: when both objects approach at equal speed, the heavier one is picked well. When a moving object hits a stationary one, judgements fail | 95% vs 23% accuracy (mass ratio 1.5, restitution e = 0.5) | Todd & Warren 1982 displays | meas | Verified **[secondary-citation]** (Todd & Warren 1982 not opened) | D2-S22, Fig. 1 caption |
| 3.11 | A simple model using post-collision velocities fits mass judgements as well as a "noisy Newton" model | Sigmoid fit r = 0.95, mean prediction error 1.4% | Reanalysis of earlier published datasets | meas | Verified | D2-S22, Results |

## 4. Precise vs crude: standards in millimetres

ISO 2768-1 sets default tolerances for any dimension drawn without its own tolerance. The class reads straight across to "precise" (f) and "crude" (c, v). Read from the rendered page image of ISO's official preview, printed p. 2 [IMG].

**Table 1, linear dimensions (mm, ±):**

| Nominal size | f fine | m medium | c coarse | v very coarse | Status | Source |
|---|---|---|---|---|---|---|
| 0.5 to 3 | 0.05 | 0.1 | 0.2 | — | Verified [IMG] | D2-S1 Table 1, p. 2 |
| over 3 to 6 | 0.05 | 0.1 | 0.3 | 0.5 | Verified [IMG] | D2-S1 |
| over 6 to 30 | 0.1 | 0.2 | 0.5 | 1 | Verified [IMG] | D2-S1 |
| over 30 to 120 | 0.15 | 0.3 | 0.8 | 1.5 | Verified [IMG] | D2-S1 |
| over 120 to 400 | 0.2 | 0.5 | 1.2 | 2.5 | Verified [IMG] | D2-S1 |
| over 400 to 1000 | 0.3 | 0.8 | 2 | 4 | Verified [IMG] | D2-S1 |
| over 1000 to 2000 | 0.5 | 1.2 | 3 | 6 | Verified [IMG] | D2-S1 |
| over 2000 to 4000 | — | 2 | 4 | 8 | Verified [IMG] | D2-S1 |

**Table 2, broken edges (external radii and chamfer heights, mm, ±):** 0.5 to 3: f/m 0.2, c/v 0.4. Over 3 to 6: f/m 0.5, c/v 1. Over 6: f/m 1, c/v 2. Verified [IMG], D2-S1 p. 2.

**Table 3, angles (by the shorter side):** up to 10 mm: f/m ±1°, c ±1°30′, v ±3°. Over 10 to 50: f/m ±0°30′, c ±1°, v ±2°. Over 50 to 120: f/m ±0°20′, c ±0°30′, v ±1°. Over 120 to 400: f/m ±0°10′, c ±0°15′, v ±0°30′. Over 400: f/m ±0°5′, c ±0°10′, v ±0°20′. Verified [IMG], D2-S1 p. 2.

The standard says it applies to parts made by metal removal or formed from sheet metal (§1), and that the class should follow "customary workshop accuracy" (§1). It is a drawing default, not a measured result.

| # | Item | Value | What | Kind | Status | Source |
|---|---|---|---|---|---|---|
| 4.1 | Fine vs very coarse on a 6–30 mm feature | ±0.1 vs ±1 mm, a factor of 10 | From Table 1 | calc | Verified source, **[own-arithmetic]** | D2-S1 |
| 4.2 | ISO 286-1 IT grades, 30–50 mm: IT6 16 µm, IT7 25 µm, IT8 39 µm, IT9 62 µm, IT10 100 µm, IT11 160 µm, IT12 0.25 mm | as listed | Fit tolerance grades | std | **Unverified** (via Grok and search index; the iTeh preview of ISO 286-1 stops before Table 1) | ISO 286-1:2010 Table 1, not opened |
| 4.3 | ISO 1302 roughness N-grades: N6 Ra 0.8 µm … N12 Ra 50 µm | as listed by Grok | Surface finish classes | std | **Unverified** (via Grok, secondary transcription; not opened) | ISO 1302 |

## 5. Panel gaps, fasteners and old metal

### 5a. Panel gaps (car bodies: the best-measured "panel line")

| # | Finding | Value (spread) | Who / what measured | Kind | Status | Source |
|---|---|---|---|---|---|---|
| 5.1 | A real gap specification at two design stages | Location 1: 3.5 ± 1 mm initially, 4.6 ± 1.6 mm after variation analysis. Location 2: 3.5 ± 1 mm, then 4.6 ± 1 mm | Tailgate to body side, one vehicle programme | std | Verified | D2-S5 Table 2, p. 241 |
| 5.2 | Smaller gaps are rated more appealing. Appeal falls at 4.6 mm and above | Gaps shown: 2.5, 3.0, 3.5, 4.6, 6.2 mm. Gap differences below 2 mm did not change appeal significantly. 2.1 mm (2.5 vs 4.6 mm) gave p = 0.057. Differences above 3 mm were significant | n = 47 company employees (23 F, 24 M), CAD images, Mann–Whitney tests | meas | Verified | D2-S5 §4.1, pp. 242–243 |
| 5.3 | Without prompting, few people notice a 1.1 mm gap change | 3.5 vs 4.6 mm noticed by 12.77% on first showing. 31.91% on the repeat (Hawthorne effect) | Same n = 47 | meas | Verified | D2-S5 §4.2, p. 243 |
| 5.4 | Gap targets tighten with car class. Large cars run smaller, tighter gaps | Fender to bonnet: A-class 5 ± 1.5, B 4 ± 1.5, C 3.5 (+1/0), D 3 (+1/0) mm. Fender to front door: A 5 ± 1.2, B 4.5 ± 1, C 4 ± 0.8, D 3.5 ± 0.75 mm. Fender to headlamp: A 3 ± 1.5 … D 1 ± 0.5 mm | Industry reference values (Chinese class labels, A smallest to D largest) | std | Verified [IMG] (asymmetric tolerances read from the page image) | D2-S6 Table 1, p. 41 |
| 5.5 | The paper gives no flush reference: "There is no unified reference value for component flush in the industry" | — | — | desc | Verified | D2-S6 p. 41 |
| 5.6 | VW ID.3 workshop panel gaps 3.5 ± 0.5 mm | as stated | One model | std | **Unverified** (via Grok; host `vw-id3.com` is not an official Volkswagen domain) | — |

### 5b. Fasteners (rivet and bolt spacing as public rules)

| # | Rule | Value | Applies to | Kind | Status | Source |
|---|---|---|---|---|---|---|
| 5.7 | Single-row rivets: edge distance at least 2 rivet diameters; spacing (pitch) at least 3 diameters, centre to centre | 2D edge, 3D pitch | Aircraft sheet-metal repair. Use the original structure's spacing where known | std | Verified | D2-S3 par. 4-57c(1) |
| 5.8 | Double and multiple rows: minimums in AC 43.13-1B Fig. 4-5 | Double row: edge 2D, inter-row 4D, pitch 4D (75% net). Triple/multiple: edge 2D, inter-row 3D, 3D/6D longitudinal (83% net) | Aircraft sheet-metal repair | std | Verified [IMG] | D2-S3 Fig. 4-5, p. 4-20 |
| 5.9 | Bolts in standard holes: spacing at least 3 bolt diameters | s_min = 3d. Worked example: 7/8 in bolt → 2.63 in | Steel bridge splices (AASHTO LRFD Art. 6.13.2.6.1) | std | Verified | D2-S4 pp. 22, 44 |
| 5.10 | Maximum bolt spacing along a free edge, for sealing | s ≤ 4.0 + 4.0t ≤ 7.0 in (t = thickness of the thinner outside plate, in). Example t = 0.625 in → 6.5 in | Same (Art. 6.13.2.6.2) | std | Verified | D2-S4 pp. 22, 45 |
| 5.11 | Edge distance for 7/8 in bolts: minimum 1-1/2 in. Maximum 8.0t, not over 5.0 in | as stated | Same (Table 6.13.2.6.6-1) | std | Verified (example values only; the full table is an image I did not transcribe) | D2-S4 p. 45 |
| 5.12 | **Engine reading:** fastener rows read "built" when pitch sits between about 3d and the sealing maximum, and the first fastener is about 1.5–2 diameters from the edge | 5.7, 5.9–5.11 | — | calc | Verified sources; the 1.5–2d edge ratio is **[own-arithmetic]** (1.5 in / 0.875 in ≈ 1.7d for bolts; 2d for rivets) | D2-S3, D2-S4 |

### 5c. Old: corrosion, patina and weathering

| # | Finding | Value (spread) | What | Kind | Status | Source |
|---|---|---|---|---|---|---|
| 5.13 | First-year carbon-steel corrosion by category (µm/a) | C1 ≤ 1.3; C2 1.3–25; C3 25–50; C4 50–80; C5 80–200; CX 200–700 | Standard flat specimens, one year of outdoor exposure | std | Verified | D2-S2 Table 2 |
| 5.14 | Same table, g/(m²·a), carbon steel | C1 ≤ 10; C2 10–200; C3 200–400; C4 400–650; C5 650–1500; CX 1500–5500 | as 5.13 | std | Verified | D2-S2 Table 2 |
| 5.15 | Zinc (µm/a): C1 ≤ 0.1; C2 0.1–0.7; C3 0.7–2.1; C4 2.1–4.2; C5 4.2–8.4; CX 8.4–25. Copper (µm/a): C1 ≤ 0.1; C2 0.1–0.6; C3 0.6–1.3; C4 1.3–2.8; C5 2.8–5.6; CX 5.6–10. Aluminium: given in g/(m²·a) only (C1 negligible … CX > 10) | as listed | as 5.13 | std | Verified (via Grok, checked in the preview text) | D2-S2 Table 2 |
| 5.16 | First-year rates "cannot be simply extrapolated" to long-term loss. ISO 9224 holds the long-term values | — | — | desc | Verified | D2-S2 §7 |
| 5.17 | ISO 9224 long-term guiding values (e.g. C3 steel ≤ 50/72/116/167/206/240 µm at 1/2/5/10/15/20 years) | as listed by Grok | — | std | **Unverified** (via Grok, from an excerpt of a paid standard; not opened) | ISO 9224:2012 |
| 5.18 | Copper alloys turn green in years, not months, and not everywhere | Greenish patina on most alloys after 6–7 years at urban and marine sites. No distinct green at the rural site after 16 years (black or brown only) | Holm & Mattsson 1982: 36 copper alloys, sheet and rod, rural / marine / urban sites, 16 years | meas | Verified **[secondary-citation]** | D2-S24 p. 53 (PDF p. 70) |
| 5.19 | Penetration rates in the first few years (µm/a) | Rural 0.3–0.5; marine 0.5–0.9; urban 0.9–1.3. Corrosion slows substantially once a film forms, "usually after the first ten years" | as 5.18 | meas | Verified **[secondary-citation]** | D2-S24 p. 53 (PDF p. 70) |
| 5.20 | Measured time-varying appearance exists for rust and patina (accelerated with chemical sprays) | 26 samples, 1,280 light/view directions per time step. Rusting Steel 1: 22 frames, mean interval 7.3 min. Rusting Steel 2: 35, 10.8 min. Cast Iron Rusting: 30, 13.9 min. Copper with Patina: 34, 31.6 min | Lab dome capture | meas | Verified [IMG] | D2-S23 Fig. 2, p. 3 |
| 5.21 | Copper patina sequence spans about 17.4 h of accelerated time | (34 − 1) × 31.6 min = 1,042.8 min ≈ 17.4 h. Fig. 2 gives the average interval *between* frames, so the span is 33 intervals, not 34 × 31.6 | — | calc | Verified source, **[own-arithmetic]** (mean interval between frames × (frames − 1)) | D2-S23 Fig. 2 |
| 5.22 | AAMA 2605 coil-coating weathering limits (10 years South Florida: ΔE ≤ 5, gloss retention ≥ 50%) | as listed by Grok | Architectural paint specification | std | **Unverified** (via Grok; paid spec, not opened) | AAMA 2605 |

## 6. Grok leads not confirmed (kept as leads only)

| # | Claim (as Grok gave it) | Why Unverified |
|---|---|---|
| 6.1 | Saccone, Landry, Chouinard (2019) meta-analysis, *Psychon Bull Rev* 26:1195–1212: SWI d = 1.82 (1.70–1.95); MWI d = 1.00 (0.74–1.26); visual-only SWI d = 1.00 | Springer page blocked (3 kB stub). Not opened |
| 6.2 | Ellis & Lederman (1993): visual volume gives a weaker SWI than haptic volume | Not opened |
| 6.3 | Flanagan & Beltzner (2000) *Nat Neurosci* 3:737–741: SWI persists after lift forces adapt | Not opened |
| 6.4 | Twardy & Bingham (2002), McConnell, Muchisky, Bingham (1998): event duration and trajectory as scale information | Not opened |
| 6.5 | O'Sullivan et al. (2003) and Reitsma & O'Sullivan (2009): collision angle and gap tolerances in physics animation | Not opened |
| 6.6 | Vicovaro & Burigana (2016): restitution wood 0.66, polystyrene 0.61, Plasticine 0.13 | Not opened (paywalled) |
| 6.7 | Dorsey & Hanrahan (1996) "Modeling and rendering of metallic patinas", SIGGRAPH 96: a demo of about 30 years of ageing | Project page not opened |
| 6.8 | Bellini, Kleiman, Cohen-Or (2016) "Time-varying weathering in texture space", *ACM TOG* 35(4) | Not opened. Method lead only |
| 6.9 | Horizon Zero Dawn machines [practitioner]: thin legs failed both hit-boxes and the read of weight; "should look like it could be built" | PlayStation Blog / PSX quotes not opened. The GDC 2018 talks exist (GDC Vault 1025040, Richard Oud; van Beek on Game Developer), but only the session blurbs were read, not the videos |
| 6.10 | Scott Robertson (*How to Render*) mech process [practitioner]: pivots and actuators before styling | Copyrighted book. Not opened and not to be quoted |

## 7. Conventions list (what the brief asked for), with the evidence behind each

Each convention gives the named example or measured row it rests on. "Practice" means no measured study was found.

**Heavy**
1. **Bigger reads heavier, but less than in proportion.** Expected mass grows more slowly than volume (2.1–2.5). A machine scaled up ×2 in each dimension (×8 volume) should look heavy but not "×8 heavy". Evidence: Peters 2015.
2. **Dense-looking materials read heavy.** Metal looks are expected to be heavier than wood or foam at the same size (2.7–2.8). Evidence: Buckingham 2009, 2013.
3. **Weight low and to the right.** Mass placed low reads heavier, and that is the stronger effect. Right reads heavier for most viewers (2.9–2.10). Evidence: Deng & Kahn 2009 (packaging, not machines).
4. **Darker reads heavier** (2.11–2.12). Evidence: Walker 2017 (abstract); Alexander & Shansky 1976 (abstract).
5. **Wide stance, no tippy direction.** Stability is judged by the weakest direction (2.13). One narrow axis makes the whole machine read unstable. Evidence: Cholewiak 2013 (abstract).
6. **Slow, long lifts and moves.** Lift duration is the main cue (3.2). Faster arm motion makes the load look lighter (3.3). Evidence: Hamilton 2007 (abstract); Zhou 2017.
7. **Real gravity, real time.** Floaty, low-gravity falls are spotted more easily than heavy ones (3.5–3.6). Fall timing tells the eye how big a thing is (3.7–3.8). A big machine whose parts drop at small-object speed reads as a toy. Evidence: Reitsma & Pollard 2003; Watson 1992; Hecht 1996.
8. **Collisions: the heavy body keeps going.** Mass is read from post-collision velocities (3.10–3.11). Evidence: Todd & Norman 2026.

**Light**
9. The opposite of 1–8: small, foam- or polymer-looking, high or top-placed mass, light colours, quick motion. An object that hangs in place after release reads light (3.4, Ujitoko 2023).

**Precise**
10. **Tight, even gaps.** The largest (D-class) cars use 3–3.5 mm with +1/0 to ±0.75 mm. The smallest (A-class) use 5 mm ±1.2–1.5 (5.4). Appeal drops from 4.6 mm (5.2). Changes under about 2 mm go unnoticed by most (5.2–5.3). Evidence: Pang 2024; Duraiswamy 2018.
11. **Small, consistent edge breaks.** ISO 2768 f/m allows ±0.2 mm on 0.5–3 mm chamfers and radii, against ±0.4 mm for c/v (§4). A precise part has small, uniform bevels. Evidence: ISO 2768-1 (a drawing default, not a perception study).
12. **Regular fastener pitch at engineering ratios.** Rivets at least 3 diameters apart and 2 from the edge (5.7). Double rows: inter-row 4D, pitch 4D (75% net). Triple/multiple: inter-row 3D, 3D/6D longitudinal (83% net) (5.8). Bolts at least 3 diameters apart, edges about 1.7 diameters for 7/8 in bolts, no more than about 7 in apart along sealed edges (5.9–5.11). Evidence: FAA AC 43.13-1B Fig. 4-5, p. 4-20; FHWA/AASHTO. The perception link is practice, not measured.

**Crude / rough-built**
13. Loose classes: ISO 2768 c/v (e.g. ±1 mm on 6–30 mm, ±8 mm over 2 m) (§4). Wide or uneven gaps (5.2, 5.4). Evidence: ISO 2768-1; Pang 2024.

**Old**
14. **Rust rate by place.** Bare steel loses from ≤ 1.3 µm in its first year (dry indoors, C1) to 200–700 µm (CX, extreme marine) (5.13). Use the corrosivity class of the setting to pace ageing, and slow it after year one (5.16). Evidence: ISO 9223.
15. **Copper goes brown or black first, then green over years.** Green at 6–7 years in towns and by the sea, still not green at 16 years in the country (5.18–5.19). Evidence: Holm & Mattsson 1982 via Scott 2002, p. 53 (PDF p. 70).
16. **Wear and rust start at edges and exposed high-curvature spots; dirt sits in crevices.** Practice, and a method in graphics (Gu 2006 shows controlling the rusting rate spatially). No perception study with numbers was found (§9).

## 8. Grok errors corrected

1. **Pang et al. 2024 panel-gap paper.** Grok gave the authors as "Wang et al.", the journal as *Eksploatacja i Niezawodność / Maintenance and Reliability*, and the DOI as 10.21595/jve.2024.23830. Crossref returns nothing for that DOI. The paper is **Pang, Huang, Chen**, *Vibroengineering Procedia* vol. 54 (2024), **DOI 10.21595/vp.2024.23830**. Grok's Table 1 numbers for the symmetric tolerances were right. Grok left out the asymmetric C and D bonnet tolerances (+1/0), which I read from the page image. Grok's "luxury vs economy" label is my reading too, but the paper's own labels are A–D class.
2. **Todd & Norman 2026 licence.** Grok said "CC-BY". The article is **CC BY-NC-ND 4.0** (Crossref record and the article's licence statement). It is flagged non-commercial, no derivatives.
3. **Gu et al. 2006.** Grok said the DOI was "unsure" and gave "~2 min between frames". The DOI is **10.1145/1179352.1141952**. About 2 minutes is the system's *minimum* time between measurements. Mean intervals for the corrosion samples are 7.3–31.6 min (Fig. 2).
4. **Reitsma & Pollard 2003.** Grok marked the threshold table unverified. I read it in the CMU author PDF, Table 2 (row 3.5).
5. **Runeson & Frykholm 1981.** Grok gave Exp. 1 n = 26 (ages 25–55). The abstract I could open gives no n, so that n is not used.
6. **ISO 2768-1.** Grok called its values a "secondary transcription". I read them from ISO's official preview page image. They match Grok's.

## 9. Not found

- **No measured study links panel-line density, fastener count, greebles or surface detail to perceived heaviness, age, precision or scale.** These stay practice (§7, items 11–12 and 16).
- **No measured study of overshoot, damping or settle time as cues to mass** for animated machines. The motion rows are about speed, duration, gravity and collisions (§3).
- **No perception study with numbers for wear location** (edges, crevices) against judged age.
- **Paint gloss and colour loss over years** from a source I could open (AAMA 2605 is paid; row 5.22 is Unverified).
- **ISO 286 IT grades and ISO 1302 roughness grades** from an official preview (rows 4.2–4.3 are Unverified).
- **AASHTO edge-distance table in full** (only the 7/8 in example was read, row 5.11).
- **Full texts blocked:** Cholewiak 2013 (PMC and ARVO behind a captcha, 403), Buckingham 2009, Buckingham 2014 review (Exeter repository 403), Saccone 2019 (Springer), Hamilton 2007 (Europe PMC full text returned 500).
- **Licence or terms for the STAF time-varying appearance data** (Gu 2006). The Columbia "about" page states none. The data were not downloaded.
- **Practitioner material:** the Horizon Zero Dawn GDC talks (Oud 2018; van Beek 2018) are videos; only the blurbs were read. Concept-art books (e.g. Robertson) are copyrighted and were not used.

## 10. Sources disagree

1. **Gap appeal against gap noticing.** Duraiswamy 2018 finds appeal changes need gaps about 2–3 mm apart (5.2), while Pang 2024's class references step by only 0.5–1 mm per class (5.4). They are not in direct conflict: OEM targets are set tighter than the size people rate differently. The 2018 sample was employees of one company, rating static CAD images.
2. **Felt weight against expected weight.** A light-looking material makes an equal-mass object *feel* heavier when lifted (MWI, 2.7), while the same look makes it *expected* to be lighter (2.8). For a game, which only shows objects, the expectation (2.8, 2.4) is the relevant one. The felt illusions (2.6–2.7) need lifting.
3. **Fall speed.** Ujitoko 2023 found no heaviness difference among fall accelerations from 0.25× to 4× g, but zero fall made objects look lighter (3.4). Reitsma & Pollard 2003 found wrong gravity detectable, especially low gravity (3.5–3.6). These are different questions (judged heaviness against judged correctness); both are kept.
4. **Right-is-heavier** holds for right-eyed viewers but not significantly for left-eyed ones (2.10).
5. **Hecht abstracts, 1994 vs 1996 (3.8).** The NASA NTRS 1994 abstract describes three experiments and says observers did better than chance but well below what the information allows. The 1996 *Perception & Psychophysics* abstract (doi 10.3758/BF03206833) reports five experiments and says observers were much more sensitive to average velocity than to gravitational acceleration. The two abstracts are not treated as the same text.

## 11. References (all accessed 2026-11-17)

Link check, 2026-11-17, `curl -L`: 200 for every URL below except these. **403 or 500 bot blocks** on publisher DOI landing pages: 10.1068/p210069, 10.1177/0301006616684369 (SAGE), 10.1167/13.4.12 (ARVO), 10.3758/BF03199388 (Springer). The papers exist (Crossref metadata checked). PubMed abstract pages return 203 with content. The FHWA ROSA P PDF returns 200 to plain curl but 403 when a browser user-agent is sent.

- **D2-S1** ISO 2768-1:1989 *General tolerances — Part 1: Tolerances for linear and angular dimensions without individual tolerance indications.* Official preview: https://cdn.standards.iteh.ai/samples/7748/62ad250fa883403897dfadc6f29c583b/ISO-2768-1-1989.pdf
- **D2-S2** ISO 9223:2012 *Corrosion of metals and alloys — Corrosivity of atmospheres — Classification, determination and estimation.* Official preview: https://cdn.standards.iteh.ai/samples/53499/e1f1aefb0a5446ac8308e3ddfce1db8b/ISO-9223-2012.pdf
- **D2-S3** FAA Advisory Circular 43.13-1B Change 1 (27 Sep 2001), par. 4-57 and Fig. 4-5 (p. 4-20). Official PDF: https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_43.13-1B_w-chg1.pdf. Par. 4-57 excerpt in NTSB docket: https://data.ntsb.gov/Docket/Document/docBLOB?FileExtension=.PDF&FileName=Excerpt+from+Advisory+Circular+%28AC%29+43.13-1B+Change+1+dated+September+27%2C+2001.-Master.PDF&ID=40266557
- **D2-S4** FHWA (2012) *Steel Bridge Design Handbook: Splice Design*, Vol. 14, FHWA-IF-12-052: https://rosap.ntl.bts.gov/view/dot/49753/dot_49753_DS1.pdf
- **D2-S5** Duraiswamy V., Campean F., Harris S., Munive-Hernandez J.E. (2018) Development of a methodology for robust evaluation of perceived quality of vehicle body panel gaps. DESIGN 2018 proceedings, pp. 237–248 (pages read 241–244). DOI 10.21278/idc.2018.0432. PDF: https://www.designsociety.org/download-publication/40444/development_of_a_methodology_for_robust_evaluation_of_perceived_quality_of_vehicle_body_panel_gaps
- **D2-S6** Pang F., Huang X., Chen C. (2024) Research on the influence of vehicle durability vibration on gap flush of exterior trim parts. *Vibroengineering Procedia* 54. DOI 10.21595/vp.2024.23830. PDF: https://www.extrica.com/article/23830/pdf
- **D2-S7** Peters M.A.K., Balzer J., Shams L. (2015) Smaller = denser, and the brain knows it. *PLoS ONE* 10(3):e0119794. https://doi.org/10.1371/journal.pone.0119794
- **D2-S8** Buckingham G., Goodale M.A. (2013) Size matters: a single representation underlies our perceptions of heaviness in the size-weight illusion. *PLoS ONE* 8(1):e54709. https://doi.org/10.1371/journal.pone.0054709
- **D2-S9** Buckingham G., Cant J.S., Goodale M.A. (2009) Living in a material world. *J Neurophysiol* 102(6):3111–3118. DOI 10.1152/jn.00515.2009. Abstract: https://pubmed.ncbi.nlm.nih.gov/19793879/
- **D2-S10** Deng X., Kahn B.E. (2009) Is your product on the right side? The "location effect" on perceived product heaviness and package evaluation. *J Marketing Research* 46(6):725–738. DOI 10.1509/jmkr.46.6.725. Manuscript: https://marketing.wharton.upenn.edu/wp-content/uploads/2016/10/the_location_effect_in_packaging_design.pdf
- **D2-S11** Walker P., Scallon G., Francis B. (2017) Cross-sensory correspondences: heaviness is dark and low-pitched. *Perception* 46(7):772–792. https://doi.org/10.1177/0301006616684369
- **D2-S12** Alexander K.R., Shansky M.S. (1976) Influence of hue, value, and chroma on the perceived heaviness of colors. *Perception & Psychophysics* 19(1):72–74. https://doi.org/10.3758/BF03199388. Official Springer abstract opened (full text not opened).
- **D2-S13** Cholewiak S.A., Fleming R.W., Singh M. (2013) Visual perception of the physical stability of asymmetric three-dimensional objects. *J Vision* 13(4):12. https://doi.org/10.1167/13.4.12
- **D2-S14** Runeson S., Frykholm G. (1981) Visual perception of lifted weight. *JEP:HPP* 7(4):733–740. DOI 10.1037/0096-1523.7.4.733. Abstract: https://pubmed.ncbi.nlm.nih.gov/6457088/
- **D2-S15** Hamilton A.F. de C., Joyce D.W., Flanagan J.R., Frith C.D., Wolpert D.M. (2007) Kinematic cues in perceptual weight judgement and their origins in box lifting. *Psychological Research* 71(1):13–21. DOI 10.1007/s00426-005-0032-4. Abstract (Europe PMC API): https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=PMCID:PMC2637436&resultType=core&format=json
- **D2-S16** Zhou A., Hadfield-Menell D., Nagabandi A., Dragan A.D. (2017) Expressive robot motion timing. HRI '17. DOI 10.1145/2909824.3020221. Author PDF: https://people.eecs.berkeley.edu/~anca/papers/HRI17_timing.pdf
- **D2-S17** Ujitoko Y., Kaneko S., Yokosaka T., Kawabe T. (2023) Falling and heaviness. *Frontiers in Psychology* 14:1042188. https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2023.1042188/full
- **D2-S18** Reitsma P.S.A., Pollard N.S. (2003) Perceptual metrics for character animation: sensitivity to errors in ballistic motion. *ACM TOG* 22(3):537–542. DOI 10.1145/882262.882304. Author PDF: https://www.cs.cmu.edu/~psar/research/metrics/metricsS2003.pdf
- **D2-S19** Hecht H., Kaiser M.K., Banks M.S. (1996) Gravitational acceleration as a cue for absolute size and distance? *Perception & Psychophysics* 58:1066–1075. DOI 10.3758/BF03206833 (1996 journal abstract). NASA NTRS record (1994 report abstract): https://ntrs.nasa.gov/citations/20020006041. The two abstracts disagree (see §10).
- **D2-S20** Watson J.S., Banks M.S., von Hofsten C., Royden C.S. (1992) Gravity as a monocular cue for perception of absolute distance and/or absolute size. *Perception* 21(1):69–76. https://doi.org/10.1068/p210069
- **D2-S21** Hamrick J.B., Battaglia P.W., Griffiths T.L., Tenenbaum J.B. (2016) Inferring mass in complex scenes by mental simulation. *Cognition* 157:61–76. DOI 10.1016/j.cognition.2016.08.012. Author PDF: https://cocosci.princeton.edu/papers/Hamrick_Cognition.pdf
- **D2-S22** Todd J.T., Norman J.F. (2026) The visual perception of relative mass from object collisions. *J Vision* 26(2):1. DOI 10.1167/jov.26.2.1. Full text (Europe PMC API, XML): https://www.ebi.ac.uk/europepmc/webservices/rest/PMC12875344/fullTextXML (CC BY-NC-ND 4.0)
- **D2-S23** Gu J., Tu C.-I., Ramamoorthi R., Belhumeur P., Matusik W., Nayar S. (2006) Time-varying surface appearance: acquisition, modeling and rendering. *ACM TOG* 25(3):762–771. DOI 10.1145/1179352.1141952. Author PDF: https://cave.cs.columbia.edu/Statics/publications/pdfs/Gu_TOG06.pdf. Data page: https://www.cs.columbia.edu/CAVE/databases/staf/STAF_about.html
- **D2-S24** Scott D.A. (2002) *Copper and Bronze in Art: Corrosion, Colorants, Conservation.* Getty Conservation Institute, p. 53 (PDF p. 70) (citing Holm R., Mattsson E. 1982). Publisher page: https://www.getty.edu/publications-reports/item/2420W0. Official Getty PDF: https://media.getty.edu/Text/a33da677-13c2-5f35-8954-7c4e1eef454b.pdf (PDF p. 70 is printed p. 53). The third-party flipbook at https://online.flipbuilder.com/mpdm/jbfy/files/basic-html/page70.html is flipbook page 70, which is the same printed p. 53; official printed p. 70 is a different passage.
- GDC Vault session blurb (practitioner, row 6.9): https://www.gdcvault.com/play/1025040/Animation-Bootcamp-Bringing-Life-to
