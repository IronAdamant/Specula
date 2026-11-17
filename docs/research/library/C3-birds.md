# C3: Birds: wing bones, wing shape, wingbeat, stroke, folding, feathers, gait

Research for item C3 of `LIBRARY_RESEARCH_BRIEF.md`. Compiled 2026-11-16 (Australia/Sydney time). Every source was accessed 2026-11-16.

**Method.** I used WebSearch, WebFetch and curl only; the grok CLI was skipped because it is not signed in. Supplementary files were read in a scratch folder, `/workspace/c3work`, which is not a repo, and deleted afterwards. Nothing was committed, posted or sent.

**Status key.**
- **Verified**: I opened the source (full text, table or abstract) and the number is in it as written.
- **Verified (computed)**: I derived the value myself from numbers verified in the source. The arithmetic is stated.
- **Unverified**: the value comes from a secondary source, or from a snippet I could not open.

**Units.**
- m = body mass, b = wingspan, S = wing area.
- AR (aspect ratio) = b²/S.
- Q (wing loading) = m·g/S in N m⁻², with g = 9.81 m s⁻².
- f = wingbeat frequency in Hz; ρ = air density in kg m⁻³.

**Flight-style codes** (from Bruderer et al., as used in B4):
- CF = continuous flapping
- FS = flapping and soaring
- FG = flapping and gliding
- PT = "passerine-type" flight, i.e. flap-bounding

---

## 1. Wing bone proportions (humerus, ulna, carpometacarpus or manus)

### 1.1 By flight style: Wang, McGowan & Dyke 2011 [B4, B5]

- **Data:** corrected Table S1 (2013 correction [B5]), one row per species for 183 living species.
- **Measurements** (mm): hu = humerus, ul = ulna, mn = manus (hand), fprim = longest primary feather, ta = hu + ul + mn. I checked that ta equals hu + ul + mn in every row.
- **Specimens:** museum skeletons from the National Museum of Ireland, measured with digital calipers.
- **No n per species** is given.
- In the table, ratios are mean ± SD (range) across species, computed here from Table S1. The fprim/ta means I computed match the published back-transformed means in the paper's Fig. 1 legend exactly. Those published means and their 95% CIs are listed under the table.

| Flight style | n species | hu/ta | ul/ta | mn/ta | Brachial index hu/ul | fprim/ta | Status |
|---|---|---|---|---|---|---|---|
| CF | 90 | 0.358 ± 0.038 (0.270–0.433) | 0.359 ± 0.036 (0.297–0.428) | 0.282 ± 0.052 (0.180–0.388) | 1.008 ± 0.151 (0.759–1.382) | 0.913 ± 0.213 (0.506–1.484) | Verified (computed) |
| FS | 20 | 0.344 ± 0.026 (0.292–0.389) | 0.400 ± 0.021 (0.374–0.444) | 0.257 ± 0.044 (0.167–0.313) | 0.860 ± 0.045 (0.741–0.938) | 0.789 ± 0.149 (0.515–1.067) | Verified (computed) |
| FG | 8 | 0.280 ± 0.038 (0.204–0.312) | 0.369 ± 0.031 (0.313–0.411) | 0.351 ± 0.054 (0.320–0.483) | 0.762 ± 0.112 (0.640–0.884) | 1.547 ± 0.555 (0.985–2.523) | Verified (computed) |
| PT | 65 | 0.316 ± 0.022 (0.271–0.376) | 0.386 ± 0.022 (0.340–0.427) | 0.298 ± 0.038 (0.199–0.354) | 0.819 ± 0.055 (0.681–0.914) | 1.178 ± 0.187 (0.828–1.643) | Verified (computed) |

Published fprim/ta, back-transformed means with 95% CI [B4, Fig. 1a legend] (Verified):
- CF 0.9131 (0.8685–0.9557)
- FS 0.7889 (0.7190–0.8588)
- FG 1.5471 (1.0835–2.0107)
- PT 1.1782 (1.1319–1.2244)

The paper's ANOVA is F(3,182) = 37.789, P < 0.001. CF and FS do not differ after the multiple-comparison test (P = 0.062).

Things to know about this dataset:
- The flight-style labels follow Bruderer. Gulls, herons, owls, crows and pigeons are coded CF.
- At least one label looks wrong: *Neophron percnopterus* (Egyptian vulture) is coded PT.
- The mass column (M, g) is unreliable even in the corrected file. For example, *Alcedo atthis* is listed at 719 g. I did not use the mass column.

### 1.2 Example species: Wang et al. 2011 corrected Table S1 [B5]

All lengths in mm. No spread is given: one value per species, and n is not stated.

