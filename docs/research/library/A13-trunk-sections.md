# A13 — Trunk cross-sections as numbers (ANSUR II), by sex and build

Accessed 2026-11-19. All arithmetic here is **computed by me** from the ANSUR II public data files unless a page is given.

**Source.** ANSUR II public data, ANSUR_II_FEMALE_Public.csv and ANSUR_II_MALE_Public.csv, https://www.openlab.psu.edu/ansur2/ (US Army, public). Definitions and summary statistics: Gordon C.C. et al. (2014), *2012 Anthropometric Survey of U.S. Army Personnel: Methods and Summary Statistics*, NATICK/TR-15/007, https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf.
**Who.** US Army soldiers measured 2010–12: **women n = 1,986, men n = 4,082**, standing, adults. Data in mm; shown in cm.
**Build.** BMI from the file (weightkg is stored in tenths of a kg; stature in mm): BMI < 25, 25–30, ≥ 30.

## 1. Shoulder level (no depth measure exists at this level)

| Sex | Biacromial breadth | Bideltoid breadth | Shoulder circumference (over deltoids) | Bideltoid/biacromial | Chest breadth/bideltoid | Waist breadth/chest breadth | Hip breadth/waist breadth |
|---|---|---|---|---|---|---|---|
| Women | 36.5 ± 1.8 | 45.0 ± 2.9 | 102.8 ± 5.3 | 1.233 | 0.598 | 1.113 | 1.186 |
| Men | 41.6 ± 1.9 | 51.0 ± 3.3 | 117.9 ± 6.4 | 1.228 | 0.568 | 1.127 | 1.064 |

## 2. Chest, waist, hip: breadth, depth, circumference and a shape number

**Shape number (computed by me).** For each person I fit a superellipse |x/a|^n + |y/b|^n = 1 with a = breadth/2 and b = depth/2, and solved for the exponent n whose perimeter equals the measured circumference (numerical perimeter, 2,000 steps per quadrant; n searched in 1.2–20). n = 2 is an ellipse; higher n is boxier. "Area ÷ bounding box" = Γ(1+1/n)² / Γ(1+2/n) at the median n (ellipse = 0.785, rectangle = 1). "Unsolved" = people whose circumference is too close to (or above) the bounding-rectangle perimeter 2(B + D) for any n ≤ 20.

