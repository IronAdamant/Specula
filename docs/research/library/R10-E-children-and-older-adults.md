# R10-E. Children 2-17 and adults over 60, with spreads

Round 10, section E. Access date 2026-11-28. Method and facts only. Numbers were retyped from the copies named below (the NHANES rows were copied from the report text and each row was checked back against it). "(mine)" marks my own arithmetic. Several sections refer back to R10-A, R10-B and R10-C rather than repeating values.

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| E-S1 | Fryar CD, Gu Q, Ogden CL (2012). Anthropometric reference data for children and adults: United States, 2007-2010. Vital Health Stat 11(252) (same as C-S11) | cdc.gov/nchs/data/series/sr_11/sr11_252.pdf | Public domain (stated in the report) | Tables 1-14 yes; later tables not reached |
| E-S2 | CDC Normal Joint Range of Motion Study, reference values (same as C-S1) | archive.cdc.gov/www_cdc_gov/ncbddd/jointrom/index.html | Public domain | Yes |
| E-S3 | Hara R et al. (2016). Predicting the location of the hip joint centres, impact of age group and sex. Sci Rep 6:37707 (same as C-S7) | Publisher PDF and Supplementary PDF | CC BY 4.0 | Yes, Supplementary Table 1 |
| E-S4 | Kelso A, Vogel K, Steinacker JM (2019). Ultrasound measurements of subcutaneous adipose tissue thickness show sexual dimorphism in children of three to five years of age. Acta Paediatr 108(3):514-521. doi:10.1111/apa.14496 | PMC6585817 | CC BY-NC 4.0 | Yes, full text XML (Table 1) |
| E-S5 | Asai Y et al. (2017). Sagittal spino-pelvic alignment in adults: the Wakayama Spine Study. PLoS ONE 12(6):e0178697 (R10-A S9) | Publisher PDF | CC BY 4.0 | Yes, Tables 2 and 3 |
| E-S6 | Rajabi R, Plandowska M, Bayattork M (2024). Lumbar lordosis surface norms, 2,500 Iranians. Asian Spine J 18(6):836-845 (R10-A S10) | Publisher PDF | CC BY-NC 4.0 | Yes, Table 2 |
| E-S7 | Campa F et al. (2025). PLoS ONE 20(6):e0326111, Supplementary S1 and S2 (same as C-S8) | PMC12200776 | CC BY 4.0 | Yes |
| E-S8 | Wu D et al. (2015). Hertel exophthalmometry, Shenyang. Sci Rep 5:8526 (R10-B B-S2) | Publisher PDF | CC BY 4.0 | Yes |

## E1. Stature and body mass by single year of age, 2-17 (E-S1, Tables 7 and 1)

US NHANES 2007-2010, nationally representative sample of the civilian non-institutionalised population, measured by trained examiners; age is age at examination. The report gives the standard error of the mean, not the SD. "Approx SD (mine)" is (P95 - P5) / 3.29, or (P90 - P10) / 2.563 where the report prints a dagger in place of the percentile (footnote: "Standard error not calculated by SUDAAN"). For weight the distribution is right-skewed, so this SD is only a rough width; use the percentiles or a log-normal (R10-MATH).

Why not SE x sqrt(n): NHANES is a weighted multistage sample, so the design effect makes SE x sqrt(n) a poor SD estimate (mine).

**Stature, cm (Table 7)**