| Species | Style | Humerus | Ulna | Manus | Longest primary | Total arm (ta) | hu / ul / mn as fraction of ta | Brachial index | fprim/ta | Status |
|---|---|---|---|---|---|---|---|---|---|---|
| *Passer domesticus* | PT | 19.09 | 22.45 | 20.29 | 63 | 61.83 | 0.31 / 0.36 / 0.33 | 0.85 | 1.02 | Verified (ratios computed) |
| *Parus major* | PT | 16.42 | 20.00 | 14.86 | 49 | 51.28 | 0.32 / 0.39 / 0.29 | 0.82 | 0.96 | Verified (ratios computed) |
| *Turdus merula* | PT | 29.67 | 35.43 | 28.82 | 98 | 93.92 | 0.32 / 0.38 / 0.31 | 0.84 | 1.04 | Verified (ratios computed) |
| *Sturnus vulgaris* | PT | 27.77 | 33.73 | 30.97 | 107 | 92.47 | 0.30 / 0.36 / 0.33 | 0.82 | 1.16 | Verified (ratios computed) |
| *Hirundo rustica* | FG | 14.98 | 23.41 | 20.56 | 105 | 58.95 | 0.25 / 0.40 / 0.35 | 0.64 | 1.78 | Verified (ratios computed) |
| *Apus apus* | FG | 11.83 | 18.11 | 27.93 | 146 | 57.87 | 0.20 / 0.31 / 0.48 | 0.65 | 2.52 | Verified (ratios computed) |
| *Columba livia* | CF | 44.62 | 51.34 | 47.00 | 193 | 142.96 | 0.31 / 0.36 / 0.33 | 0.87 | 1.35 | Verified (ratios computed) |
| *Columba palumbus* | CF | 54.90 | 59.02 | 26.21 | 208 | 140.13 | 0.39 / 0.42 / 0.19 | 0.93 | 1.48 | Verified (ratios computed) |
| *Pica pica* | CF | 43.09 | 51.31 | 35.40 | 127 | 129.80 | 0.33 / 0.40 / 0.27 | 0.84 | 0.98 | Verified (ratios computed) |
| *Corvus corone* | CF | 66.26 | 79.50 | 69.63 | 228 | 215.39 | 0.31 / 0.37 / 0.32 | 0.83 | 1.06 | Verified (ratios computed) |
| *Falco tinnunculus* | FG | 53.85 | 63.41 | 55.37 | 170 | 172.63 | 0.31 / 0.37 / 0.32 | 0.85 | 0.98 | Verified (ratios computed) |
| *Falco peregrinus* | FG | 87.26 | 102.32 | 92.39 | 280 | 281.97 | 0.31 / 0.36 / 0.33 | 0.85 | 0.99 | Verified (ratios computed) |
| *Accipiter nisus* | FS | 51.77 | 62.29 | 49.74 | 146 | 163.80 | 0.32 / 0.38 / 0.30 | 0.83 | 0.89 | Verified (ratios computed) |
| *Buteo buteo* | FS | 106.88 | 127.15 | 94.35 | 253 | 328.38 | 0.33 / 0.39 / 0.29 | 0.84 | 0.77 | Verified (ratios computed) |
| *Milvus milvus* | FS | 123.58 | 141.42 | 112.82 | 268 | 377.82 | 0.33 / 0.37 / 0.30 | 0.87 | 0.71 | Verified (ratios computed) |
| *Aquila chrysaetos* | FS | 184.56 | 214.78 | 95.47 | 405 | 494.81 | 0.37 / 0.43 / 0.19 | 0.86 | 0.82 | Verified (ratios computed) |
| *Gyps fulvus* | FS | 237.85 | 292.02 | 161.55 | 576 | 691.42 | 0.34 / 0.42 / 0.23 | 0.81 | 0.83 | Verified (ratios computed) |
| *Ciconia ciconia* | FS | 199.33 | 229.92 | 169.04 | 350 | 598.29 | 0.33 / 0.38 / 0.28 | 0.87 | 0.59 | Verified (ratios computed) |
| *Ardea cinerea* | CF | 170.73 | 200.77 | 120.78 | 284 | 492.28 | 0.35 / 0.41 / 0.25 | 0.85 | 0.58 | Verified (ratios computed) |
| *Larus argentatus* | CF | 128.77 | 144.26 | 103.82 | 305 | 376.85 | 0.34 / 0.38 / 0.28 | 0.89 | 0.81 | Verified (ratios computed) |
| *Tyto alba* | CF | 82.52 | 91.41 | 51.75 | 230 | 225.68 | 0.37 / 0.41 / 0.23 | 0.90 | 1.02 | Verified (ratios computed) |
| *Strix aluco* | CF | 84.01 | 95.03 | 55.80 | 185 | 234.84 | 0.36 / 0.40 / 0.24 | 0.88 | 0.79 | Verified (ratios computed) |
| *Anas platyrhynchos* | CF | 89.75 | 74.25 | 63.84 | 204 | 227.84 | 0.39 / 0.33 / 0.28 | 1.21 | 0.90 | Verified (ratios computed) |
| *Anser anser* | CF | 179.74 | 171.11 | 181.19 | 410 | 532.04 | 0.34 / 0.32 / 0.34 | 1.05 | 0.77 | Verified (ratios computed) |
| *Cygnus olor* | CF | 279.68 | 254.68 | 231.93 | 423 | 766.29 | 0.36 / 0.33 / 0.30 | 1.10 | 0.55 | Verified (ratios computed) |

### 1.3 Scaling and group-level patterns

| Item | Value | Population | Spread | Source | Status |
|---|---|---|---|---|---|
| Total arm length (hu + ul + manus) vs body mass | ∝ M^0.37–0.39 (positive allometry) | Comparative species set (abstract; n not stated) | no spread given | Nudds 2007 [B9], abstract | Verified (abstract) |
| Wingspan vs body mass | ∝ M^0.35 by independent contrasts (CAIC); ∝ M^0.39 with species as data points | same | no spread given | [B9] abstract | Verified (abstract) |
| Individual wing bones vs mass | similar exponents for each bone; each bone is isometric with total arm length | same | — | [B9] abstract | Verified (abstract) |
| Brachial index (BI = humerus/ulna) | BI > 1 is the primitive state for modern birds; BI varies significantly between clades at every taxonomic level | "comprehensive dataset" of Neornithes | — | Nudds, Dyke & Rayner 2004 [B8], abstract | Verified (abstract) |
| Groups with BI < 1 | Piciformes and "higher" landbirds up to Passeriformes; humerus extremely short in swifts and hummingbirds; list includes Megapodidae, Piciformes, Galbuliformes, Bucerotiformes, Upupiformes, Trogoniformes, Coraciiformes, Psittaciformes | — | — | [B8] electronic appendix. Seen only as a search-engine snippet; the file is behind Cloudflare. | Unverified |
| Bird skeleton machine-measurement error | Root-mean-square error vs hand measurements: humerus 0.47 mm, ulna 0.96 mm, carpometacarpus 1.11 mm (UMMZ, 100 specimens) | Passerines | RMSE given | Weeks et al. 2025 [B23], Table 1 | Verified |

---

## 2. Wingspan, wing area, aspect ratio and wing loading

### 2.1 Example species: Alerstam et al. 2007, Protocol S1 [B6]

- **Data:** weighted mean biometry compiled from 33,610 measurements: Dunning, Magnan, Bruderer & Boldt, Pennycuick, Greenewalt and others.
- **Definitions:**
  - b = maximum tip-to-tip span on a bird in the hand.
  - S = projected area of both stretched wings, including the body strip between them.
- **Spread:** the file gives no spread for biometry.
- **Derived values:** I computed AR and Q from m, b and S.
- **Style column:** taken from Wang et al. 2011 [B4] where the species appears there.

