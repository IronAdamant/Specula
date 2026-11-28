# R10-MATH Procedural generation: the maths for drawing bodies from published spreads

Round 10, maths note covering sections A to F. Access date 2026-11-28. Method and facts only. Formulas come from the cited open sources, with the page, equation or section named. "(mine)" marks my own derivations, worked numbers or advice. I computed the worked numbers with NumPy 2.5.5 and SciPy 1.18.1 using no outside data (the script is raw/round10/math/worked.py and its output is worked.txt). Nothing here uses SMPL, SCAPE, CAESAR, Basel Face Model, FLAME or AMASS.

## Sources

| # | Citation | Free legal copy | Licence | Read? |
|---|---|---|---|---|
| M-S1 | Rasmussen CE, Williams CKI (2006). Gaussian Processes for Machine Learning. MIT Press. Appendix A, Mathematical Background | Authors' free online edition: https://gaussianprocess.org/gpml/chapters/RWA.pdf | Copyright MIT Press; the free online edition is posted by the authors (no open licence) | Yes (A.2 eqs. A.5-A.6, sampling paragraph, A.4 eq. A.17) |
| M-S2 | Higham NJ (2002). Computing the nearest correlation matrix: a problem from finance. IMA Journal of Numerical Analysis 22(3):329-343. doi:10.1093/imanum/22.3.329 | MIMS EPrint 2006.70: https://eprints.maths.manchester.ac.uk/232/1/paper3.pdf | Publisher copyright; the author's eprint is free to read | Yes (abstract, eqs. 3.2-3.3, Algorithm 3.3) |
| M-S3 | Robert CP (1995). Simulation of truncated normal variables. Statistics and Computing 5:121-125. doi:10.1007/BF00143942 | arXiv:0907.4010: https://arxiv.org/abs/0907.4010 | arXiv non-exclusive distribution licence | Yes (Lemma 2.2, Proposition 2.3, section 2.2) |
| M-S4 | NIST/SEMATECH e-Handbook of Statistical Methods, section 1.3.6.6.9 Lognormal Distribution | https://www.itl.nist.gov/div898/handbook/eda/section3/eda3669.htm | US Government work | Yes (pdf, mean, standard deviation) |
| M-S5 | SciPy documentation, scipy.stats.vonmises | https://docs.scipy.org/doc/scipy/reference/generated/scipy.stats.vonmises.html | BSD-3-Clause (SciPy) | Yes (Notes: pdf) |
| M-S6 | Wikipedia, "Von Mises distribution" (Secondary; cites Mardia KV, Jupp PE (2000) Directional Statistics, Wiley, p.36) | https://en.wikipedia.org/wiki/Von_Mises_distribution | CC BY-SA 4.0 | Yes (pdf, circular variance, limiting behaviour). Mardia & Jupp not read (no free copy) |
| M-S7 | Wikipedia, "Mahalanobis distance" (Secondary) | https://en.wikipedia.org/wiki/Mahalanobis_distance | CC BY-SA 4.0 | Yes (chi-square statement, 2-D formula) |
| M-S8 | Shingleton A (2010). Allometry: the study of biological scaling. Nature Education Knowledge 3(10):2 | https://www.nature.com/scitable/knowledge/library/allometry-the-study-of-biological-scaling-13228439/ | Copyright Nature Education; free to read | Yes (log-log equation, ontogenetic, static and evolutionary allometry) |
| M-S9 | Round 9 note R9-D2 (Kilbourne & Hoffman 2013 PLoS ONE 8(11):e78392, CC BY; Scott 1985; Doube et al. 2009; Chase et al. 2002) | See R9-D2 | As listed there | Exponents taken from R9-D2, not re-read this round |
| M-S10 | Best DJ, Fisher NI (1979). Efficient simulation of the von Mises distribution. Applied Statistics 28(2):152-157 | None found (JSTOR; my fetch returned a non-PDF page, saved as .FAILED) | Publisher copyright | No. Cited only as the usual sampler; UNVERIFIED |

## 1. Correlated draws: multivariate normal, Cholesky and conditioning

**Drawing (M-S1, A.2, "generating multivariate Gaussian samples").** To draw x ~ N(m, K): compute the Cholesky factor L with K = L L^T (L lower triangular, eq. A.17). Draw u ~ N(0, I) from independent standard normals. Set x = m + L u. Then cov(x) = L E[u u^T] L^T = K. M-S1 adds that "in practice it may be necessary to add a small multiple of the identity matrix" (jitter) for numerical reasons.

**Building K from published tables (mine).** Sections C to F give SDs s_i and section D gives correlations r_ij. So K = S R S, where S = diag(s_i). Use SDs and correlations from the **same** sample where possible. Mixing sources is the main way to get a matrix that is not positive definite (section 5).