| Sex | Age | n | Mean | SE of mean | P5 | P50 | P95 | Approx SD (mine) |
|---|---|---|---|---|---|---|---|---|
| M | 2 | 285 | 91.8 | 0.38 | 84.1 | 92.0 | 99.5 | 4.7 |
| M | 3 | 202 | 98.9 | 0.36 | 91.1 | 98.8 | 105.8 | 4.5 |
| M | 4 | 244 | 106.2 | 0.43 | 97.8 | 106.0 | 114.9 | 5.2 |
| M | 5 | 205 | 113.7 | 0.55 | 104.2 | 113.1 | † | 5.9 (P10-P90) |
| M | 6 | 193 | 119.3 | 0.45 | 108.6 | 119.5 | 128.2 | 6.0 |
| M | 7 | 215 | 125.4 | 0.43 | 117.1 | 124.8 | 136.5 | 5.9 |
| M | 8 | 210 | 131.6 | 0.66 | 122.0 | 131.4 | † | 6.0 (P10-P90) |
| M | 9 | 190 | 137.9 | 0.88 | 123.4 | 136.7 | 151.2 | 8.4 |
| M | 10 | 197 | 142.3 | 0.64 | 131.7 | 142.5 | 152.8 | 6.4 |
| M | 11 | 211 | 149.9 | 0.62 | 138.8 | 149.3 | 163.1 | 7.4 |
| M | 12 | 159 | 154.6 | 0.61 | 143.1 | 153.4 | 168.2 | 7.6 |
| M | 13 | 146 | 163.7 | 0.68 | 147.7 | 164.5 | † | 9.2 (P10-P90) |
| M | 14 | 177 | 168.5 | 0.94 | 154.9 | 169.5 | † | 7.8 (P10-P90) |
| M | 15 | 160 | 173.8 | 0.88 | 160.3 | 173.7 | 187.8 | 8.4 |
| M | 16 | 175 | 175.1 | 0.50 | 163.3 | 175.4 | 186.3 | 7.0 |
| M | 17 | 188 | 175.9 | 0.69 | 162.6 | 176.1 | † | 7.8 (P10-P90) |
| F | 2 | 257 | 90.4 | 0.41 | 84.7 | 90.0 | 97.6 | 3.9 |
| F | 3 | 189 | 98.7 | 0.39 | 90.9 | 99.0 | 106.9 | 4.9 |
| F | 4 | 200 | 105.0 | 0.46 | 98.0 | 104.7 | 112.6 | 4.4 |
| F | 5 | 177 | 112.6 | 0.45 | 104.0 | 112.0 | 122.1 | 5.5 |
| F | 6 | 177 | 119.2 | 0.52 | 111.0 | 118.8 | 128.9 | 5.4 |
| F | 7 | 207 | 124.6 | 0.53 | 113.5 | 125.1 | 134.2 | 6.3 |
| F | 8 | 203 | 131.3 | 0.84 | 121.2 | 130.7 | † | 6.7 (P10-P90) |
| F | 9 | 205 | 137.0 | 0.47 | 127.6 | 136.7 | 148.3 | 6.3 |
| F | 10 | 183 | 144.5 | 0.52 | 133.9 | 144.5 | 155.4 | 6.5 |
| F | 11 | 219 | 150.4 | 0.54 | 137.5 | 150.7 | 161.6 | 7.3 |
| F | 12 | 166 | 156.1 | 0.64 | 142.9 | 156.7 | 166.3 | 7.1 |
| F | 13 | 140 | 160.0 | 0.55 | 151.0 | 159.5 | 170.4 | 5.9 |
| F | 14 | 168 | 161.6 | 0.76 | † | 161.9 | 174.7 | 6.7 (P10-P90) |
| F | 15 | 137 | 162.9 | 0.78 | 150.9 | 161.7 | 176.2 | 7.7 |
| F | 16 | 156 | 162.2 | 0.64 | 151.9 | 161.4 | 171.6 | 6.0 |
| F | 17 | 143 | 163.1 | 0.62 | 150.0 | 164.0 | 174.7 | 7.5 |

**Body mass, kg (Table 1)**