| Level | Sex | Build (BMI) | n | Breadth cm | Depth cm | Circumference cm | Breadth/Depth | Superellipse n, median (P5–P95) | Area ÷ bounding box at median n | Unsolved |
|---|---|---|---|---|---|---|---|---|---|---|
| Chest (nipple level) | Women | all | 1986 | 26.9 ± 1.9 | 24.7 ± 2.7 | 94.7 ± 8.3 | 1.10 | 5.94 (4.09–10.50) | 0.963 | 61 |
| Chest (nipple level) | Women | BMI<25 | 949 | 25.9 ± 1.5 | 22.9 ± 1.9 | 88.9 ± 5.3 | 1.14 | 5.69 (4.03–9.75) | 0.960 | 10 |
| Chest (nipple level) | Women | BMI 25-30 | 847 | 27.6 ± 1.6 | 25.9 ± 1.9 | 98.2 ± 5.6 | 1.07 | 6.09 (4.10–10.58) | 0.965 | 28 |
| Chest (nipple level) | Women | BMI>=30 | 190 | 29.0 ± 1.7 | 29.0 ± 2.1 | 107.8 ± 6.5 | 1.00 | 6.80 (4.47–12.52) | 0.971 | 23 |
| Chest (nipple level) | Men | all | 4082 | 28.9 ± 1.8 | 25.4 ± 2.6 | 105.9 ± 8.7 | 1.15 | 10.39 (6.59–13.84) | 0.987 | 2886 |
| Chest (nipple level) | Men | BMI<25 | 1070 | 27.6 ± 1.4 | 22.6 ± 1.6 | 96.2 ± 4.7 | 1.23 | 9.44 (5.99–13.22) | 0.984 | 528 |
| Chest (nipple level) | Men | BMI 25-30 | 1904 | 28.9 ± 1.5 | 25.3 ± 1.5 | 105.6 ± 4.7 | 1.14 | 10.84 (7.61–14.11) | 0.988 | 1378 |
| Chest (nipple level) | Men | BMI>=30 | 1108 | 30.4 ± 1.6 | 28.2 ± 1.8 | 115.7 ± 5.9 | 1.08 | 11.61 (8.58–14.31) | 0.989 | 980 |
| Waist | Women | all | 1986 | 30.0 ± 3.3 | 21.3 ± 3.1 | 86.1 ± 10.0 | 1.42 | 2.83 (2.28–3.60) | 0.872 | 0 |
| Waist | Women | BMI<25 | 949 | 27.8 ± 2.2 | 19.1 ± 1.8 | 78.9 ± 6.1 | 1.46 | 2.87 (2.32–3.59) | 0.875 | 0 |
| Waist | Women | BMI 25-30 | 847 | 31.4 ± 2.3 | 22.6 ± 2.1 | 90.6 ± 6.5 | 1.40 | 2.82 (2.29–3.66) | 0.871 | 0 |
| Waist | Women | BMI>=30 | 190 | 34.8 ± 3.0 | 26.7 ± 2.8 | 102.0 ± 8.3 | 1.31 | 2.68 (2.12–3.38) | 0.861 | 0 |
| Waist | Men | all | 4082 | 32.6 ± 3.5 | 23.8 ± 3.5 | 94.1 ± 11.2 | 1.38 | 2.71 (2.30–3.25) | 0.863 | 0 |
| Waist | Men | BMI<25 | 1070 | 29.0 ± 2.0 | 20.2 ± 1.6 | 81.9 ± 5.6 | 1.44 | 2.65 (2.25–3.18) | 0.858 | 0 |
| Waist | Men | BMI 25-30 | 1904 | 32.5 ± 2.1 | 23.5 ± 2.1 | 93.5 ± 6.3 | 1.39 | 2.74 (2.33–3.28) | 0.865 | 0 |
| Waist | Men | BMI>=30 | 1108 | 36.4 ± 2.5 | 27.7 ± 2.7 | 106.8 ± 7.5 | 1.32 | 2.72 (2.34–3.26) | 0.864 | 0 |
| Hip / buttock | Women | all | 1986 | 35.4 ± 2.7 | 23.3 ± 2.4 | 102.1 ± 7.6 | 1.53 | 3.64 (2.98–4.49) | 0.915 | 0 |
| Hip / buttock | Women | BMI<25 | 949 | 33.7 ± 2.0 | 21.6 ± 1.5 | 96.7 ± 5.0 | 1.57 | 3.74 (3.10–4.61) | 0.918 | 0 |
| Hip / buttock | Women | BMI 25-30 | 847 | 36.4 ± 1.9 | 24.3 ± 1.5 | 105.5 ± 4.7 | 1.50 | 3.58 (2.95–4.35) | 0.912 | 0 |
| Hip / buttock | Women | BMI>=30 | 190 | 39.1 ± 2.4 | 27.5 ± 1.8 | 114.3 ± 5.6 | 1.42 | 3.34 (2.68–4.10) | 0.902 | 0 |
| Hip / buttock | Men | all | 4082 | 34.6 ± 2.4 | 24.6 ± 2.6 | 102.0 ± 7.7 | 1.41 | 3.40 (2.78–4.36) | 0.904 | 0 |
| Hip / buttock | Men | BMI<25 | 1070 | 32.4 ± 1.6 | 21.7 ± 1.5 | 93.7 ± 4.3 | 1.50 | 3.49 (2.81–4.48) | 0.909 | 0 |
| Hip / buttock | Men | BMI 25-30 | 1904 | 34.4 ± 1.7 | 24.5 ± 1.5 | 101.7 ± 4.3 | 1.41 | 3.43 (2.81–4.43) | 0.906 | 0 |
| Hip / buttock | Men | BMI>=30 | 1108 | 36.9 ± 2.0 | 27.5 ± 1.8 | 110.3 ± 5.5 | 1.34 | 3.26 (2.74–4.07) | 0.898 | 0 |

## 3. How to read the chest rows

The chest fit fails for most men (2,886 of 4,082 unsolved) and gives n ≈ 6–10 for women. The measured chest circumference is on average 0.974 (men) and 0.916 (women) of the bounding-rectangle perimeter, and for 14.8% of men it is **equal to or larger** than 2(B + D) (14.1% strictly larger) (computed by me). This means the tape path at chest level is not the convex outline spanned by chest breadth and chest depth: the tape bridges the shoulder blades and latissimus at the back and passes over the breast in women, while chest breadth is taken with calipers at a different set of points. **Do not use the chest superellipse numbers as a body outline;** use breadth/depth only, or a measured contour (Not found below). Waist and hip fits behave (n ≈ 2.7–3.8, all solved).

## 4. Levels not covered

ANSUR II has no breadth/depth pair at the shoulder, nipple-plus-arms, or navel level separate from the waist. Whether ANSUR II's "waist" is at the navel (omphalion) or the natural waist should be read from Gordon et al. 2014's measurement definitions before use; I did not confirm it this round.

## Sources disagree

- Chest superellipse vs physical plausibility: see section 3. The computed exponents are artefacts of mixing a caliper breadth with a tape circumference.

## Not found

- **Measured trunk cross-section contours (outline coordinates) at the shoulder, nipple, waist, navel and hip levels for men and women by build:** not found as open numbers. The Visible Human Project has full sections of one man and one woman, but NLM data come under Terms and Conditions (not public domain) — link only: https://www.nlm.nih.gov/research/visible/visible_human.html. Braune's 1872/1888 frozen sections (PD) exist but give one male body.
- **Superellipse exponents published by any study:** not found; all exponents here are computed by me.

## Sources

- ANSUR II public data — https://www.openlab.psu.edu/ansur2/
- Gordon et al. 2014 — https://tools.openlab.psu.edu/publicData/ANSURII-TR15-007.pdf