| Species | Style [B4] | Mass (kg) | Span b (m) | Area S (m²) | AR | Q (N m⁻²) | Status |
|---|---|---|---|---|---|---|---|
| *Parus major* | PT | 0.019 | 0.23 | 0.0109 | 4.9 | 17 | Verified (AR, Q computed) |
| *Fringilla coelebs* | PT | 0.022 | 0.26 | 0.0132 | 5.1 | 16 | Verified (AR, Q computed) |
| *Alauda arvensis* | PT | 0.039 | 0.35 | 0.0207 | 5.9 | 18 | Verified (AR, Q computed) |
| *Turdus philomelos* | PT | 0.068 | 0.36 | 0.0218 | 5.9 | 31 | Verified (AR, Q computed) |
| *Sturnus vulgaris* | PT | 0.083 | 0.38 | 0.0244 | 5.9 | 33 | Verified (AR, Q computed) |
| *Hirundo rustica* | FG | 0.016 | 0.32 | 0.0136 | 7.5 | 12 | Verified (AR, Q computed) |
| *Apus apus* | FG | 0.038 | 0.40 | 0.0168 | 9.5 | 22 | Verified (AR, Q computed) |
| *Columba palumbus* | CF | 0.490 | 0.75 | 0.0842 | 6.7 | 57 | Verified (AR, Q computed) |
| *Corvus corone* | CF | 0.566 | 0.91 | 0.1378 | 6.0 | 40 | Verified (AR, Q computed) |
| *Corvus corax* | — | 1.149 | 1.21 | 0.2472 | 5.9 | 46 | Verified (AR, Q computed) |
| *Falco tinnunculus* | FG | 0.203 | 0.73 | 0.0708 | 7.5 | 28 | Verified (AR, Q computed) |
| *Falco peregrinus* | FG | 0.789 | 1.02 | 0.1257 | 8.3 | 62 | Verified (AR, Q computed) |
| *Accipiter nisus* | FS | 0.277 | 0.67 | 0.0768 | 5.8 | 35 | Verified (AR, Q computed) |
| *Buteo buteo* | FS | 0.885 | 1.24 | 0.2689 | 5.7 | 32 | Verified (AR, Q computed) |
| *Milvus milvus* | FS | 1.012 | 1.66 | 0.3248 | 8.5 | 31 | Verified (AR, Q computed) |
| *Aquila chrysaetos* | FS | 4.069 | 2.03 | 0.5970 | 6.9 | 67 | Verified (AR, Q computed) |
| *Haliaeetus albicilla* | FS | 4.967 | 2.18 | 0.8824 | 5.4 | 55 | Verified (AR, Q computed) |
| *Ciconia ciconia* | FS | 3.432 | 1.91 | 0.5326 | 6.8 | 63 | Verified (AR, Q computed) |
| *Ardea cinerea* | CF | 1.439 | 1.73 | 0.3716 | 8.1 | 38 | Verified (AR, Q computed) |
| *Grus grus* | CF | 5.614 | 2.22 | 0.5855 | 8.4 | 94 | Verified (AR, Q computed) |
| *Larus argentatus* | CF | 1.142 | 1.34 | 0.1968 | 9.1 | 57 | Verified (AR, Q computed) |
| *Sterna paradisaea* | — | 0.110 | 0.80 | 0.0571 | 11.2 | 19 | Verified (AR, Q computed) |
| *Gavia arctica* | — | 2.543 | 1.20 | 0.1196 | 12.0 | 209 | Verified (AR, Q computed) |
| *Phalacrocorax carbo* | CF | 2.227 | 1.40 | 0.2240 | 8.7 | 98 | Verified (AR, Q computed) |
| *Anas platyrhynchos* | CF | 1.082 | 0.88 | 0.1062 | 7.3 | 100 | Verified (AR, Q computed) |
| *Somateria mollissima* | CF | 2.015 | 0.98 | 0.1310 | 7.3 | 151 | Verified (AR, Q computed) |
| *Anser anser* | CF | 3.326 | 1.55 | 0.3079 | 7.8 | 106 | Verified (AR, Q computed) |
| *Cygnus olor* | CF | 10.597 | 2.30 | 0.6504 | 8.1 | 160 | Verified (AR, Q computed) |
| *Pelecanus onocrotalus* | FS | 8.504 | 2.91 | 0.9554 | 8.9 | 87 | Verified (AR, Q computed) |

### 2.2 By flight style (computed)

Species that appear in both Alerstam et al. Protocol S1 [B6] (biometry) and Wang et al. 2011 [B4] (style code). Values are mean ± SD (range) across species.

| Style | n species | AR | Q (N m⁻²) | Mass range (kg) | Status |
|---|---|---|---|---|---|
| CF (ducks, geese, swans, waders, gulls, herons, crane, cormorant, pigeons, crows) | 42 | 8.6 ± 1.4 (5.7–12.2) | 75 ± 44 (27–166) | 0.054–10.6 | Verified (computed) |
| FS (storks, pelican, kites, eagles, buzzards, harriers, osprey, sparrowhawk) | 16 | 7.2 ± 1.2 (5.4–8.9) | 47 ± 24 (21–104) | 0.277–8.50 | Verified (computed) |
| FG (swift, falcons, swallows and martins) | 7 | 8.1 ± 0.7 (7.5–9.5) | 27 ± 17 (12–62) | 0.015–0.789 | Verified (computed) |
| PT (larks, pipits, wagtail, thrushes, tits, jay, starling, finches), excluding the mislabelled Egyptian vulture | 19 | 5.5 ± 0.5 (4.4–6.2) | 21 ± 7 (12–34) | 0.009–0.162 | Verified (computed) |

### 2.3 Allometry of cruising speed: Alerstam et al. 2007, Table 1 [B6]

Reduced-major-axis regressions on log values. The 95% CIs were obtained by bootstrapping (100,000 replicates).

| Relationship | Sample | n | a (95% CI) | Exponent c (95% CI) | Status |
|---|---|---|---|---|---|
| Ue (m s⁻¹) = a × mass(kg)^c | all species | 138 | 15.9 (15.2–16.7) | 0.13 (0.11–0.15) | Verified |
| same | species with ≥ 10 tracks | 56 | 16.0 (15.0–17.0) | 0.13 (0.10–0.16) | Verified |
| same | phylogenetic contrasts | 17 | — | 0.12 (0.07–0.16) | Verified |
| Ue = a × Q(N m⁻²)^c | all species | 129 | 4.3 (4.0–4.6) | 0.31 (0.27–0.35) | Verified |
| same | phylogenetic contrasts | 17 | — | 0.32 (0.24–0.40) | Verified |
| Aspect ratio vs mass and vs Q | AR rises significantly with both (p < 0.01, n = 129; p < 0.05 by contrasts, n = 17) | 129 | — | slope not given in text | Verified (text) |

Theory predicts exponents of 1/6 for mass and 1/2 for wing loading; the observed values are lower [B6].

### 2.4 Wing shape vs flight style: Wang & Clarke 2015 [B7]