| Sex | Age | n | Mean | SE of mean | P5 | P50 | P95 | Approx SD (mine) |
|---|---|---|---|---|---|---|---|---|
| M | 2 | 325 | 14.0 | 0.14 | 11.3 | 13.9 | 17.3 | 1.8 |
| M | 3 | 203 | 16.2 | 0.21 | 13.1 | 15.9 | 20.2 | 2.2 |
| M | 4 | 244 | 18.5 | 0.22 | 14.9 | 17.7 | 25.5 | 3.2 |
| M | 5 | 205 | 21.2 | 0.32 | 16.3 | 20.2 | 31.0 | 4.5 |
| M | 6 | 193 | 24.3 | 0.47 | 16.8 | 23.1 | 36.5 | 6.0 |
| M | 7 | 216 | 26.7 | 0.43 | 19.7 | 24.9 | 39.5 | 6.0 |
| M | 8 | 211 | 31.3 | 0.70 | 21.6 | 29.8 | 46.5 | 7.6 |
| M | 9 | 191 | 36.6 | 1.17 | 22.8 | 33.0 | † | 10.9 (P10-P90) |
| M | 10 | 197 | 40.0 | 0.93 | 25.8 | 38.0 | 59.7 | 10.3 |
| M | 11 | 211 | 46.6 | 1.13 | 31.5 | 42.4 | 73.5 | 12.8 |
| M | 12 | 159 | 51.5 | 1.27 | 31.3 | 49.2 | 75.1 | 13.3 |
| M | 13 | 146 | 59.2 | 1.45 | 38.0 | 56.6 | 95.8 | 17.6 |
| M | 14 | 177 | 63.9 | 1.92 | 40.2 | 60.4 | † | 18.2 (P10-P90) |
| M | 15 | 160 | 70.1 | 1.95 | 48.4 | 66.0 | 115.3 | 20.3 |
| M | 16 | 175 | 75.1 | 1.52 | 52.0 | 70.7 | 108.9 | 17.3 |
| M | 17 | 188 | 77.4 | 2.41 | 52.3 | 71.0 | 128.5 | 23.2 |
| F | 2 | 282 | 13.4 | 0.15 | 10.9 | 13.2 | 16.5 | 1.7 |
| F | 3 | 191 | 15.7 | 0.24 | 12.3 | 15.4 | 19.7 | 2.2 |
| F | 4 | 200 | 17.7 | 0.21 | 14.2 | 17.1 | 23.1 | 2.7 |
| F | 5 | 177 | 21.1 | 0.54 | 15.9 | 19.7 | † | 3.8 (P10-P90) |
| F | 6 | 177 | 23.6 | 0.47 | 18.3 | 22.5 | 33.5 | 4.6 |
| F | 7 | 207 | 26.8 | 0.53 | 19.4 | 25.2 | 39.6 | 6.1 |
| F | 8 | 203 | 31.9 | 1.01 | 21.8 | 29.2 | 48.0 | 8.0 |
| F | 9 | 205 | 35.5 | 0.98 | 23.3 | 31.9 | 55.7 | 9.8 |
| F | 10 | 183 | 41.1 | 0.74 | 27.3 | 39.0 | 61.3 | 10.3 |
| F | 11 | 219 | 47.5 | 1.28 | 30.0 | 43.3 | 79.6 | 15.1 |
| F | 12 | 166 | 52.3 | 1.26 | 30.6 | 51.4 | 75.7 | 13.7 |
| F | 13 | 140 | 56.8 | 1.41 | 38.8 | 52.3 | 83.2 | 13.5 |
| F | 14 | 168 | 61.6 | 1.13 | 42.7 | 59.0 | † | 12.3 (P10-P90) |
| F | 15 | 137 | 63.3 | 1.32 | 44.2 | 59.5 | 92.3 | 14.6 |
| F | 16 | 156 | 62.4 | 1.11 | 45.5 | 58.7 | 92.0 | 14.1 |
| F | 17 | 143 | 63.7 | 1.56 | 45.4 | 60.8 | 92.0 | 14.2 |

## E2. Adults 60 and over, stature and mass (E-S1, Tables 9, 11, 3, 5; all race and ethnic groups)

**Stature, cm**

| Sex | Age | n | Mean | SE of mean | P5 | P50 | P95 | Approx SD (mine) |
|---|---|---|---|---|---|---|---|---|
| F | 60-69 | 952 | 161.6 | 0.26 | 150.5 | 162.0 | 171.7 | 6.4 |
| F | 70-79 | 679 | 159.1 | 0.32 | 148.2 | 159.4 | 169.8 | 6.6 |
| F | 80+ | 398 | 155.9 | 0.36 | 144.6 | 156.1 | 166.3 | 6.6 |
| M | 60-69 | 932 | 174.9 | 0.25 | 162.2 | 175.3 | 187.0 | 7.5 |
| M | 70-79 | 646 | 173.2 | 0.34 | 161.7 | 173.1 | 184.7 | 7.0 |
| M | 80+ | 354 | 170.7 | 0.39 | 158.7 | 170.9 | 181.6 | 7.0 |

