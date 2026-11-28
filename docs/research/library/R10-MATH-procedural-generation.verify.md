# R10-MATH verify

Re-check done 2026-11-28. Formula statements were checked against the saved source copies in raw/round10/math/ by regular expressions listed in MATH.chk. The worked numbers were checked against worked.txt, which is the output of worked.py (my code, NumPy 2.5.5 and SciPy 1.18.1, run on the box). Cross-references to rounds 9 and 10 notes were checked against those notes.

| Row | Claim | Source location | Result |
|---|---|---|---|
| 1 | GPML conditional eq A.6 | M-S1 A.2 | VERIFIED |
| 2 | GPML sampling x = m + Lu | M-S1 A.2 | VERIFIED |
| 3 | GPML jitter | M-S1 A.2 | VERIFIED |
| 4 | GPML Cholesky LL^T eq A.17 | M-S1 A.4 | VERIFIED |
| 5 | Higham problem statement | M-S2 Abstract | VERIFIED |
| 6 | Higham PU eq 3.2 | M-S2 eq. 3.2 | VERIFIED |
| 7 | Higham PS eq 3.3 | M-S2 eq. 3.3 | VERIFIED |
| 8 | Higham Algorithm 3.3 Dykstra | M-S2 Algorithm 3.3 | VERIFIED |
| 9 | Higham linear convergence | M-S2 after Alg. 3.3 | VERIFIED |
| 10 | Higham eprint id | M-S2 cover | VERIFIED |
| 11 | Robert Proposition 2.3 | M-S3 Prop. 2.3 | VERIFIED |
| 12 | Robert acceptance 0.760 vs 0.5 | M-S3 after Prop. 2.3 | VERIFIED |
| 13 | Robert two-sided section | M-S3 sec. 2.2 | VERIFIED |
| 14 | Robert Gibbs and multivariate | M-S3 Abstract, Keywords | VERIFIED |
| 15 | NIST lognormal definition | M-S4 pdf | VERIFIED |
| 16 | NIST sigma is SD of log, m median | M-S4 pdf | VERIFIED |
| 17 | NIST mean | M-S4 properties | VERIFIED |
| 18 | NIST SD | M-S4 properties | VERIFIED |
| 19 | SciPy von Mises pdf | M-S5 Notes | VERIFIED |
| 20 | Wiki von Mises circular variance | M-S6 | VERIFIED |
| 21 | Wiki von Mises large kappa sigma^2 = 1/kappa | M-S6 Limiting behaviour | VERIFIED |
| 22 | Wiki von Mises Mardia & Jupp p.36 | M-S6 ref | VERIFIED |
| 23 | Wiki von Mises small kappa uniform | M-S6 | VERIFIED |
| 24 | Mahalanobis chi-square | M-S7 | VERIFIED |
| 25 | Mahalanobis 2-D formula | M-S7 | VERIFIED |
| 26 | Shingleton citation | M-S8 | VERIFIED |
| 27 | Shingleton log equation | M-S8 | VERIFIED |
| 28 | Shingleton three kinds | M-S8 Figure 3 | VERIFIED |
| 29 | Worked chi2 d=2 95 | worked.txt | VERIFIED |
| 30 | Worked chi2 d=10 99 | worked.txt | VERIFIED |
| 31 | Worked chi2 d=20 95 | worked.txt | VERIFIED |
| 32 | Worked 2D k=2.54 | worked.txt | VERIFIED |
| 33 | Worked box acceptance d=20 | worked.txt | VERIFIED |
| 34 | Worked truncation a=2 | worked.txt | VERIFIED |
| 35 | Worked one-sided -2 | worked.txt | VERIFIED |
| 36 | Worked clamp SD | worked.txt | VERIFIED |
| 37 | Worked log-normal skinfold | worked.txt | VERIFIED |
| 38 | Worked cond SD r=0.66 | worked.txt | VERIFIED |
| 39 | Worked Morna kappa | worked.txt | VERIFIED |
| 40 | Worked kappa 10 row | worked.txt | VERIFIED |
| 41 | Worked bad matrix eig | worked.txt | VERIFIED |
| 42 | Worked nearest corr | worked.txt | VERIFIED |
| 43 | Skinfold input 14.7 (7.6) | R10-C (Campa S2) | VERIFIED |
| 44 | NIOSH 2.54 ellipse | R10-D D1 | VERIFIED |
| 45 | Ear r 0.660 | R10-D D2 | VERIFIED |
| 46 | Wakayama trio | R10-D D4/D5 | VERIFIED |
| 47 | Hara HJC X equation | R10-C | VERIFIED |
| 48 | R9 exponents | R9-D2 (Kilbourne & Hoffman Table 3) | VERIFIED |
| 49 | Morna elbow 3.46 | R10-F | VERIFIED |
| 50 | Log-normal sigma_log and mu_log from mean and SD | Section 3 | Labelled (mine): derived from M-S4 by scaling the standard form by the median m; reproduced numerically in worked.py |
| 51 | Circular SD = sqrt(-2 ln R) | Section 2 | Usual definition (Mardia & Jupp); source not read, so the citation is UNVERIFIED; the values are computed (worked.txt) |
| 52 | Best & Fisher 1979 sampler | M-S10 | UNVERIFIABLE (no free copy; JSTOR fetch FAILED) |
| 53 | Wakayama trio determinant 0.542 | Section 5 | Recomputed (mine): det = 0.5423 |
| 54 | Truncation SD correction 1/0.880 = 1.137 | Section 3 | Arithmetic (mine) from row 34 |
| 55 | Box vs ellipsoid, clamp vs reject advice; recipe in section 7 | Sections 4, 7 | Labelled (mine), advice not a sourced fact |
| 56 | Higham algorithm implementation | worked.py | My code follows Algorithm 3.3 with W = I (rows 7-8); the result has unit diagonal and smallest eigenvalue 0 to rounding |

Fixes: the Robert regex was reordered to match the order of the abstract and keywords. A JSTOR fetch for Best & Fisher returned a non-PDF page and was renamed .FAILED.

WEAK: the von Mises and Mahalanobis statements rest on Wikipedia (Secondary), backed by SciPy for the von Mises pdf. The allometry exponents are carried over from round 9 and were not re-read. The bad-matrix example is illustrative, not from a paper.