- **Method:** geometric morphometrics on 498 spread-wing specimens from 105 species (Burke Museum), using 90 landmarks: 60 on the outline and 30 on the coverts.
- **Main result:** wing shape is explained mostly by phylogeny and ecology. It shows "only a weak relationship" with flight-style categories, wing loading and aerodynamic variables.
- **Statistics:** r² for aspect ratio vs PC1 was 0.32 (dorsal) and 0.34 (ventral) on the Hackett et al. tree, rising to 0.50 and 0.53 on the Jarvis et al. tree.
- **Where the shape varies most:** greater primary covert extent (PC1 is 30.8% dorsal, 40.06% ventral). Waterbirds have longer primary coverts and shorter secondaries.
- **Status:** Verified (PMC full text). The species AR and Q values are in their Table S4, which I could not open.

---

## 3. Wingbeat frequency vs mass (Pennycuick equations) and Strouhal tuning

| Item | Equation or value | Population | Spread | Source | Status |
|---|---|---|---|---|---|
| Pennycuick 1990 | f = 1.08 · m^(1/3) · g^(1/2) · b^(−1) · S^(−1/4) · ρ^(−1/3) (SI units) | 32 species, 18 families, 20 g to nearly 5 kg; field observations | regression-based; no CI in abstract | [B1], abstract (two copies) | Verified (abstract) |
| Pennycuick 1996, full form | f ∝ (mg)^(1/2) · b^(−17/24) · S^(−1/3) · I^(−1/8) · ρ^(−3/8), where I = wing moment of inertia | 47 species (32 from 1990 + 15 new) | — | [B2], abstract | Verified |
| Pennycuick 1996, working equation | f = m^(3/8) · g^(1/2) · b^(−23/24) · S^(−1/3) · ρ^(−3/8). Assumes I ∝ m·b². No multiplying constant needed. | 47 species | — | [B2], abstract. Also reproduced exactly by me for all 16 species of [B3] Table 2, to 2 decimal places. | Verified |
| Scaling between geometrically similar species | f ∝ m^(−1/6) | theory | — | [B2] | Verified |
| Same bird, mass changes only (fuel, food) | f ∝ m^(1/2) | theory | — | [B2] | Verified |
| Constants used | g = 9.81 m s⁻²; sea-level ρ = 1.23 kg m⁻³ | — | uncertainty of g 0.01 (as a proportion) | [B3], Table 3 | Verified |
| Observed vs benchmark | Observed f ranged from 0.81 to 1.05 × f_ref in 14 of 16 species. The two bounding passerines (starling, chaffinch) were higher. | 16 migrating species, Falsterbo, Sweden | — | [B3], abstract and Results | Verified |
| Bounding correction | Gravity during flapping is effectively g′ = g/q, where q is the fraction of time flapping. Using g′ in the f_ref equation predicted the bounders' frequencies. | — | — | [B3], eq. 8 | Verified |
| Strouhal tuning | St = f·A/U. Cruising birds sit at the lower end of 0.2 < St < 0.4. | 60 species | — | Nudds, Taylor & Thomas 2004 [B10], abstract; Taylor, Nudds & Thomas 2003 [B11] | Verified (abstract) |
| Stroke-angle scaling | θ ≈ 67·b^(−0.24) | 60 species | — | [B10], abstract | Verified (abstract); units (θ in degrees, b in m) are inferred, not checked in the full text |
| Strouhal frequency predictor | f ≈ St·U / (b·sin(33.5·b^(−0.24))). St ≈ 0.21 for continuous flappers, 0.25 for intermittent fliers. Explains 90% of variance. | 60 species | — | [B10], abstract | Verified (abstract) |

### 3.1 Observed wingbeat frequency and speed: Pennycuick 2001, Tables 1–2 [B3]

- Mass, span and area are from Pennycuick's own database (Table 1). AR and Q are computed here.
- Observed values are mean ± SD.
- f_ref is the 1996 equation. Its "±" is a propagated uncertainty, not an SD.
- Air speeds are equivalent (sea-level) values.
- Columns: obs f = observed wingbeat frequency; Vmp = calculated minimum-power speed.

| Species | m (kg) | b (m) | S (m²) | AR | Q (N m⁻²) | obs f (Hz) | f_ref (Hz) | obs airspeed (m s⁻¹) | Vmp (m s⁻¹) | Status |
|---|---|---|---|---|---|---|---|---|---|---|
| *Corvus corone* (hooded crow) | 0.553 | 0.925 | 0.147 | 5.8 | 37 | 3.84 ± 0.0522 | 4.74 ± 0.181 | 10.5 ± 2.03 | 12.1 ± 0.976 | Verified |
| *Sturnus vulgaris* (starling, bounding) | 0.0884 | 0.384 | 0.0251 | 5.9 | 35 | 10.6 ± 1.25 | 9.97 ± 0.385 | 17.3 ± 1.75 | 10.2 ± 0.864 | Verified |
| *Fringilla coelebs* (chaffinch, bounding) | 0.0228 | 0.262 | 0.0130 | 5.3 | 17 | 18.2 ± 1.74 | 10.8 ± 0.691 | 15.3 ± 3.46 | 7.82 ± 0.715 | Verified |
| *Buteo buteo* (common buzzard) | 0.964 | 1.29 | 0.254 | 6.6 | 37 | 3.63 ± 0.168 | 3.54 ± 0.218 | 9.31 ± 1.92 | 12.3 ± 1.21 | Verified |
| *Accipiter nisus* (sparrowhawk) | 0.196 | 0.611 | 0.0642 | 5.8 | 30 | 5.10 ± 0.321 | 6.30 ± 0.388 | 8.72 ± 2.40 | 10.5 ± 1.03 | Verified |
| *Milvus milvus* (red kite) | 0.851 | 1.50 | 0.304 | 7.4 | 27 | 2.88 ± 0.0926 | 2.75 ± 0.170 | 7.90 ± 1.81 | 10.9 ± 1.08 | Verified |
| *Ardea cinerea* (grey heron) | 1.21 | 1.60 | 0.358 | 7.2 | 33 | 2.90 ± 0.0354 | 2.79 ± 0.172 | 11.0 ± 1.66 | 11.9 ± 1.17 | Verified |
| *Cygnus olor* (mute swan) | 9.01 | 2.31 | 0.682 | 7.8 | 130 | 3.38 ± 0.0863 | 3.37 ± 0.264 | 16.0 ± 0.700 | 19.4 ± 2.08 | Verified |
| *Anas penelope* (wigeon) | 0.770 | 0.822 | 0.0829 | 8.2 | 91 | 6.83 ± 0.330 | 7.27 ± 0.500 | 17.1 ± 1.99 | 14.3 ± 1.41 | Verified |
| *Somateria mollissima* (eider) | 1.39 | 0.978 | 0.131 | 7.3 | 104 | 6.47 ± 0.294 | 6.60 ± 0.586 | 20.2 ± 3.51 | 15.9 ± 2.19 | Verified |
| *Phalacrocorax carbo* (cormorant) | 2.56 | 1.35 | 0.224 | 8.1 | 112 | 4.83 ± 0.156 | 5.09 ± 0.265 | 15.0 ± 1.80 | 16.6 ± 1.39 | Verified |
| *Columba palumbus* (wood pigeon) | 0.495 | 0.751 | 0.0797 | 7.1 | 61 | 5.61 ± 0.256 | 6.81 ± 0.420 | 15.4 ± 2.06 | 12.9 ± 1.27 | Verified |
| *Larus ridibundus* (black-headed gull) | 0.280 | 0.963 | 0.0985 | 9.4 | 28 | 3.27 ± 0.0110 | 4.04 ± 0.274 | 10.1 ± 1.89 | 9.42 ± 0.964 | Verified |
| *Larus canus* (common gull) | 0.364 | 1.10 | 0.138 | 8.8 | 26 | 2.98 ± 0.151 | 3.50 ± 0.216 | 11.6 ± 1.74 | 9.62 ± 0.946 | Verified |
| *Larus argentatus* (herring gull) | 0.925 | 1.35 | 0.200 | 9.1 | 45 | 3.13 ± 0.0761 | 3.61 ± 0.218 | 11.8 ± 2.07 | 11.9 ± 1.06 | Verified |
| *Larus marinus* (great black-backed gull) | 1.51 | 1.66 | 0.290 | 9.5 | 51 | 2.91 ± 0.223 | 3.14 ± 0.215 | 12.8 ± 1.31 | 12.6 ± 1.35 | Verified |

