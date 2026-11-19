# A15 — Skinfolds by site, sex, age and build

Millimetres. Accessed 2026-11-19. All statistics here are **computed by me** from US public-use survey files (US government data, public domain), using the survey's examination weights; they are weighted means and weighted percentiles, not design-corrected standard errors.

## 1. NHANES III (1988–94): four sites

**Source.** CDC/NCHS, NHANES III Examination Data File, https://wwwn.cdc.gov/nchs/data/nhanes3/1a/exam.dat with layout https://wwwn.cdc.gov/nchs/data/nhanes3/1a/exam.sas (columns: BMPTRI 1609–1612 "Triceps skinfold (mm)", BMPSUB 1613–1616 "Subscapular skinfold (mm)", BMPSUP 1617–1620 "Suprailiac skinfold (mm)", BMPTHI 1621–1624 "Thigh skinfold (mm)"; weight WTPFEX6 59–67; sex HSSEX 15; age HSAGEIR 16–17; BMI BMPBMI 1524–1527; pregnancy MAPF12R 5151).
**Who.** US civilian non-institutional population, examined 1988–94; adults aged 20+ only; pregnant women (MAPF12R = 1) removed. Code 8888 (blank) dropped. Code **5555** is read here as "skinfold too large for the caliper": I did **not** confirm this in the CDC codebook (not opened); it is inferred from the data — the code's share rises steeply with BMI (0% at BMI < 25 to 47% for women's thigh at BMI ≥ 30), and one person with BMI 45.5 has 5555 at all four sites (seen in a raw listing). Those people are **excluded** from the mean, so means in the heaviest groups are biased low; the last column gives the share at the cap (an unweighted share of those examined; checker note: means and medians recomputed and matched, cap shares match unweighted to ±0.1 point).