**Conditioning (M-S1, eq. A.6).** If (x, y) are jointly normal with means (mu_x, mu_y) and covariance blocks A = cov(x), B = cov(y), C = cov(x, y), then:

x | y ~ N( mu_x + C B^-1 (y - mu_y), A - C B^-1 C^T ).

- **Given stature (mine, the scalar case).** For one measure x and stature h: E[x | h] = mu_x + r (s_x / s_h)(h - mu_h), and SD[x | h] = s_x sqrt(1 - r^2). Worked number (mine): with r = 0.66 (the ear length-width correlation in R10-D, D2), the conditional SD is 0.751 of the marginal SD, so knowing one cuts the spread of the other by only a quarter.
- **Regression tables are already conditionals (mine).** The Hara hip-joint-centre equations in R10-C (for example HJC X = 11 - 0.063 LL) are E[x | leg length]. Their residual SD, if reported, is the conditional SD.
- **Given sex (mine).** Do not condition on sex inside one pooled normal. Keep a separate mean vector and covariance per sex (sections C, E and F report by sex) and pick the sex first. A pooled covariance from a mixed-sex sample inflates correlations that track size, because the sex difference in size adds a shared component.
- **Sequential draws (mine).** Drawing stature first and then the rest from the conditional gives the same joint distribution as one MVN draw. It is useful when stature is set by the user.

## 2. Angles: von Mises and wrapped normal

**Density (M-S5 Notes; M-S6).** f(x | mu, kappa) = exp(kappa cos(x - mu)) / (2 pi I0(kappa)), where I0 is the modified Bessel function of order 0. The circular variance is 1 - I1(kappa)/I0(kappa) (M-S6). As kappa grows it approaches a normal with sigma^2 = 1/kappa, and as kappa goes to 0 it becomes uniform (M-S6, citing Mardia & Jupp p.36).

**When it matters (mine).**

| kappa | Mean resultant length R = I1/I0 | Circular SD sqrt(-2 ln R), degrees | 1/sqrt(kappa), degrees |
|---|---|---|---|
| 1 | 0.4464 | 72.77 | 57.30 |
| 2 | 0.6978 | 48.61 | 40.51 |
| 4 | 0.8635 | 31.04 | 28.65 |
| 10 | 0.9486 | 18.61 | 18.12 |

The circular SD formula is the usual definition (Mardia & Jupp; not read here, UNVERIFIED as a citation); the values are my computation. Below about 20 degrees the normal and von Mises agree closely. Joint angles at rest in sections A and F have SDs of 2-6 degrees. Worked number (mine): the Morna elbow SD of 3.46 degrees (R10-F) gives kappa approximately 1/(0.0604 rad)^2 = 274. A plain normal, truncated to the joint's range of motion, is fine for these. Use von Mises for quantities that can wrap or have large spread: head yaw at rest, ear swivel (if ever measured), heading or pose orientation.

**Sampling.** SciPy's scipy.stats.vonmises (M-S5) and NumPy's Generator.vonmises sample it directly. The standard method is Best & Fisher 1979 (M-S10; UNVERIFIED, not read). A wrapped normal (draw a normal, take it modulo 360 degrees) is the simple alternative. It differs from von Mises only at large spread (mine).

## 3. Skewed and bounded measures: log-normal and truncated normal

### Log-normal

**Definition (M-S4).** X is log-normal if ln X is normal. The 2-parameter pdf has shape sigma (the SD of ln X) and scale m (the median). For the standard form, the mean is e^(0.5 sigma^2) and the SD is sqrt(e^(sigma^2)(e^(sigma^2) - 1)).

**From a published mean and SD (mine, derived from M-S4 by scaling by m).** CV = s/mean = sqrt(e^(sigma^2) - 1), so:

- sigma_log = sqrt(ln(1 + (s/mean)^2))
- mu_log = ln(mean) - sigma_log^2 / 2

**Worked example (mine).** Women 20-24 subscapular skinfold, 14.7 (7.6) mm (R10-C, Campa S2):

| Model | Median | P5 | P95 | Share below zero |
|---|---|---|---|---|
| Normal | 14.7 (the mean) | 2.2 | - | 2.65% (negative skinfolds) |
| Log-normal (sigma_log = 0.487, mu_log = 2.569) | 13.06 | 5.86 | 29.08 | 0 |

Use log-normal for skinfolds, body mass, fat-related girths and any measure with CV above about 20%. Cattle body weight in R10-F has a CV of about 26%. If a source gives percentiles, check P50 against the mean: a median below the mean is the sign of right skew (mine).

### Truncated normal