For comparison, the 1990 equation [B1] applied to the same 16 birds gives values 2–10% below f_ref. Examples: swan 3.13 vs 3.37 Hz; eider 5.99 vs 6.60 Hz. These are computed values (Verified (computed)). See §8.

Other wingbeat facts:
- **Magpie and pigeon** (Tobalske & Dial 1996 [B13], wind tunnel): wingbeat frequency "did not change appreciably" with speed in steady flight, from 4–14 m s⁻¹ (magpie) and 6–20 m s⁻¹ (pigeon). No numbers in the abstract. Verified (abstract).
- **Zebra finch** (Tobalske, Peacock & Dial 1999 [B14]; 13.2 g, N = 4, 0–14 m s⁻¹): flap-bounds at every speed. Flight speed affected every measured wing variable except the percentage of the wingbeat spent in downstroke. Maximum body lift during bounds was 0.0206 N (15.9% of body weight) at 10 m s⁻¹. Verified (abstract).

---

## 4. Phases of the wingbeat (stroke description)

**How a wingbeat runs, in plain terms** (synthesis of [B12], [B13] and [B15]; Verified where a number is given):
1. **Downstroke.** The wing is extended and swept down and forward. This is the main lift and thrust phase. Downstroke kinematics vary little across speeds and species [B12].
2. **Turn at the bottom.** The wing supinates and begins to flex.
3. **Upstroke.** Its form depends on wing shape and speed [B12]:
   - Pointed, high-aspect-ratio wings (pigeon): *tip-reversal* upstroke at slow speed, where the hand-wing flips and sweeps back; *feathered* upstroke at medium speed; *swept-wing* upstroke at fast speed.
   - Rounded, low-aspect-ratio wings (magpie, finches): the wing is *flexed* close to the body on the upstroke at all speeds. Exceptions: galliforms use tip-reversal at take-off, and magpies change posture when accelerating.
   - Span ratio (mid-upstroke span ÷ mid-downstroke span) generally falls as speed rises. In some passerines it can rise.
4. **Turn at the top.** The wings re-extend for the next downstroke. In doves the wingtips can "clap" together here.

| Item | Value | Population | Spread | Source | Status |
|---|---|---|---|---|---|
| Phase timing at slow flight (cycle starts at upstroke onset = 0%) | Transition upstroke→downstroke ≈ 40–50% of cycle; first quarter of downstroke ≈ 50–63%; second quarter ≈ 63–75%; last half ≈ 75–100% | Diamond dove *Geopelia cuneata* and zebra finch *Taeniopygia guttata*, 4 birds each, 3rd–5th wingbeat after take-off | approximate ranges only | Crandell & Tobalske 2015 [B15], Methods | Verified |
| Hand-wing extension during upstroke | Dove keeps it at 80.2 ± 3.2% of wing length (tip-reversal); finch flexes to 64.8 ± 5.2% (flexed-wing upstroke) | as above | ± SD | [B15], Results | Verified |
| Wingtip speed peaks | Dove: 8.4 ± 0.6 m s⁻¹ (upstroke), 9.9 ± 1.3 m s⁻¹ (downstroke). Finch: 7.5 ± 3.3 m s⁻¹ (upstroke), 11.0 ± 1.9 m s⁻¹ (downstroke) | as above | ± SD | [B15] | Verified |
| Hand-wing angular velocity | Dove: 15.3 ± 0.8 deg ms⁻¹ (mid-upstroke), 19.4 ± 2.1 (mid-downstroke). Finch: 45.4 ± 13.8 deg ms⁻¹ at the upstroke→downstroke transition | as above | ± SD | [B15]. The abstract says 45.5; the Results say 45.4. | Verified |
| Angle of attack | Dove minimum −81.3 ± 4° in the first third of upstroke. Finch minimum −45.9 ± 7.3° in upstroke, peak 51.5 ± 7.4° in downstroke | as above | ± SD | [B15] | Verified |
| Clap and peel (dove) | Leading edges meet about 3 ms before trailing edges, at about 5% of the cycle; trailing edges separate 4–9 ms after leading edges | dove | ± SD | [B15] | Verified |
| Gaits | The old two-gait scheme ("vortex-ring" vs "continuous-vortex", Rayner 1988) has been replaced by continuous variation with speed | review | — | Tobalske 2007 [B12]; [B13] for pigeon gait change at about 10 m s⁻¹ | Verified |
| Intermittent flight | Species using both flap-gliding and flap-bounding weigh under about 300 g or have pointed high-AR wings. Birds over 300 g (e.g. pigeons) glide but do not bound. Birds under about 30 g with rounded wings almost only bound. | review | — | [B12] | Verified |

---

## 5. Wing folding