| Site | Sex | Group | Weighted mean | P5 | Median | P95 | n measured | % beyond caliper (unweighted) |
|---|---|---|---|---|---|---|---|---|
| Triceps | Men | 20-39  | 12.5 | 5.3 | 11.2 | 25.3 | 2971 | 0.5% |
| Triceps | Men | 40-59  | 13.6 | 6.2 | 12.5 | 25.1 | 1969 | 0.8% |
| Triceps | Men | 60+  | 13.7 | 6.5 | 12.4 | 25.3 | 2437 | 0.1% |
| Triceps | Women | 20-39  | 22.3 | 10.2 | 20.8 | 38.1 | 3198 | 3.3% |
| Triceps | Women | 40-59  | 25.9 | 12.7 | 25.8 | 39.0 | 2144 | 4.2% |
| Triceps | Women | 60+  | 22.7 | 11.1 | 22.3 | 36.6 | 2476 | 1.8% |
| Triceps | Men | BMI<25  | 9.6 | 4.8 | 8.9 | 16.3 | 2941 | 0.0% |
| Triceps | Men | BMI 25-30  | 13.5 | 7.4 | 12.8 | 21.8 | 2990 | 0.0% |
| Triceps | Men | BMI>=30  | 20.1 | 9.9 | 18.6 | 35.6 | 1444 | 2.2% |
| Triceps | Women | BMI<25  | 17.8 | 9.3 | 17.4 | 27.4 | 3208 | 0.0% |
| Triceps | Women | BMI 25-30  | 26.3 | 17.1 | 26.0 | 35.5 | 2419 | 0.1% |
| Triceps | Women | BMI>=30  | 33.0 | 22.6 | 33.3 | 41.6 | 2179 | 10.1% |
| Subscapular | Men | 20-39  | 17.7 | 8.1 | 16.2 | 32.6 | 2916 | 0.9% |
| Subscapular | Men | 40-59  | 20.4 | 9.6 | 19.5 | 34.1 | 1872 | 1.2% |
| Subscapular | Men | 60+  | 20.0 | 9.4 | 19.2 | 33.1 | 2353 | 0.4% |
| Subscapular | Women | 20-39  | 20.0 | 8.2 | 17.8 | 38.2 | 3056 | 4.6% |
| Subscapular | Women | 40-59  | 23.7 | 9.7 | 23.1 | 39.6 | 2030 | 4.8% |
| Subscapular | Women | 60+  | 20.5 | 8.2 | 20.0 | 36.1 | 2424 | 1.6% |
| Subscapular | Men | BMI<25  | 13.6 | 7.5 | 12.9 | 22.0 | 2929 | 0.0% |
| Subscapular | Men | BMI 25-30  | 20.8 | 12.4 | 20.2 | 30.6 | 2951 | 0.0% |
| Subscapular | Men | BMI>=30  | 28.1 | 18.2 | 27.6 | 38.8 | 1260 | 4.5% |
| Subscapular | Women | BMI<25  | 15.1 | 7.5 | 14.0 | 26.1 | 3191 | 0.0% |
| Subscapular | Women | BMI 25-30  | 24.5 | 13.7 | 24.1 | 36.8 | 2362 | 0.4% |
| Subscapular | Women | BMI>=30  | 32.0 | 20.3 | 32.3 | 42.0 | 1948 | 12.6% |
| Suprailiac | Men | 20-39  | 22.0 | 6.3 | 21.6 | 38.7 | 2922 | 2.7% |
| Suprailiac | Men | 40-59  | 24.1 | 8.7 | 24.0 | 40.0 | 1936 | 2.5% |
| Suprailiac | Men | 60+  | 20.0 | 7.3 | 19.2 | 34.7 | 2393 | 1.0% |
| Suprailiac | Women | 20-39  | 18.9 | 5.8 | 16.5 | 38.3 | 3136 | 4.9% |
| Suprailiac | Women | 40-59  | 22.8 | 6.3 | 22.7 | 38.6 | 2084 | 6.1% |
| Suprailiac | Women | 60+  | 19.4 | 6.0 | 18.7 | 35.6 | 2372 | 3.7% |
| Suprailiac | Men | BMI<25  | 15.2 | 5.7 | 14.1 | 29.0 | 2923 | 0.0% |
| Suprailiac | Men | BMI 25-30  | 24.9 | 12.3 | 24.8 | 37.0 | 2980 | 0.2% |
| Suprailiac | Men | BMI>=30  | 32.6 | 19.6 | 33.6 | 42.5 | 1347 | 10.0% |
| Suprailiac | Women | BMI<25  | 13.1 | 5.1 | 11.7 | 25.6 | 3194 | 0.0% |
| Suprailiac | Women | BMI 25-30  | 24.1 | 12.3 | 23.9 | 35.9 | 2403 | 0.2% |
| Suprailiac | Women | BMI>=30  | 32.3 | 21.0 | 32.9 | 41.6 | 1988 | 16.0% |
| Thigh (front) | Men | 20-39  | 14.2 | 5.7 | 12.5 | 29.6 | 2867 | 1.0% |
| Thigh (front) | Men | 40-59  | 15.3 | 6.5 | 13.8 | 32.0 | 1923 | 1.1% |
| Thigh (front) | Men | 60+  | 14.8 | 6.2 | 12.7 | 30.8 | 2348 | 0.5% |
| Thigh (front) | Women | 20-39  | 27.9 | 14.1 | 27.7 | 41.6 | 2598 | 16.7% |
| Thigh (front) | Women | 40-59  | 31.4 | 15.4 | 32.8 | 42.7 | 1648 | 22.5% |
| Thigh (front) | Women | 60+  | 29.3 | 13.0 | 30.3 | 42.1 | 2156 | 11.4% |
| Thigh (front) | Men | BMI<25  | 11.1 | 5.2 | 9.9 | 20.7 | 2886 | 0.0% |
| Thigh (front) | Men | BMI 25-30  | 15.5 | 7.4 | 14.2 | 29.0 | 2917 | 0.0% |
| Thigh (front) | Men | BMI>=30  | 21.2 | 9.2 | 18.9 | 39.0 | 1334 | 4.4% |
| Thigh (front) | Women | BMI<25  | 26.1 | 12.8 | 26.0 | 39.7 | 3112 | 0.8% |
| Thigh (front) | Women | BMI 25-30  | 33.0 | 18.4 | 34.0 | 43.0 | 2094 | 8.1% |
| Thigh (front) | Women | BMI>=30  | 35.8 | 20.3 | 37.8 | 44.0 | 1187 | 47.3% |

## 2. NHANES 2005–06: triceps and subscapular

**Source.** CDC/NCHS NHANES 2005–2006 Body Measures (BMX_D.xpt, variables BMXTRI, BMXSUB, BMXBMI) and Demographics (DEMO_D.xpt: RIAGENDR, RIDAGEYR, RIDEXPRG, WTMEC2YR), https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2005/DataFiles/BMX_D.xpt and …/DEMO_D.xpt. **Who:** US population aged 20+, pregnant women removed. Missing values dropped (the subscapular site is missing more often in heavy people — see Sources disagree).