**Sampling (M-S3).** For a one-sided truncation N+(0, mu-, 1) with mu- > 0, Robert's optimal accept-reject uses an exponential proposal with rate alpha* = (mu- + sqrt(mu-^2 + 4)) / 2 (Proposition 2.3). M-S3 notes acceptance of 0.760 per pass at mu- = 0, against 0.5 for repeatedly drawing normals. Section 2.2 covers two-sided truncation. SciPy's scipy.stats.truncnorm does the same job.

**What truncation does to the spread (mine, computed).** For N(0, 1) truncated to plus or minus a, compared with clamping at plus or minus a:

| a | SD after truncation | Share of draws that clamping would pile onto the bounds |
|---|---|---|
| 1.5 | 0.743 | 13.4% |
| 2.0 | 0.880 | 4.55% |
| 2.5 | 0.955 | 1.24% |
| 3.0 | 0.987 | 0.27% |

- One-sided truncation at -2 SD (keep x > -2) shifts the mean up to +0.055 SD and the SD falls to 0.942 (mine).
- Clamping at plus or minus 2 SD gives an SD of 0.959 but puts 4.55% of bodies exactly on the bound, which shows up as a visible cluster of identical extreme bodies.
- **Rule (mine).** If the published SD should be kept after truncation, solve for the pre-truncation sigma, for example sigma = 1/0.880 = 1.137 times the published SD for a plus or minus 2 SD cut. Otherwise the generated population is narrower than the measured one.

## 4. Bounds: Mahalanobis and chi-square, clamps, rejection

**Population ellipsoid (M-S7).** For x ~ N(mu, Sigma) in d dimensions, d^2 = (x - mu)^T Sigma^-1 (x - mu) follows a chi-square distribution with d degrees of freedom. In 2-D, P(d < t) = 1 - e^(-t^2/2).

**Thresholds (mine, computed from the chi-square quantile; radius = sqrt).**

| d | 95% chi-square | radius | 99% chi-square | radius |
|---|---|---|---|---|
| 1 | 3.841 | 1.960 | 6.635 | 2.576 |
| 2 | 5.991 | 2.448 | 9.210 | 3.035 |
| 3 | 7.815 | 2.795 | 11.345 | 3.368 |
| 5 | 11.070 | 3.327 | 15.086 | 3.884 |
| 10 | 18.307 | 4.279 | 23.209 | 4.818 |
| 20 | 31.410 | 5.605 | 37.566 | 6.129 |

**A published example.** The NIOSH fit-panel ellipse (R10-D, D1, Zhuang 2007 Appendix B) uses radius 2.54 in the PC1-PC2 plane. 1 - e^(-2.54^2/2) = 0.960 (mine), which matches the paper's "more than 95%" coverage.

**Box against ellipsoid (mine, computed).** A per-dimension cut at plus or minus 2 SD keeps 95.45% in 1-D. For d independent dimensions it keeps 0.9545^d: 91.1% (2), 79.2% (5), 62.8% (10), 39.4% (20). So per-dimension 2 SD limits reject most bodies in high dimensions, and they are the wrong shape: they reject plausible bodies that are slightly large in many measures and keep implausible combinations at the corners. Use one Mahalanobis threshold for "inside the measured population", with a radius taken from the table above.

**Clamp against reject against truncate (mine).**

- **Clamping** each dimension to [lo, hi] breaks correlations at the edge and piles mass on the bounds (section 3). Use it only for tiny numerical overshoot.
- **Rejection** (redraw the whole vector until it passes every test) keeps the shape of the distribution inside the region exactly. The cost is the reject rate above. Correlations and SDs of the accepted set are slightly smaller than the targets.
- **Truncated-MVN samplers** (M-S3 builds the multivariate case from the one-dimensional sampler by Gibbs sampling) reach the same target as rejection when the acceptance region is small.

**Hard limits (mine).** Keep two layers:

1. The population layer is the Mahalanobis ellipsoid from the study's covariance, at a chosen coverage.
2. The physical layer is per-measure hard limits: survey min and max (R10-C, ANSUR II), Guinness-type extremes as absolute caps (R10-C), and range-of-motion limits for angles (R10-A and C). Apply these as rejection tests, not clamps.

Survey min and max are sample extremes. They grow with sample size, so they are not population limits.

**Exact constraints (mine).** Where a published identity holds (pelvic incidence = pelvic tilt + sacral slope, R10-D), draw two of the three and compute the third. Drawing all three from a covariance would break the identity.

## 5. Positive-definiteness: checking and repairing a correlation matrix

**Check (M-S1, A.4; mine).** A symmetric matrix is positive definite exactly when its Cholesky factorisation succeeds. That is the cheapest test, and it is the factor needed for drawing anyway. Equivalently, all eigenvalues must be above 0.