| Item | Value | Population | Spread | Source | Status |
|---|---|---|---|---|---|
| Mechanism | Elbow and wrist motion is coupled, but not by the planar "drawing parallels" four-bar linkage. Best fit is a six-bar mechanism: two spherical joints at the elbow, a pin joint at the radius–radiale, a slider joint (ulnare slides along the ulna), and two spherical joints to the carpometacarpus. 8 degrees of freedom. | Rock pigeon *Columba livia*, 3 birds, motion capture plus micro-CT in 7 poses | — | Stowers, Matloff & Lentink 2017 [B16] | Verified |
| Out-of-plane motion at full flex | At full flexion, the plane of radius/ulna + carpometacarpus deviates about 45° from the anatomical (humerus–forearm) plane | 3 pigeons | approximate | [B16], Results | Verified |
| Minimum planar error | Bones leave the best-fit motion plane by 5.0 ± 0.7 mm on average | 3 pigeons | ± (spread type as printed) | [B16] | Verified |
| Digit 2 | Moves independently by up to about 30° | pigeon | — | [B16], Fig. 2 caption | Verified |
| Gliding | Wing flex and extension during gliding follow a narrow path within the full wrist range | pigeon | — | [B16] | Verified |
| Pigeon gliding with speed | Pigeons flexed their wings progressively more as glide speed rose; magpie glide span did not change with speed | wind tunnel | — | [B13], abstract | Verified (abstract) |

---

## 6. Feather counts by group

| Feather row | Count | Group notes | Source | Status |
|---|---|---|---|---|
| Primaries | "Most bird species have 9–10"; "Most groups of birds have 10" | general | USFWS Feather Atlas glossary and position guide [B17] | Verified |
| Primaries | Typically 11 on the hand (6 on the metacarpus + 5 on the phalanges), but the outermost (the remicle) is often tiny or absent. Non-passerines 9–11. Grebes, storks and flamingos 12. Ostriches 16. Most passerines 10, some 9. | group list | Wikipedia "Flight feather" [B18] (secondary; primary references not opened) | Unverified |
| Secondaries | 9–25 depending on species | general | [B17] | Verified |
| Secondaries | 6 (hummingbirds) to about 40 (some albatrosses); more in larger, longer-winged birds. More than 40 non-passerine families lack the 5th secondary (diastataxy). | group list | [B18] | Unverified |
| Tertials | "Usually 3–4" | general | [B17] | Verified |
| Rectrices (tail) | "Most bird species have 10–12"; the most common number is 12 | general | [B17] | Verified |
| Rectrices | Always paired; most species have 6 pairs. Absent in grebes and some ratites; many grouse have more than 12; the count varies between individuals in ruffed grouse, hazel grouse and common snipe; domestic pigeons vary widely | group list | [B18] | Unverified |
| Ratites and penguins | Cassowary remiges reduced to 5–6 bare quills; most ratites have no rectrices (ostrich does); penguins have no differentiated flight feathers | — | [B18] | Unverified |
| Longest primary vs arm | See fprim/ta in §1.1. Primary length is about 0.79 × ta in soarers and 1.55 × ta in flap-gliders (swifts, falcons) | 183 species | 95% CI given | [B4] | Verified |

---

## 7. Walking vs hopping

| Item | Value | Population | Source | Status |
|---|---|---|---|---|
| Definitions | Hopping = legs move together (synchronous); walking and running = legs alternate (asynchronous) | passerines | Clark 1975 [B20] | Verified |
| Who hops | Arboreal species typically hop on the ground; more terrestrial species usually walk or run | 47 species observed (US, England, Kenya) plus a literature review | [B20] | Verified |
| Walking passerines | Adult pinyon jays *Gymnorhinus cyanocephalus* (Corvidae) walk. The Australian magpie *Gymnorhina tibicen* walks while the pied currawong *Strepera graculina* (same family, Cracticidae) hops. Furnariinae walk or run. Formicariidae hop, apart from the terrestrial forms. Common grackles usually walk but sometimes hop. | family notes | [B20], pp. 384–386 | Verified |
| Mixed families | At least 19 passerine higher taxa contain both mainly hopping and mainly walking species | — | [B20], Discussion and Summary. Adds 11 taxa to Kunkel's (1962) 8. | Verified |
| Same species, different gaits | Gait can change with age (e.g. *Mimus polyglottos* and *Gymnorhinus cyanocephalus* hop as juveniles and walk as adults), with substrate and speed, and with display vs foraging. House sparrows hop at all ages. | — | [B20], Table 1 (p. 385) and text | Verified |
| Evolution | Hopping and arboreality are derived traits and are significantly correlated (ancestral-state reconstruction on more than 1000 species). Hopping is linked to pelvis shape (hip and knee extensor origins). Habitat, more than gait, shapes the tarsometatarsus. | >1000 species; 47 species micro-CT | Provini & Höfling 2020 [B19], abstract | Verified (abstract) |
| Magpie gaits | Black-billed magpies walk, run and "out-of-phase" hop. Joint flexion patterns are similar across gaits. At speeds above walking, magpies prefer hopping to running. | *Pica pica*, high-speed video | Verstappen et al. 2000 [B21], abstract | Verified (abstract) |
| Ground birds | Quail, oystercatcher, lapwing, pigeon and avocet use mixed gaits (one walking step, one running step) more often than pure walking, and perhaps as often as grounded running. Walking is not strictly preferred at low speed. | >1 million frames | Andrada et al. 2015 [B22], abstract | Verified (abstract) |

---

## 8. Not found or not obtained

- **Rayner 1988**, "Form and function in avian flight" (*Current Ornithology* 5). This is a copyrighted book chapter that is not open. I did not use it; only its gait terms appear, as reported by Tobalske 2007.
- **Pennycuick 2008**, *Modelling the Flying Bird* (book). Not used, under the rule against copying copyrighted books. The 1990, 1996 and 2001 J Exp Biol papers cover the equations.
- **Nudds 2007** [B9] regression coefficients and CIs for predicting M, ta and b from single bones. The paper is paywalled and only the abstract exponents were seen.
- **Nudds, Dyke & Rayner 2004** [B8] mean BI per order. The electronic appendix (royalsocietypublishing.org) returned a Cloudflare challenge to curl and timed out in WebFetch.
- **Wang & Clarke 2015** [B7] Table S4 (AR and Q for 54 taxa). Same Cloudflare block.
- **Weeks et al. 2025 Skelevision dataset** (14,419 passerine skeletons, 2,057 species, CC0, Dryad doi:10.5061/dryad.v41ns1s4c). The Dryad API now needs a bearer token (HTTP 401). The GitHub repo holds code only and I found no mirror. Not obtained.
- **Downstroke fraction** (percentage of the cycle in downstroke) as a table with spread across many species. Only the two-species phase timing in [B15] was found. Tobalske et al. 1999 says the fraction did not change with speed in zebra finches but gives no number in the abstract.
- **Wing folding angles** (elbow and wrist range in degrees) for species other than the pigeon. Not found.
- **Group-by-group feather counts** from a primary or open ornithology source. Only USFWS (general ranges) and Wikipedia (secondary) were found, so the group rows are Unverified.
- **Per-species n and SD** for wing bones (Wang 2011 Table S1 gives one value per species) and for the biometry in Alerstam S1.
- **Wingbeat frequency of hummingbirds, hovering and swifts** with the Pennycuick equations. Not looked up beyond the 16-species table.