| Site | Sex | Group | Weighted mean | P5 | Median | P95 | n |
|---|---|---|---|---|---|---|---|
| Triceps | Men | 20-39 | 14.5 | 5.4 | 13.0 | 28.8 | 736 |
| Triceps | Men | 40-59 | 15.2 | 6.2 | 13.8 | 28.2 | 645 |
| Triceps | Men | 60+ | 15.3 | 7.2 | 14.0 | 28.0 | 685 |
| Triceps | Women | 20-39 | 22.6 | 11.0 | 22.2 | 36.2 | 593 |
| Triceps | Women | 40-59 | 24.8 | 13.1 | 25.0 | 36.8 | 571 |
| Triceps | Women | 60+ | 23.1 | 10.8 | 23.2 | 36.0 | 601 |
| Triceps | Men | BMI<25 | 9.8 | 5.0 | 9.2 | 16.6 | 591 |
| Triceps | Men | BMI 25-30 | 14.1 | 7.6 | 13.3 | 22.4 | 865 |
| Triceps | Men | BMI>=30 | 20.8 | 10.5 | 20.0 | 34.0 | 600 |
| Triceps | Women | BMI<25 | 18.2 | 9.4 | 17.4 | 28.8 | 698 |
| Triceps | Women | BMI 25-30 | 25.5 | 16.2 | 25.6 | 35.8 | 526 |
| Triceps | Women | BMI>=30 | 30.5 | 21.0 | 31.0 | 39.0 | 533 |
| Subscapular | Men | 20-39 | 17.7 | 8.2 | 16.4 | 30.8 | 635 |
| Subscapular | Men | 40-59 | 20.4 | 9.4 | 20.4 | 32.6 | 518 |
| Subscapular | Men | 60+ | 20.1 | 10.4 | 19.2 | 33.2 | 566 |
| Subscapular | Women | 20-39 | 19.7 | 8.0 | 18.4 | 35.0 | 514 |
| Subscapular | Women | 40-59 | 22.5 | 8.8 | 23.0 | 35.8 | 473 |
| Subscapular | Women | 60+ | 20.1 | 7.6 | 20.0 | 34.0 | 544 |
| Subscapular | Men | BMI<25 | 13.6 | 7.4 | 12.7 | 23.4 | 586 |
| Subscapular | Men | BMI 25-30 | 20.2 | 11.8 | 19.7 | 30.0 | 772 |
| Subscapular | Men | BMI>=30 | 25.5 | 16.0 | 25.0 | 35.8 | 355 |
| Subscapular | Women | BMI<25 | 15.5 | 7.3 | 14.2 | 27.3 | 681 |
| Subscapular | Women | BMI 25-30 | 23.3 | 12.2 | 23.0 | 35.0 | 479 |
| Subscapular | Women | BMI>=30 | 29.5 | 19.4 | 30.2 | 38.1 | 368 |

## 3. Reading the pattern (computed by me from the tables)

- Women carry about 1.6–1.9× the triceps fat of men at every age; on the trunk (subscapular, suprailiac) the sexes are close.
- The front-thigh skinfold is where the sexes differ most: women 20–39 median 27.7 mm vs men 12.5 mm, and for women with BMI ≥ 30 nearly half (47%) exceed the caliper.
- Subscapular rises most with build: men BMI < 25 → ≥ 30 roughly doubles (13.6 → 28.1 mm in NHANES III).

## Sources disagree

- NHANES III vs 2005–06 triceps, men 20–39: 12.5 vs 14.5 mm mean; women similar (22.3 vs 22.6). The later survey's men are heavier; also caliper ceiling handling differs (2005–06 has no 5555-type code in BMX_D; values above the limit appear as missing).
- Missing-not-at-random: in 2005–06, subscapular has about 30% fewer valid men in BMI ≥ 30 than triceps (355 vs 600), so heavy-group subscapular means are likely too low.

## Not found

- **Chest, abdominal, midaxillary, calf and biceps skinfolds in a US public file:** NHANES III and 2005–06 do not include them. Not found open with population weights.
- **Skinfolds on the hand and foot:** not found (not a standard site).
- **Ultrasound fat-layer thickness by site** (alternative to calipers): see A14 — not found as open tables.

## Sources

- NHANES III exam file and layout — https://wwwn.cdc.gov/nchs/data/nhanes3/1a/exam.sas
- NHANES 2005–06 — https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2005/DataFiles/BMX_D.xpt