**Why it fails (mine).** Correlations taken from different papers, samples or subsets (R10-D: D4/D5 spine angles from several studies) need not be consistent with each other. Rounding to two decimals can also push a nearly singular matrix negative.

**Repair (M-S2).** Higham's nearest correlation matrix minimises the weighted Frobenius distance to the nearest symmetric positive semidefinite matrix with unit diagonal. With W = I:

- P_S(A) projects onto the PSD cone: set the negative eigenvalues to zero (eq. 3.3, general W form).
- P_U(A) projects onto unit-diagonal matrices: set the diagonal to 1 (eq. 3.2).
- Algorithm 3.3 alternates the two with Dykstra's correction: R_k = Y_(k-1) - dS_(k-1); X_k = P_S(R_k); dS_k = X_k - R_k; Y_k = P_U(X_k).
- Convergence is linear at best (M-S2, after Algorithm 3.3).

**Worked example (mine, an illustrative matrix, not from any paper).** Take A = [[1, 0.9, 0.7], [0.9, 1, 0.3], [0.7, 0.3, 1]]. Its eigenvalues are -0.0074, 0.711 and 2.297, and Cholesky fails. After 200 iterations of Algorithm 3.3 (my code) the off-diagonals become 0.8946, 0.6966 and 0.3025, at a Frobenius distance of 0.0097. The smallest eigenvalue is now 0 to rounding: the nearest correlation matrix is only semidefinite, so Cholesky can still fail. **Fix (mine):** after repair, either add jitter (M-S1) and rescale to a unit diagonal, or floor the eigenvalues at a small positive value (for example 1e-6) inside P_S.

**A real matrix that passes (mine).** Wakayama's Table 4 trio (R10-D: SVA-TK 0.12, SVA-PI-LL 0.54, TK-PI-LL -0.33) has determinant 0.542, so it is positive definite.

## 6. Allometric scaling for animals

**The law (M-S8).** log y = alpha log x + log b, that is, y = b x^alpha. alpha = 1 is isometry when x and y are the same kind of measure, with hyperallometry above 1 and hypoallometry below 1. For a length against body mass, the isometric value is 1/3 (R9-D2).

**Three kinds (M-S8, Figure 3).**

- **Ontogenetic allometry** is growth within an individual.
- **Static allometry** is among individuals of one species at the same stage.
- **Evolutionary allometry** is across species.

They have different slopes. For a generator (mine): use the evolutionary exponents (R9-D2: Kilbourne & Hoffman limb length against mass, all mammals hind 0.37, fore 0.40; Carnivora 0.42; Artiodactyla 0.30 hind and 0.34 fore) to build a species or breed template from body mass. Then use static spread (section F SDs and CVs) for individuals within it.

**Generator form (mine).** Work in log space:

log y_i = log b + alpha log M + e_i, with e ~ MVN(0, Sigma_log).

Sigma_log uses sigma_log from each CV as in section 3. Correlations come from a PCA-like structure: one shared size factor plus small shape noise, as F1 (dog PC1 = size) and F2 (cat indices far steadier than bone lengths) both show. Log space keeps lengths positive and makes the scaling linear.

**Breeds (mine).** In dogs, nearly all variation is between breeds (F1, withers SD 2-7% of the mean within a breed). Treat the breed as a template with its own mean vector, not as a draw from one species-wide normal. Dwarfism breeds sit off the main size axis (F1, PC2) and need their own limb-to-body ratios.

## 7. A recipe that ties it together (mine)

1. Choose the category: sex, age band, species or breed. Each has its own mean vector and covariance (sections C, E, F).
2. Optional: fix stature (or body mass for animals) and use the conditional in section 1.
3. Build K = S R S. Check it with Cholesky, and repair it with section 5 if needed.
4. Transform the skewed measures to log space (section 3).
5. Draw x = m + L u.
6. Reject if the Mahalanobis d^2 is above the chosen chi-square quantile (section 4).
7. Back-transform the logs. Compute the identities (PI = PT + SS). Reject on the hard physical limits and range of motion.
8. Draw rest angles as truncated normals, or von Mises for wide or wrapping quantities (section 2).
9. Log the acceptance rate. If it is low, the covariance and the limits disagree, which means a source problem, not a sampling problem.

## Gaps

- The Best & Fisher von Mises sampler and Mardia & Jupp were not read (no free copy). Their statements here rest on SciPy and Wikipedia (Secondary).
- No source here gives a joint distribution of human body measures and joint angles. Whether to correlate rest angles with body shape is my assumption to leave them independent.
- The static allometry exponents within each animal species are not tabulated in the section F sources. Only between-species exponents (R9-D2) are available.
- The truncation SD correction assumes normality before truncation. For log-normal measures, apply it in log space (mine).