**Body mass, kg**

| Sex | Age | n | Mean | SE of mean | P5 | P50 | P95 | Approx SD (mine) |
|---|---|---|---|---|---|---|---|---|
| F | 60-69 | 951 | 77.4 | 0.65 | 52.8 | 75.2 | 109.5 | 17.2 |
| F | 70-79 | 679 | 74.8 | 0.69 | 49.9 | 72.3 | 109.2 | 18.0 |
| F | 80+ | 397 | 64.9 | 0.73 | 45.4 | 63.5 | 87.4 | 12.8 |
| M | 60-69 | 933 | 90.5 | 0.76 | 61.7 | 88.5 | 127.4 | 20.0 |
| M | 70-79 | 649 | 86.5 | 0.88 | 62.6 | 84.7 | 117.9 | 16.8 |
| M | 80+ | 360 | 79.3 | 0.81 | 57.6 | 77.9 | 104.5 | 14.3 |

Reading (mine): mean stature falls by about 5.7 cm (women) and 4.2 cm (men) from 60-69 to 80+ in this cross-section; part of that is a birth-cohort effect, not shrinkage alone.

## E3. Range of motion, children (E-S2)

Means (95% CI), degrees. No SDs are published on the CDC page for children, and I found no secondary SD table for these bands.

| Motion | F 2-8 | M 2-8 | F 9-19 | M 9-19 |
|---|---|---|---|---|
| Hip flexion | 140.8 (139.2-142.4) | 131.1 (129.4-132.8) | 134.9 (133.0-136.8) | 135.2 (133.0-137.4) |
| Hip extension | 26.2 (23.9-28.5) | 28.3 (27.2-29.4) | 20.5 (18.6-22.4) | 18.2 (16.6-19.8) |
| Knee flexion | 152.6 (151.2-154.0) | 147.8 (146.6-149.0) | 142.3 (140.8-143.8) | 142.2 (140.4-144.0) |
| Knee extension | 5.4 (3.9-6.9) | 1.6 (0.9-2.3) | 2.4 (1.5-3.3) | 1.8 (0.9-2.7) |
| Ankle dorsiflexion | 24.8 (22.5-27.1) | 22.8 (21.3-24.3) | 17.3 (15.6-19.0) | 16.3 (14.9-17.7) |
| Ankle plantar flexion | 67.1 (64.8-69.4) | 55.8 (54.4-57.2) | 57.3 (54.8-59.8) | 52.8 (50.8-54.8) |
| Shoulder flexion | 178.6 (176.9-180.3) | 177.8 (176.7-178.9) | 171.8 (169.8-173.8) | 170.9 (169.1-172.7) |
| Elbow flexion | 152.9 (151.5-154.3) | 151.4 (150.8-152.0) | 149.7 (148.5-150.9) | 148.3 (146.8-149.8) |
| Elbow extension | 6.8 (5.2-8.4) | 2.2 (0.9-3.5) | 6.4 (4.7-8.1) | 5.3 (3.6-7.0) |
| Pronation | 84.6 (82.8-86.4) | 79.6 (78.8-80.4) | 81.2 (79.6-82.8) | 79.8 (77.8-81.8) |
| Supination | 93.7 (91.4-96.0) | 86.4 (85.3-87.5) | 90.0 (88.0-92.0) | 87.8 (85.7-89.9) |

Adults 45-69 are in R10-C (C1). The CDC sample stops at 69; nothing measured here for 70+.

## E4. Hip joint centre, children 5-11 (E-S3, Supplementary Table 1)

Mean (SD), mm, origin at the mid-ASIS point (see R10-C, C4):

| Group | n | Age | Height cm | HJC X | HJC Y | HJC Z | Inter-ASIS | Leg length |
|---|---|---|---|---|---|---|---|---|
| Girls 5-11 | 13 | 9 (2) | 135 (13) | -32 (5) | 62 (7) | -60 (6) | 177 (22) | 682 (72) |
| Boys 5-11 | 24 | 7 (2) | 123 (19) | -25 (8) | 59 (7) | -56 (7) | 167 (23) | 606 (102) |

