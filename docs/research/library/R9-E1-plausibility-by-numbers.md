# R9-E1 Is a generated body plausible? Checks by numbers alone

Round 9, section E. Access date for every source: **2026-11-28**. Everything is in my own words. Numbers are reported as facts with their location; **no dataset was taken** and no code was read. Reasoning marked **(mine)** is my own. Pages are **PDF pages** unless marked "printed".

**The brief asks for:** open work on judging a generated body by numbers alone, under four headings:
1. outline measures against anthropometric data;
2. symmetry;
3. volume against stated mass and density (round one's A4 has segment masses);
4. balance of the standing pose over the feet.

**Short answer.** No single open source does all four. Each heading has at least one measured source below. What is missing is a published "plausibility score" that combines them; section 6 is my own design for one, and it is labelled as mine throughout.

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| S1 | ISO 20685-1:2018, "3-D scanning methodologies for internationally compatible anthropometric databases, Part 1: Evaluation protocol for body dimensions extracted from 3-D body scans" | **No free full text.** A free preview (11 PDF pp.) is published by iTeh: catalogue page https://standards.iteh.ai/catalog/standards/sist/0964d367-fe6b-4d2f-b03b-ec81e434ce3f/iso-20685-1-2018 (the preview PDF's own download link was not logged; the catalogue URL is printed on every preview page) | © ISO 2018. Preview only; not open | Preview only: Scope, clause 3.5, clause 4.1-4.2, Table 4 (PDF p.7-11). **Clause 5 (study design, sample size, analysis) is beyond the preview** |
| S2 | Kudzia P., Jackson E., Dumas G. (2022). "Estimating body segment parameters from three-dimensional human body scans." *PLOS ONE* 17(1):e0262296. doi:10.1371/journal.pone.0262296 | https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0262296 | CC BY 4.0 (PDF p.1; Crossref) | Yes: abstract, methods, results, limitations (PDF p.1-12, 18-19). Their software exists; **not looked at (method only)** |
| S3 | Hakamada N., Funato K. (2012). "Estimation of center of gravity obtained from 3D whole body scanning anthropometry method." *Proc. 1st Asian Workshop on 3D Body Scanning Technologies*, Tokyo, pp. 64-70. doi:10.15221/A12.064 | https://proc.3dbody.tech/papers/A2012/a12064_04hakamada.pdf (the box copy is byte-identical to this URL, 985,507 bytes) | CC BY 4.0 (abstract page https://proc.3dbody.tech/abstracts/A2012/064hakamada.html: "Authors retain all rights to individual papers, which are licensed under a Creative Commons Attribution 4.0 International License") | Yes, all 7 pages. Tables 1, 5 and 6 read from page images. **Printed page = PDF page + 63** |
| S4 | Wuhrer S., Shu C. (2011). "Estimating 3D human shapes from measurements." arXiv:1109.1175. Journal version: *Machine Vision and Applications* 24(6):1133-1147 (2013), doi:10.1007/s00138-012-0472-y | https://arxiv.org/abs/1109.1175 | arXiv non-exclusive distribution licence (free to read, **not** an open licence). The journal version was not read | Yes (arXiv version): approach, parameters, experiments, Tables 1-4 (PDF p.1-17). Trained on CAESAR scans; **no data taken; method only** |
| S5 | Wu Y., Morris K., Liu X., Boisvert R., Wu H. (2025). "Analyzing body asymmetry using 3D scans and machine learning: insights from demographic patterns and dominant hand bias." *3DBODY.TECH Journal* 2, paper #30. doi:10.15221/25.30 | https://jrnl.3dbody.tech/papers/2025/2530.pdf | CC BY 4.0 (abstract page https://jrnl.3dbody.tech/abstracts/2025/2530.html: "Copyright © 2025 by the author(s). This work is licensed under a Creative Commons Attribution 4.0 International License") | Yes: abstract, methods, results 3.1-3.2, Figure 1 (page image), limitations (PDF p.1-10) |
| S6 | Fujiwara K., Asai H., Kiyota N., Mammadova A. (2010). "Relationship between quiet standing position and perceptibility of standing position in the anteroposterior direction." *Journal of Physiological Anthropology* 29(6):197-203. doi:10.2114/jpa2.29.197 | J-STAGE: https://www.jstage.jst.go.jp/article/jpa2/29/6/29_6_197/_pdf | Free to read on J-STAGE. **No licence statement found** (OpenAlex classes it "bronze", licence none) | Yes: abstract, methods, results, discussion (PDF p.1-6) |

Not used and why:
- Round one's A4 (segment masses) is cited by the brief; it is in the earlier library and is not repeated here.
- SCAPE, SMPL and Blanz-Vetter data are forbidden and nothing here depends on them. S4 trains on CAESAR scans (a licensed survey); the method is described, no data is used.

## 1. Outline measures against anthropometric data

### 1a. How close is "close enough"? The ISO tolerance table (S1)

- **What it is for** (S1 PDF p.5, p.7): measurements taken from a 3D scan should be comparable with the ISO 7250-1 measurements taken by hand. The standard sets how to judge whether a scanning system is good enough for that.
- **The reference** (S1 clause 3.5, PDF p.8; clause 4.2, PDF p.11): the "true value" is the value an anthropometrist with several years' experience of ISO 7250-1 gets with tape and calliper. The standard says plainly that the body cannot be held to machine-tool accuracy (PDF p.11).
- **Which measures suit which scanner** (S1 clause 4.1, Tables 1-3, PDF p.9-10): whole-body scanners may lack the resolution for small parts such as the hand, so the tables assign each ISO 7250-1 measure to whole-body, head, hand or foot scanners.
- **Table 4, "maximum allowable error between extracted value and traditionally measured value"** (S1 PDF p.11). The column is a **maximum mean difference**, in mm (it points to clause 5.4, which the preview does not include):

| Measurement type (S1's example) | Max mean difference, mm |
|---|---|
| Segment lengths (buttock-popliteal length) | 5 |
| Body heights (shoulder height) | 4 |
| Large circumferences (chest circumference) | 9 |
| Small circumferences (neck circumference) | 4 |
| Body breadths (biacromial breadth) | 4 |
| Body depths (chest depth) | 5 |
| Head dimensions without hair | 1 |
| Head dimensions with hair | 2 |
| Hand dimensions | 1 |
| Foot dimensions | 2 |

- If a measure's difference is below the Table 4 value it may go into ISO 15535 databases (PDF p.11).
- **What this does and does not give a generator (mine).** Table 4 is a tolerance on a *measuring system*, averaged over people, not a range a body must fall in. A generator can use it two ways: (a) as the precision to which its own measuring of a generated mesh must agree with a hand-measure definition; (b) as the smallest difference worth reporting when a generated body is compared to a requested measurement. It does not say whether a measurement is human-like; for that see 1b.
- **UNVERIFIED:** how Clause 5 sets sample size and computes the mean difference. Beyond the preview.

### 1b. Inside or outside the space of real bodies (S4)

- **The problem** (S4 PDF p.1-3): methods that build a body from measurements by learning a statistical shape space only make bodies inside that space. S4 says such methods usually stay within two standard deviations of the shape variation (PDF p.2), so local differences that the training set lacks cannot be produced.
- **Method, two steps** (S4 PDF p.5-7):
  1. Learn a linear map from measurements to PCA weights ("feature analysis"). For a new set of measurements, predict the weights, then **clamp each weight to at most l standard deviations** along its PCA axis (PDF p.7). The stated assumption is that the shape space is independent Gaussians and most bodies lie within l standard deviations. This step keeps the starting body human.
  2. Optimise inside the learned shape space to fit the measurements, then run a mesh-based optimisation that deforms the surface to meet the measurements under a smoothness term, with no body prior (PDF p.5-6). This lets local variation outside the training set appear.
- **The trade-off parameter** (S4 PDF p.10): l trades measurement accuracy against "likely human shapes". If the measurements are known to come from a real person, l can be high; if they are random, l must be low to avoid non-human shapes. S4 used **l = 10 for real data and l = 3 for synthetic data**, and smoothing weight λ = 0.1.
- **How they tested "far from the data"** (S4 PDF p.11): fit a Gaussian N(μ, Σ) to the training measurements. S_close is 200 samples from that Gaussian. S_k is 200 samples on an ellipsoid further out, with k = 2 and 4; larger k is further from the training set.
- **Results, mean absolute measurement error in mm** (S4 Table 2, PDF p.14; 360 CAESAR training bodies, 34 measurements, PDF p.13):

| Test set | Feature analysis | SGPLVM | S4's approach |
|---|---|---|---|
| S_close | 3.29 | 4.91 | 1.74 |
| S_2 | 14.26 | 12.19 | 6.39 |
| S_4 | 36.86 | 22.88 | 15.49 |
| S_real | 3.09 | 4.55 | 1.10 |

- **The failure that matters for plausibility** (S4 PDF p.12, p.14): with feature analysis alone, most S_4 bodies and some S_2 bodies come out *outside the space of human bodies*; for faces, S4 notes that people are sensitive to unrealistic face shape (PDF p.12). Errors grow fast as requests move away from the data in every method.
- On faces (Table 1, PDF p.12; 50 training faces, 7 measurements) feature analysis has the *lowest* mean error, but produces faces that are clearly not human for S_4. **Low measurement error and plausibility are different things** (S4's point, PDF p.12).
- With a small training set (35 typical male bodies, PDF p.16; l = 3 here because the test bodies are poorly covered by training) S4 predicted bodies similar to the ground truth for most test shapes (PDF p.17). **Stated failure:** one test body, far from the training data, came out smaller than the real one (PDF p.17). Table 4 (PDF p.17) gives per-measurement errors per subject, some above 25 mm.
- **What the method gives without the data (mine):** the test "is each shape weight within l standard deviations of its axis" is a plausibility check that needs only a shape model's per-axis spread, and S4 gives a reasoned choice of l (3 for made-up requests, 10 for measured people). The request-side test "how many standard deviations from the mean is this set of measurements" (their S_k construction) is a plausibility score on the request itself, before any mesh is made.

## 2. Symmetry (S5)

- **Method** (S5 PDF p.4-5): 245 people scanned (ages 6 to 75, a deliberately broad range). 22 left-right pairs of apparel measures (14 upper body, 8 lower). 16 pairs were taken automatically in AnthroScan; 6 (bust, across back, front full length, shoulder slope, waist, hip) by hand on the scan, by two trained researchers independently, with re-measurement where they disagreed.
- **Main result** (S5 §3.1, PDF p.7): paired t-tests found **no statistically significant mean left-right difference in any of the 22 pairs** at 0.05. S5 reads this as fluctuating asymmetry: individuals differ left to right, but around zero with no population direction.
- **Where asymmetry is largest** (PDF p.7): the smallest p-values were wrist circumference (0.09) and across back (0.14), then shoulder width (0.22), ankle circumference (0.24) and shoulder slope (0.27). Shoulder slope and shoulder width show the widest spread in Figure 1 (PDF p.8); inseam and outseam length the least. The upper body is more asymmetric than the lower body, in line with the literature they cite.
- **Direction** (PDF p.7): left-side upper-body measures tended to be larger (8 of 14 pairs) and right-side lower-body measures larger (5 of 8 pairs).
- **Disagreement with earlier work** (PDF p.7): S5 notes this is the opposite of the cross pattern reported before (right side larger in the upper body, left in the lower), and suggests its more racially diverse sample as a reason. Both views stand; the earlier studies (their refs 14 and 23) were not read.
- **UNVERIFIED: magnitudes.** Figure 1 (PDF p.8) is a box plot of right-minus-left differences, but I could not find the unit of its axis (about -0.4 to 0.5) in the caption or text. **No number for "normal" asymmetry is given here.** A substitute: the search result for Rodríguez et al. 2021 (PMC8615123, cyclists) quotes leg length 3.04 mm and calf girth 5.25 mm differences, **not read; UNVERIFIED and not used**.
- **Limits** (PDF p.10): sample size and demographic spread; a larger stratified sample is needed.
- **What a generator can take (mine):** an exactly mirror-symmetric body is not "wrong" by population means (S5 found no mean difference), but individuals are not mirror images, and the variation is largest at the shoulders and wrist and smallest in leg length. A symmetry check should flag *large* left-right differences in leg length first, and be most tolerant at shoulder slope and width. The size of "large" is **not supplied by any source read**; Table 4 of S1 (4-5 mm for lengths, heights and breadths) is the nearest measured floor, because a difference smaller than that cannot be measured reliably anyway.

## 3. Volume against stated mass and density (S2, S3)

### 3a. Mass from scanned volume (S2)

- **Method** (S2 PDF p.1, p.6-7): 21 adults (10 men, 11 women; Table 1, PDF p.4). Each body scanned with Kinect V2 cameras, cut by hand into 16 segments (head, torso, abdomen, pelvis; left and right hand, forearm, upper arm, thigh, shank, foot), each segment's volume taken as the space its point cloud encloses, and mass = volume × a **uniform density per segment** from the literature (cadaver values and CT trunk values; the values are in their S2 File, not read). Total mass = sum of the 16. The segment's centre of mass was taken as the geometric centre of its volume.
- **Checking the method on a known object** (PDF p.9): on a beam of known shape the scan method overestimated volume by +4.2%, underestimated length by about 0.6% and centre-of-mass position by 0.3%.
- **Repeatability** (PDF p.11): volume differed little across repeated scans (CV 1.8% men, 1.6% women); ICC(2,1) 0.96 men, 0.99 women.
- **Mass against the scale** (PDF p.11):
  - Men: scan method 74.5 ± 4.5 kg, elliptical-cylinder method 73.8 ± 4.7 kg, scale 73.4 ± 4.5 kg; no significant difference (p = 0.8529).
  - Women: 73.2 ± 8.0 kg, 70.7 ± 8.1 kg, 70.4 ± 8.1 kg; no significant difference (p = 0.6339).
  - Bland-Altman against the scale: men, limits 2.8 to -5.0 kg, bias -1.1 kg (-1.5%); women, limits 0.64 to -6.9 kg, bias -3.1 kg (-4.4%).
- **Internal inconsistency (flagged):** S2 says a positive bias means the scan overestimates (PDF p.10). Yet the scan means are *higher* than the scale (74.5 > 73.4; 73.2 > 70.4) while the reported biases are negative. The likeliest reading is that the difference was computed as scale minus scan; **UNVERIFIED**. Either way the size, about 1 to 3 kg, stands.
- **Density choice** (PDF p.10, p.12): they repeated the agreement test with one density, **1000 kg/m³ (water) for every segment**; S2 says this led to the same conclusions. The numbers for that run are not given in the text I read.
- **Limits** (PDF p.12, p.18): segmenting took about 30-40 minutes per scan; segmenting was most reliable for the larger segments (PDF p.12); scanning closer than 0.5 m is error-prone; curved, concave and sharp surfaces are smoothed; the face shows occlusion; the gap between the breasts may be overestimated.

### 3b. Mass and centre of gravity from scanned volume (S3)

- **Method** (S3 PDF p.1-4, printed p.64-67): 6 men (22.3 ± 1.1 years, 172.5 ± 5.9 cm, 67.2 ± 2.5 kg). Whole-body laser scan (Hamamatsu Body Line Scanner, 2.5 mm vertical and 1 mm horizontal spacing, 400,000-700,000 points, 5-10 s). Body cut into 14 segments by Clauser (1969) definitions using 72 landmarks. Each segment's mass = volume × Clauser's density, assumed uniform.
- **Clauser's segment densities** (S3 Table 1, PDF p.3, kg per litre): head 1.070, trunk 1.019, upper arm 1.056, forearm 1.098, hand 1.109, thigh 1.044, calf 1.085, foot 1.084.
- S3 states plainly that uniform density means the point found is the centre of **volume**, not the true centre of mass (PDF p.3).
- **Mass check** (PDF p.1): whole-body mass from the scan differed from body weight by **under 1.5%**.
- Segment results (PDF p.5): trunk mass by scan was significantly lower than Clauser's; thigh mass higher. The trunk's centre of volume sat about 6% lower than Clauser's centre of mass.

### 3c. Where S2 and S3 differ
- S2 finds the scan **slightly heavier** than the scale on average (means, PDF p.11); S3 reports a difference under 1.5% without a direction in the text (PDF p.1). Different scanners, segmentations and densities; both views stand.
- S2 reports one uniform water density gives the same conclusions; S3 attributes its trunk error to uniform density. These are not in direct conflict (S2 judges total mass, S3 judges centre position), but they pull in different directions on whether per-segment density matters.

**What a generator can take (mine):** mass = Σ segment volume × segment density is the check. With Clauser's densities (1.02-1.11 kg/L) or plain water (1.00 kg/L), a generated body whose stated mass and computed mass differ by more than a few per cent is suspect; the measured agreements are about 1.5% (S3) and 1.5-4.4% bias (S2). This works only if the mesh is closed and its segments are cut consistently.

## 4. Balance of the standing pose over the feet (S3, S6)

### 4a. Height of the centre of gravity (S3)
- **Measured reference**: a reaction board, with the subject lying on it in the scan posture (S3 PDF p.4). The board is 180 × 91.5 × 2.5 cm (abstract, PDF p.1). The abstract says it sits on "a scale"; the methods say two scales with a pivot at each side, and the COG comes from the ratio of the two readings (PDF p.4). The methods account is the fuller one.
- **Results** (S3 Tables 5 and 6, PDF p.6, printed p.69):

| Method | COG height, cm | COG as % of body height |
|---|---|---|
| Scan (BLS) | 94.4 ± 3.0 | 54.9 ± 0.5 |
| Reaction board (measured) | 97.2 ± 3.6 | 56.6 ± 0.5 |
| Clauser 1969 prediction | 100.1 ± 3.5 | 58.3 ± 0.6 |

  All three differ significantly (p < 0.001). The scan put the COG **lower** than measured for every subject; Clauser's prediction put it **higher**.
- **A wording conflict inside S3:** the abstract and conclusion say the scan agrees "within 2.9%" (PDF p.1, p.7), but 2.9% is the **mean** of the per-subject differences; Table 5 shows 2.1% to **4.4%** (subject 4). Report as "mean 2.9 ± 0.9%, maximum 4.4%".
- **Why it errs** (PDF p.6): uniform density per segment, especially in the trunk with its mix of bone, muscle and fat; and possible differences in subjects' build, race or age from Clauser's cadavers.

### 4b. Where the weight sits over the foot (S6)
- **Method** (S6 PDF p.2-3): 36 young adults (15 men, 21 women; mean height 164.8 cm, foot length 24.1 cm). A force platform measured the centre of pressure front-to-back, written as a percentage of foot length from the back of the heel (%FL).
- **Results** (S6 PDF p.4):
  - Quiet standing: **mean 44.5 %FL (SD 7.7), range 31.4-57.7 %FL**, on average in front of the ankle joint and behind the head of the fifth metatarsal.
  - Leaning as far forward as possible: 82.4 %FL (SD 1.5). As far back: 18.6 %FL (SD 1.9).
- **Context** (S6 PDF p.2): behind about 30 %FL or in front of about 60 %FL, leg-muscle activity and foot pressure change considerably (citing earlier work); In the discussion S6 puts the back end of the stable standing range at about 30 %FL, about 10 %FL ahead of extreme backward lean, and the front end about 20 %FL behind extreme forward lean (PDF p.6). So the backward margin is the smaller one.
- **The link to a generated body (mine, standard statics, not from S6):** for a body standing still, the centre of pressure and the ground projection of the centre of mass coincide on average. So a generated standing pose can be checked by computing the centre of mass (section 3) and dropping it to the floor: inside the outline of the feet is the hard limit; inside roughly 30-60% of foot length from the heel matches people standing quietly (S6); 18.6-82.4% is the measured edge of leaning.
- **Not found:** a measured side-to-side figure, and any source on balance for animals. Gap.

## 5. Where the sources disagree
- **Scan versus classic tables** for COG height: S3 scan 54.9% of height against Clauser 58.3% and measured 56.6%. Each method's bias has a different sign.
- **Mass agreement** S2 (bias 1.1-3.1 kg, means above scale) against S3 (under 1.5%).
- **Plausibility versus accuracy** (S4): a method can have the lowest measurement error and still make faces that are not human (PDF p.12). Any check that uses only measurement error is not a plausibility check.

## 6. A numbers-only check for a generated body (mine; design, not from any source)
Run in order; each step uses only the mesh, the request and published numbers.
1. **Request check.** How far is the request from the population, in standard deviations (S4's S_k idea)? Beyond about 3 is extrapolation; warn, as S4 does with l = 3 for made-up requests.
2. **Measure-back check.** Measure the generated mesh by the same definitions as the request; differences beyond S1 Table 4 (for example 9 mm for chest circumference, 4 mm for heights) mean the generator missed.
3. **Shape-space check** (only if a shape model exists): each weight within l standard deviations.
4. **Mass check.** Σ volume × density against the stated mass; agreement of a few per cent is what scans of real people achieve (S2, S3).
5. **Symmetry check.** Left-right differences reported per measure; the tolerance itself is **unsourced** (section 2).
6. **Balance check.** Centre of mass dropped to the floor; inside the foot outline, and ideally 30-60 %FL (S6).
7. **COG height check.** About 55-58% of stature for adult men by these sources (S3); a value far outside says segment masses are wrong. Only men were measured.

## 7. Gaps
- ISO 20685-1 clause 5 (how the mean difference is estimated, sample size). Paywalled beyond the preview.
- A sourced tolerance for left-right asymmetry in mm (S5's figure has no unit I could find).
- Side-to-side balance figures; balance for animals.
- The density values in S2's supplementary file, and S2's numbers for the water-density run.
- A published combined plausibility score. None found; section 6 is mine.