## 9. Sources disagree

1. **Pennycuick 1990 vs 1996 equations.**
   - Applied to the 16 birds in [B3], the 1990 formula gives 2–10% lower frequencies than the 1996 formula. Examples: eider 5.99 vs 6.60 Hz; wigeon 6.56 vs 7.27 Hz (computed).
   - Pennycuick 1996 states the 1990 formula underestimated high-frequency species.
   - Even the 1996 formula badly underestimates bounding passerines: chaffinch observed 18.2 Hz vs 10.8 Hz predicted. This is fixed only with the g/q bounding correction [B3].
2. **Biometry of the same species differs between compilations.**
   - Mute swan mass: 9.01 kg [B3] vs 10.597 kg [B6].
   - Common buzzard AR: 6.6 [B3] vs 5.7 [B6].
   - Grey heron AR: 7.2 [B3] vs 8.1 [B6].
   - The two papers give similar definitions of span and area, so the gaps probably come from different birds (wild migrants vs museum or hand measurements).
3. **Number of secondaries:** USFWS 9–25 [B17] vs Wikipedia 6–40 [B18].
4. **Number of primaries:**
   - USFWS says "most 9–10" and also "most groups 10" [B17].
   - Wikipedia says typically 11, including the often-vestigial remicle, and 12 in grebes, storks and flamingos [B18].
   - The gap is mostly about whether the remicle is counted.
5. **Does wing form track flight style?**
   - Wang et al. 2011 [B4] finds bone and feather proportions differ significantly between flight styles (F(3,182) = 37.8).
   - Wang & Clarke 2015 [B7] finds the wing *outline and covert* shape only weakly related to flight-style categories, and mostly explained by phylogeny.
   - These are different measurements, but they lead to different modelling advice.
6. **Wingspan exponent:** M^0.35 (phylogenetic contrasts) vs M^0.39 (species points) in the same study [B9].
7. **Wang et al. 2011 Table S1 data quality.**
   - The original 2011 file has a manus column that is sorted on its own, so its values do not match their rows.
   - The 2013 corrected file [B5] fixes the bone columns (ta = hu + ul + mn in all 183 rows).
   - The corrected file still has implausible masses (e.g. kingfisher 719 g) and labels the Egyptian vulture "PT". Use the bones, not the masses.
   - Some bone rows also look doubtful: wood pigeon manus 26.21 mm vs 47.00 mm in the smaller rock pigeon. Check any single species against a second source before relying on it.
   - The paper reports 184 species; the file has 183.
8. **Finch peak angular velocity:** 45.5 ± 13.8 deg ms⁻¹ (abstract) vs 45.4 ± 13.8 (Results) in [B15]. This is a small typo-level difference.

---

## 10. Sources (all accessed 2026-11-16)