The leg-length equations in R10-C (C4) were fitted across children and adults together, and age did not need its own term.

## E5. Fat in young children (E-S4, Table 1)

274 German preschool children aged 3-5 (144 boys, 130 girls), ultrasound at eight sites. Mean (SD) and range:

| Measure | Boys | Girls |
|---|---|---|
| Height, cm | 108.8 (6.7) | 107.7 (6.6) |
| Body mass, kg | 18.60 (3.14) | 18.09 (3.10) |
| Waist, cm | 51.9 (4.1) | 51.4 (4.0) |
| Mean SAT thickness incl. fibrous structures, mm | 5.3 (2.0), 2.4 to 12.3 | 6.3 (2.0), 2.4 to 15.3 |
| Sum of SAT, 8 sites, incl., mm | 42.6 (15.8), 19.0 to 98.2 | 50.8 (15.9), 19.2 to 122.0 |

This is ultrasound tissue thickness, not caliper skinfold (a skinfold is a double layer plus skin). Do not mix the two (mine). Caliper skinfolds for children (NHANES Tables 27 and 30) were not reachable this round.

## E6. Spine and pelvis, adults over 60 (E-S5, Tables 2 and 3)

Wakayama, Japan, standing radiographs. Mean (SD), degrees (SVA in mm). The youngest and oldest row labels lost their signs in my copy; they are the under-50 and 80-and-over groups (mine).

| Age | SVA men | SVA women | TK men | TK women | LL men | LL women | PT men | PT women | PI men | PI women |
|---|---|---|---|---|---|---|---|---|---|---|
| 60-69 | 8.2 (35.0) | 1.8 (32.7) | 37.8 (9.1) | 36.2 (11.4) | 45.2 (11.8) | 46.9 (12.8) | 15.5 (6.8) | 18.1 (8.5) | 48.3 (9.9) | 50.6 (10.4) |
| 70-79 | 13.9 (39.2) | 24.2 (43.1) | 41.6 (11.8) | 38.9 (12.7) | 45.7 (13.4) | 43.6 (14.2) | 16.0 (7.5) | 23.0 (10.3) | 47.5 (9.3) | 52.7 (10.7) |
| 80+ | 39.1 (54.3) | 51.6 (57.9) | 40.3 (12.3) | 44.2 (17.8) | 39.3 (15.9) | 38.6 (19.1) | 19.7 (8.4) | 25.2 (10.4) | 48.1 (10.9) | 51.6 (11.4) |

Reading (mine): the SD of SVA grows with age (about 35 to 55 mm), so older posture needs a wider spread, not just a shifted mean. PI barely changes with age (it is a fixed bony shape), while PT rises; since PI = PT + SS, SS falls.

**Surface lumbar lordosis (E-S6, Table 2), degrees, mean (SD):**

| Age | Women | Men |
|---|---|---|
| 10 and under (label printed as ">10"; mine) | 35.068 (10.3195), n 66 | 36.422 (10.6137), n 89 |
| 11-19 | 43.282 (12.4116), n 352 | 36.069 (11.2317), n 382 |
| 60-69 | 48.335 (13.9107), n 150 | 37.949 (9.5091), n 147 |
| 70 and over (label printed as "<70"; mine) | 45.170 (16.6451), n 126 | 36.900 (12.1100), n 105 |

Children's radiographic sagittal values (Mac-Thiong) are in R10-A.

## E7. Other measures already in this round

- Skinfolds, adults over 60, seven sites by sex: R10-C (C5), Campa >60 rows (91 women, 94 men).
- Hertel exophthalmometry for children 8-14 (n 515), teenagers 15-19 (n 517) and elderly 70-87 (n 396): R10-B (B1).
- ROM 45-69 with secondary SDs: R10-C (C1).

## Gaps

- Child caliper skinfolds with SDs by age (NHANES Tables 27-32) not reached.
- Child ROM SDs not found; only CIs.
- No ROM data for 70+.
- Limb segment lengths for children (NHANES upper arm and upper leg length, Tables 24 and 33) not reached in my text copy.
- Over-60 joint angles at rest beyond the spine (knee, foot) were not found in an open source this round.