**Link check, 2026-11-16, curl from the box:**
- HTTP 200: B3 PDF (Tethys), B2 Tethys page, B1 Bristol page, B4 DOI and PMC, B5 DOI, B6 DOI and PMC, B7 PMC, B9 Manchester page, B11 DOI, B16 author PDF, B18, B20, B22 DOI and PMC, B23 DOI and Dryad landing page.
- HTTP 203: the PubMed pages for B13, B14, B16, B19 and B21.
- HTTP 403 to curl (the publisher's Cloudflare check): the JEB and Royal Society DOIs, Wiley and OUP pages, and USFWS. These were read through WebFetch or Europe PMC instead:
  - B12 and B15 full text came through WebFetch.
  - B17 came through WebFetch.
  - The B8, B10 and B19 abstracts came through Europe PMC.
- B9's Wiley DOI returned HTTP 500; the Manchester repository page supports its numbers.

| ID | Full reference | URL(s) | Licence | Supports |
|---|---|---|---|---|
| B1 | Pennycuick CJ (1990) Predicting wingbeat frequency and wavelength of birds. *J. Exp. Biol.* 150: 171–185. | https://doi.org/10.1242/jeb.150.1.171 ; https://research-information.bris.ac.uk/en/publications/predicting-wingbeat-frequency-and-wavelength-of-birds/ | © The Company of Biologists 1990 (free to read on JEB; not open-licensed) | §3 1990 equation |
| B2 | Pennycuick CJ (1996) Wingbeat frequency of birds in steady cruising flight: new data and improved predictions. *J. Exp. Biol.* 199: 1613–1618. | https://doi.org/10.1242/jeb.199.7.1613 ; https://tethys.pnnl.gov/publications/wingbeat-frequency-birds-steady-cruising-flight-new-data-improved-predictions | © The Company of Biologists 1996 | §3 1996 equations and scaling |
| B3 | Pennycuick CJ (2001) Speeds and wingbeat frequencies of migrating birds compared with calculated benchmarks. *J. Exp. Biol.* 204: 3283–3294. Tables 1–3, eqs 2 and 8. | https://doi.org/10.1242/jeb.204.19.3283 ; PDF: https://tethys.pnnl.gov/sites/default/files/publications/Pennycuick_2001.pdf | © The Company of Biologists 2001 | §3.1 table, constants, bounding |
| B4 | Wang X, McGowan AJ, Dyke GJ (2011) Avian wing proportions and flight styles: first step towards predicting the flight modes of Mesozoic birds. *PLoS ONE* 6(12): e28672. Results, Fig. 1 legend, Table S1. | https://doi.org/10.1371/journal.pone.0028672 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC3233598/ | **CC BY** (Europe PMC licence field "cc by") | §1.1, §1.2, style codes |
| B5 | Wang X, McGowan AJ, Dyke GJ (2013) Correction: Avian wing proportions and flight styles…. *PLoS ONE* 8(10). Corrected Table S1. | https://doi.org/10.1371/annotation/6c8c755f-58fc-49e3-b499-8d082318eef6 | **CC BY** | §1.2 bone values |
| B6 | Alerstam T, Rosén M, Bäckman J, Ericson PGP, Hellgren O (2007) Flight speeds among bird species: allometric and phylogenetic effects. *PLoS Biol.* 5(8): e197. Table 1, Protocol S1. | https://doi.org/10.1371/journal.pbio.0050197 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC1914071/ | **CC BY** | §2.1–2.3 |
| B7 | Wang X, Clarke JA (2015) The evolution of avian wing shape and previously unrecognized trends in covert feathering. *Proc. R. Soc. B* 282: 20151935. | https://doi.org/10.1098/rspb.2015.1935 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC4614784/ | © 2015 The Author(s), published by the Royal Society, all rights reserved (free in PMC) | §2.4 |
| B8 | Nudds RL, Dyke GJ, Rayner JMV (2004) Forelimb proportions and the evolutionary radiation of Neornithes. *Proc. R. Soc. B* 271 (Suppl. 5): S324–S327. | https://doi.org/10.1098/rsbl.2004.0167 ; https://europepmc.org/articles/PMC1810057 | © Royal Society (free scan in PMC) | §1.3 BI |
| B9 | Nudds RL (2007) Wing-bone length allometry in birds. *J. Avian Biol.* 38(4): 515–519. | https://doi.org/10.1111/j.2007.0908-8857.03913.x ; https://research.manchester.ac.uk/en/publications/wing-bone-length-allometry-in-birds/ | © Journal of Avian Biology (paywalled) | §1.3 exponents |
| B10 | Nudds RL, Taylor GK, Thomas ALR (2004) Tuning of Strouhal number for high propulsive efficiency accurately predicts how wingbeat frequency and stroke amplitude relate and scale with size and flight speed in birds. *Proc. R. Soc. B* 271: 2071–2076. | https://doi.org/10.1098/rspb.2004.2838 ; https://pubmed.ncbi.nlm.nih.gov/15451698/ | © Royal Society | §3 Strouhal |
| B11 | Taylor GK, Nudds RL, Thomas ALR (2003) Flying and swimming animals cruise at a Strouhal number tuned for high power efficiency. *Nature* 425: 707–711. | https://doi.org/10.1038/nature02000 | © Nature Publishing Group (paywalled) | §3 St range |
| B12 | Tobalske BW (2007) Biomechanics of bird flight. *J. Exp. Biol.* 210: 3135–3146. | https://doi.org/10.1242/jeb.000273 ; https://journals.biologists.com/jeb/article/210/18/3135/17027/Biomechanics-of-bird-flight | © The Company of Biologists (free to read) | §4 stroke, intermittent flight |
| B13 | Tobalske B, Dial K (1996) Flight kinematics of black-billed magpies and pigeons over a wide range of speeds. *J. Exp. Biol.* 199: 263–280. | https://doi.org/10.1242/jeb.199.2.263 ; https://pubmed.ncbi.nlm.nih.gov/9317775/ | © The Company of Biologists | §3, §4, §5 |
| B14 | Tobalske BW, Peacock WL, Dial KP (1999) Kinematics of flap-bounding flight in the zebra finch over a wide range of speeds. *J. Exp. Biol.* 202: 1725–1739. | https://doi.org/10.1242/jeb.202.13.1725 ; https://pubmed.ncbi.nlm.nih.gov/10359676/ | © The Company of Biologists | §3 |
| B15 | Crandell KE, Tobalske BW (2015) Kinematics and aerodynamics of avian upstrokes during slow flight. *J. Exp. Biol.* 218(16): 2518 ff. | https://doi.org/10.1242/jeb.116228 ; https://journals.biologists.com/jeb/article/218/16/2518/14222/Kinematics-and-aerodynamics-of-avian-upstrokes | © The Company of Biologists (free to read) | §4 phase timings |
| B16 | Stowers AK, Matloff LY, Lentink D (2017) How pigeons couple three-dimensional elbow and wrist motion to morph their wings. *J. R. Soc. Interface* 14: 20170224. | https://doi.org/10.1098/rsif.2017.0224 ; https://pubmed.ncbi.nlm.nih.gov/28794161/ ; author PDF: https://lentinklab.com/wp-content/uploads/2022/06/1510366770_2017-avian-morphing-stowers.pdf | © 2017 The Author(s), published by the Royal Society, all rights reserved | §5 |
| B17 | U.S. Fish & Wildlife Service Forensics Laboratory. The Feather Atlas: Glossary; Identifying Feather Position. | https://www.fws.gov/lab/featheratlas/glossary.html ; https://www.fws.gov/lab/featheratlas/id-position.html | US federal government web page (US government works are normally public domain); no explicit licence seen on the page | §6 |
| B18 | Wikipedia contributors. "Flight feather". *Wikipedia* (read through the MediaWiki API, 2026-11-16). | https://en.wikipedia.org/wiki/Flight_feather | **CC BY-SA 4.0**; a secondary source | §6 (Unverified rows) |
| B19 | Provini P, Höfling E (2020) To hop or not to hop? The answer is in the bird trees. *Syst. Biol.* 69(5): 962–972. | https://doi.org/10.1093/sysbio/syaa015 ; https://pubmed.ncbi.nlm.nih.gov/32101317/ | © Oxford University Press | §7 |
| B20 | Clark GA Jr (1975) Additional records of passerine terrestrial gaits. *Wilson Bull.* 87(3): 384 ff. (Article 9). | https://digitalcommons.usf.edu/wilson_bulletin/vol87/iss3/9 | Free access (SORA archive); © Wilson Ornithological Society | §7 |
| B21 | Verstappen M, Aerts P, Van Damme R (2000) Terrestrial locomotion in the black-billed magpie: kinematic analysis of walking, running and out-of-phase hopping. *J. Exp. Biol.* 203: 2159–2170. | https://doi.org/10.1242/jeb.203.14.2159 ; https://pubmed.ncbi.nlm.nih.gov/10862728/ | © The Company of Biologists | §7 |
| B22 | Andrada E, Haase D, Sutedja Y, Nyakatura JA, Kilbourne BM, Denzler J, Fischer MS, Blickhan R (2015) Mixed gaits in small avian terrestrial locomotion. *Sci. Rep.* 5: 13636. | https://doi.org/10.1038/srep13636 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC4558583/ | **CC BY 4.0** | §7 |
| B23 | Weeks BC, Zhou Z, Probst C, Berv J, et al. (2025) Skeletal trait measurements for thousands of bird species. *Sci. Data*, article s41597-025-05234-y. Table 1. Dataset: doi:10.5061/dryad.v41ns1s4c. | https://doi.org/10.1038/s41597-025-05234-y ; https://doi.org/10.5061/dryad.v41ns1s4c | Paper **CC BY-NC-ND 4.0** (non-commercial, no derivatives; per the PDF, Crossref and Europe PMC; corrected 2026-11-16 per Library Verify); dataset CC0 (Dryad), **not obtained** | §1.3 error figures; §8 |
