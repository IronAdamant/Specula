# C5: Fish and snakes (body waves, fin placement, the four snake gaits)

Research for the Specula game-engine library. Written 2026-11-17 (AEDT). Every source was opened, or its link checked, on **2026-11-17**. Every table row gives its URL in the last column.

**Method.** I read the primary papers directly: PMC HTML, author-hosted PDFs (Lauder lab, Hu lab), the arXiv preprint, the Archimer manuscript, Europe PMC abstracts and FishBase pages. On top of that I ran one Grok live-search pass (grok-4.6, reasoning effort xhigh, live search), **2026-11-17 08:59–09:04 AEDT**. Everything Grok returned was treated as an unverified lead. A Grok number is marked **Verified** here only if I found the same number in the primary source myself. §6 lists what the Grok pass added or contradicted. Scratch files are in /workspace/c5work. No dataset was downloaded.

**Status key.**
- **Verified**: I opened the cited source on 2026-11-17 and the number is there, at the stated table, figure or page.
- **Verified (abstract)**: the number is in the authors' own abstract, which I read. The full text is behind Cloudflare or a paywall.
- **Verified (computed)**: arithmetic on Verified numbers, shown.
- **Verified (as quoted)**: a number one primary paper quotes from another; the original was not opened.
- **Unverified**: the source was not opened (paywall or bot block), or the number came only from Grok.

**Units.** L = body length (total length unless the row says SL, standard length, or FL, fork length). BL s⁻¹ = body lengths per second. Amplitude conventions differ between papers; see Sources disagree 1. St = Strouhal number. λ = body wavelength. Slip = swimming speed ÷ body-wave speed (U/V). f = tail-beat or wave frequency.

**Licence summary.** The numbers are facts taken from papers. Nothing here copies a figure, a table layout or prose. Two sources carry **non-commercial** terms and are flagged where they are used: FishBase (CC BY-NC 4.0) and the Newman thesis, which is not used. FISHMORPH is CC BY 4.0, and Di Santo's Dryad data are CC0, but neither was downloaded.

---

## 1. Fish: how the body wave travels

### 1.1 Comparative: 44 species across the four classic modes (Di Santo et al. 2021, F1)

Adults, n = 1–6 per species, 43 fish species plus amphioxus, filmed in steady swimming at four facilities. Speeds 0.2–25 BL s⁻¹. Midlines digitized at 200 points, nose to caudal fin tip.

| Quantity | Value | Spread | Population | Source (location) | Status | URL (accessed 2026-11-17) |
|---|---|---|---|---|---|---|
| Wavelength by mode, all speeds | anguilliform median **0.75 L**; subcarangiform **0.93 L**; thunniform **1.14 L** | ANOVA F = 8.26, P < 0.001; individual SD up to ~20% of L in trout, brook trout, yellowfin tuna, crevalle jack and short-nose sturgeon | 44 species; modes n = 12 / 27 / 4 / 1 species (angu / subcar / car / thunn) | F1 Results, Fig. 2A | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Wavelength, four canonical species at ~1 BL s⁻¹ | eel **0.58 L**, brook trout **1.00 L**, mackerel **0.96 L**, yellowfin tuna **1.17 L** | n = 5 sequences each; eel significantly shorter (F = 7.70, P = 0.0024) | *Anguilla rostrata*, *Salvelinus fontinalis*, *Scomber scombrus*, *Thunnus albacares* | F1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Outliers against the mode labels | Atlantic salmon (subcarangiform) **0.74 L**; amphioxus (anguilliform) **1.04–1.49 L**, median 1.14 | — | *Salmo salar*, *Branchiostoma lanceolatum* | F1 Results and Discussion | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Head amplitude | median **0.03 L** | IQR 0.02–0.05 | all individuals | F1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Tail amplitude | median **0.18 L** | IQR 0.15–0.21; maximum always at ≥ 90% of L | all individuals | F1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Head:tail amplitude, high cases | amphioxus **0.74**; hagfishes **0.38** and **0.47** | — | single species values | F1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Amplitude envelope, global fit | **y = 0.05 − 0.13x + 0.28x²** (x = position from nose, fraction of L; y in L) | r² = 0.59, P < 0.001; explains ~60% of variance; 90–92% of individuals fit a 2nd-order polynomial with R² > 0.9 | all species | F1 Results, Fig. 3B | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Envelope evaluated at x = 0 / 0.2 / 0.4 / 0.6 / 0.8 / 1.0 | **0.050 / 0.035 / 0.043 / 0.073 / 0.125 / 0.200 L** | inherits the fit's r² 0.59 | — | computed from the row above | Verified (computed) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Maximum curvature | median **3.53 L⁻¹**, always posterior to the caudal peduncle | IQR 2.22–5.12 L⁻¹ | all species | F1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Strouhal number | median **0.29** | range 0.15–0.82; IQR 0.24–0.35 | all sequences | F1 Results (SI Fig.) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| What changes with speed | Only tail-beat frequency: r = **0.95** with speed. Head amplitude r = 0.08, maximum amplitude 0.18, wavelength −0.03, curvature 0.02 | — | all sequences | F1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |
| Reynolds number | median **110,288** | IQR 60,370–753,319; amphioxus mean 568 | all sequences | F1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/ |

**For the engine.** One quadratic envelope plus a wavelength of 0.6–1.2 L covers eel to tuna. The authors found no steady head-to-tail amplitude trend from anguilliform to thunniform. In their data, the mode labels track body shape (fineness ratio, peduncle) more than the 2-D midline motion. To animate a faster swimmer, raise the frequency and keep the shape constant.

### 1.2 Anguilliform: American eel, the fullest single dataset (Tytell & Lauder 2004; Tytell 2004)

| Quantity | Value | Spread | Population | Source (location) | Status | URL (accessed 2026-11-17) |
|---|---|---|---|---|---|---|
| Body length | 20.8 cm | ± 0.7 (s.e.m.), N = 3 fish | *Anguilla rostrata*; 3 of 11 eels (8–23 cm) | F2 Table 1 (p. 1832) | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Swimming speed U | 1.374 L s⁻¹ | ± 0.002 s.e.m., N = 118 beats | same | F2 Table 1 | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Tail-beat amplitude A (half the excursion) | **0.0693 L** | ± 0.0005 | same | F2 Table 1 | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Tail-beat frequency | **3.11 Hz** | ± 0.03 | same | F2 Table 1 | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Body-wave speed V | 1.878 L s⁻¹ | ± 0.006 | same | F2 Table 1 | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Wavelength | **0.604 L** (about 1.65 waves on the body) | ± 0.006 | same | F2 Table 1 and text | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Strouhal number (2fA/U) | **0.314** | ± 0.003 | same | F2 Table 1 | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Slip U/V | **0.731** | ± 0.002 | same | F2 Table 1 | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Stride length U/f | 0.448 L | ± 0.005 | same | F2 Table 1 | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Amplitude envelope | y(s) = A·e^{α(s−1)}, s from 0 (head) to 1 (tail); **α = 2.759** | ± 0.009 (N = 2180); r² = 0.978 against 0.890 for a linear fit | same | F2 Table 1 and text | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Envelope evaluated at s = 0 / 0.2 / 0.4 / 0.6 / 0.8 / 1.0 | **0.0044 / 0.0076 / 0.0132 / 0.0230 / 0.0399 / 0.0693 L** | — | — | computed from A and α above | Verified (computed) | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Individual variation | Most variables within 10% between eels. One eel had ~13% higher amplitude and ~25% lower frequency; another had a ~20% longer wavelength. Wave speed within 5% for all | — | 3 eels | F2 Results | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Speed sweep: speed classes | 0.549 (N = 17), 0.906 (N = 56), 1.374 (N = 118), 1.88 (N = 83) L s⁻¹ | ± 0.007 / 0.005 / 0.003 / 0.01 | 3 eels (20, 20, 23 cm; 14–16 g), 274 beats | F3 Results (p. 3268) | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.2004.pdf |
| Speed sweep: regressions | amplitude 0.0699 ± 0.0006 L (no significant speed effect, P = 0.096); f = 1.3 + 1.30·U Hz; wavelength 0.597 ± 0.005 L (n.s., P = 0.067); wave speed V = 0.39 + 1.07·U; St 0.324 ± 0.003 (n.s.) | ± values as printed | same | F3 Table 1 | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.2004.pdf |
| Slip vs speed | 0.57 at the slowest speed, rising to 0.784 at the fastest | ± 0.01 / ± 0.002 | same | F3 Results | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.2004.pdf |
| Envelope vs speed | α = **3.90** at the slowest speed, **2.25** at the fastest. Head amplitude is 420% larger at the highest speed | ± 0.04 / ± 0.01 | same | F3 Results | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.2004.pdf |
| Envelope at slow (α 3.90) vs fast (α 2.25), s = 0 / 0.2 / 0.4 / 0.6 / 0.8 / 1.0 | slow 0.0014 / 0.0031 / 0.0067 / 0.0147 / 0.0320 / 0.0699; fast 0.0074 / 0.0116 / 0.0181 / 0.0284 / 0.0446 / 0.0699 L | — | — | computed (A = 0.0699) | Verified (computed) | https://sites.harvard.edu/glauder/files/2022/03/Tytell.2004.pdf |
| Wavelength individuality | individuals differ by up to 30%; within-set amplitude and frequency vary by up to 20% (SD 8%) | — | same | F3 Results | Verified | https://sites.harvard.edu/glauder/files/2022/03/Tytell.2004.pdf |
| St during steady swimming vs acceleration | steady St stays close to **0.32**; during acceleration it departs from that value | — | *A. rostrata* | F4 abstract | Verified (abstract) | https://pmc.ncbi.nlm.nih.gov/articles/PMC1691903/ |
| Gillis 1998 eel values (f 2.484 Hz, V 1.27 L s⁻¹, tail amplitude ~0.08 L at 1.0 L s⁻¹) | — | — | *A. rostrata* | via Grok (quoting Tytell 2004 II and the Gillis abstract) | **Unverified**: Gillis 1998 not opened (JEB behind Cloudflare) | https://doi.org/10.1242/jeb.201.7.949 (not opened) |

### 1.3 Carangiform and thunniform

| Quantity | Value | Spread | Population | Source (location) | Status | URL (accessed 2026-11-17) |
|---|---|---|---|---|---|---|
| Yellowfin tuna propulsive wavelength | **1.23–1.29 L** (L = fork length), 30–60% longer than in salmonids | range as printed | *Thunnus albacares*, water tunnel | F5 abstract | Verified (abstract) | https://pubmed.ncbi.nlm.nih.gov/9317308/ |
| Kawakawa (thunniform) vs chub mackerel (carangiform) at equal speed | Kawakawa: higher tail-beat frequency; lower stride length, tail-beat amplitude and wavelength; lower intervertebral flexion along most of the body. Snout yaw not significantly different | — | juveniles: 10 *Euthynnus affinis* (15.1–25.5 cm FL), 8 *Scomber japonicus* (14.0–23.4 cm FL), 24 °C, 120 Hz video | F6 abstract | Verified (abstract); no numbers in the abstract | https://pubmed.ncbi.nlm.nih.gov/11003821/ |
| Amplitude at 0.85 L | chub mackerel and kawakawa **~4%** of L (Donley & Dickson 2000); largemouth bass **~4.5%** (Jayne & Lauder 1995); eel **4.4%** | — | as named | quoted in F2 Discussion (p. 1838) | Verified (as quoted) | https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf |
| Strouhal window | dolphins, sharks and bony fish swim at **0.2 < St < 0.4**, where propulsive efficiency usually peaks. St = fA/U | — | multi-taxon compilation | F7 abstract | Verified (abstract); per-species tables not read | https://pubmed.ncbi.nlm.nih.gov/14562101/ |
| Bainbridge's speed rule: V = ¼·L·(3f − 4) for f > 5 Hz; maximum tail amplitude ≈ 0.2 L | — | — | dace, trout, goldfish, L up to ~30 cm | Bainbridge 1958, via Grok | **Unverified**: JEB behind Cloudflare; no abstract in Europe PMC | https://doi.org/10.1242/jeb.35.1.109 (not opened) |
| Saithe vs mackerel: dimensionless kinematics nearly constant with speed; tail amplitude 0.1 L and stride 0.86 L for a 0.35 m saithe at 3 L s⁻¹ | — | — | *Pollachius virens*, *Scomber scombrus* | Videler & Hess 1984 and a secondary quote, via Grok | **Unverified** | https://doi.org/10.1242/jeb.109.1.209 (not opened) |

---

## 2. Fin placement

No compiled, peer-reviewed table gives the longitudinal positions of the pectoral, pelvic, dorsal and anal fins as a fraction of length with spread across many species. Grok came to the same conclusion. Two usable routes:

| Item | Finding | Population | Source (location) | Status | URL (accessed 2026-11-17) |
|---|---|---|---|---|---|
| FISHMORPH traits | 10 traits per species: 9 unitless ratios measured from one lateral picture, plus maximum body length from FishBase. They include **pectoral-fin vertical position (PFv)**; PFv is set to 0 for species without pectoral fins. No longitudinal fin-origin ratios | **8,342** freshwater ray-finned species (48.69% of the world freshwater fauna); one adult picture each, males where species are dimorphic | F11 §2.1–2.3 | Verified | https://archimer.ifremer.fr/doc/00725/83668/99886.pdf |
| FISHMORPH dataset licence | **CC BY 4.0** (DataCite record for the figshare DOI). Not downloaded | — | F11 data DOI | Verified | https://api.datacite.org/dois/10.6084/m9.figshare.14891412 (figshare page itself returned 403) |
| FishBase morphometrics: method | Measured from one picture per species. Pre-pectoral, pre-dorsal, pre-pelvic and pre-anal lengths, body depth and head length as **% TL**, plus caudal-fin aspect ratio. n = 1 picture; no spread | per species | F12 | Verified | https://www.fishbase.se/physiology/MorphMetSummaryV2.php?genusname=Chrysichthys&id=9294&picname=Chbra_u1.gif&speciesname=brachynema |
| *Chrysichthys brachynema* (catfish) | SL 85.2; pre-pectoral **24.0**; pre-dorsal **33.0**; pre-pelvic **48.4**; pre-anal **63.3**; depth 22.1; head 27.6 (% TL); caudal AR 1.30 | 1 picture, unsexed | F12 | Verified | https://www.fishbase.se/physiology/MorphMetSummaryV2.php?genusname=Chrysichthys&id=9294&picname=Chbra_u1.gif&speciesname=brachynema |
| *Loweina rara* (lanternfish) | SL 85.2; pre-pectoral **22.1**; pre-dorsal **45.9**; pre-pelvic **35.5**; pre-anal **52.7**; depth 18.6; head 22.9 (% TL); caudal AR 1.05 | 1 picture, male, 4 cm SL | F12 | Verified | https://www.fishbase.se/physiology/MorphMetSummaryV2.php?genusname=Loweina&id=7427&picname=Lorar_m0.gif&speciesname=rara |
| *Hyalobagrus ornatus* (glass catfish) | SL 81.1; pre-pectoral **22.7**; pre-dorsal **32.1**; pre-pelvic **35.9**; pre-anal **45.2**; depth 17.5; head 23.2 (% TL); caudal AR 1.40 | 1 picture, unsexed, 3 cm TL | F12 | Verified | https://www.fishbase.se/physiology/MorphMetSummaryV2.php?genusname=Hyalobagrus&id=23084&picname=Hyorn_u0.jpg&speciesname=ornatus |
| *Nezumia atlantica* (grenadier; anal and dorsal fins run into the tail) | SL 95.1; pre-pectoral **20.1**; pre-dorsal **21.0**; pre-pelvic **21.0**; pre-anal **29.7**; depth 15.3; head 19.1 (% TL) | 1 picture, unsexed | F12 | Verified | https://www.fishbase.se/physiology/MorphMetSummaryV2.php?genusname=Nezumia&id=8509&picname=Neatl_u0.gif&speciesname=atlantica |
| FishBase licence | **CC BY-NC 4.0** (FishBase dataset on the GBIF IPT). Non-commercial: cite as facts only, and do not ship FishBase tables | — | F12 | Verified | https://www.gbif.se/ipt/resource?r=fishbase |

The common game species (trout, mackerel, tuna, eel, bluegill) returned "No morphometric data" without a picture-specific ID, so they are in Not found. The four rows above only show the range: the pre-pelvic position runs from 21% to 48% of TL, and the pelvic fin can sit ahead of the dorsal (*Loweina*) or behind it (*Chrysichthys*).

---

## 3. Snakes: the four ways a snake moves

### 3.1 Definitions (Jayne 2020, S2; CC BY 4.0)

| Mode | Defining features | Subtypes | Source (location) | Status | URL (accessed 2026-11-17) |
|---|---|---|---|---|---|
| Lateral undulation | All points move at once with **sliding** contact, as left and right bends travel down the whole body; points follow one path | 5: forward aquatic, backward aquatic, terrestrial, with a ventrolateral keel, arboreal (actively prevents sag) | S2, sections "Lateral undulation" and "Parameters for identifying limbless gaits" | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC7391877/ |
| Sidewinding | Posteriorly travelling side-to-side bends plus a **lifted** back between regions of **static** contact. Points follow different, nearly parallel paths. Tracks are disconnected, parallel and oblique to travel; within a contact region, posterior segments sit ahead of anterior ones | — | S2, "Sidewinding" | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC7391877/ |
| Concertina | Static plus sliding contact; head-to-tail distance changes as bunched loops anchor and the body then straightens forward. Anchoring loops form from front to back | 4: flat-surface, tunnel, arboreal with alternate bends, arboreal with helical wrapping | S2, "Concertina" | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC7391877/ |
| Rectilinear | No axial bending; the skin moves relative to the skeleton (costocutaneous muscles) | — | S2; S6 abstract | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC7391877/ ; https://pubmed.ncbi.nlm.nih.gov/29212845/ |
| Swimming vs crawling waves | When swimming, both amplitude and wavelength grow from head to tail, and the wave travels faster than the snake moves forward | — | S2, "Lateral undulation" | Verified (qualitative) | https://pmc.ncbi.nlm.nih.gov/articles/PMC7391877/ |
| Tail share of length (rig note) | the tail, which has no costocutaneous muscles or mobile skin, ranges from **< 5%** (some burrowers) to **> 40%** (slender arboreal species) of total length | — | S2, citing Sheehy et al. 2016 | Verified (as quoted) | https://pmc.ncbi.nlm.nih.gov/articles/PMC7391877/ |

### 3.2 Lateral undulation on a flat surface (Hu et al. 2009, S1)

| Quantity | Value | Spread | Population | Source (location) | Status | URL (accessed 2026-11-17) |
|---|---|---|---|---|---|---|
| Snakes | mass 15.2 g; length 36 cm | ± 2.6 g; ± 2.3 cm (mean ± SE) | 10 juvenile *Lampropeltis triangulum campbelli* | S1 Methods | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Wavelength | **0.4 L** | ± 0.1 | n = 20 filmed trials | S1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Amplitude A | **0.1 L** | ± 0.03 | same | S1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Forward speed | **0.22 L per undulation period**; short bursts to 0.4 | ± 0.08; observed range 0.15–0.4 | same | S1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Normalisation used | L = 30 cm and period τ = 2 s | — | same | S1 Results | Verified (see Sources disagree 3) | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Heading oscillation | mean orientation swings **20°** | ± 5° | same | S1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Friction on cloth (static) | forward μ_f **0.11**; backward μ_b **0.14**; transverse μ_t **0.20** | ± 0.011 / 0.015 / 0.015 | anaesthetised snakes, 90 measurements | S1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Friction on smooth fibreboard | nearly isotropic: μ_f ≈ 0.14, μ_t ≈ 0.16, μ_b ≈ 0.14 | — | same | S1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Uphill limit | the snakes slither forward only below an incline of **7°** | ± 2° | n = 70 trials, −20° to +20° | S1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Froude number (inertia vs friction) | Fr ≈ **0.1**; red racer ≈ 1–1.5 (L 60–130 cm, U 130 cm s⁻¹, cited) | — | — | S1 Model | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |
| Body lifting | lifting the curved parts raises model speed from 0.17 to 0.23 L per period (+35%) | model | — | S1 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/ |

### 3.3 Concertina

| Quantity | Value | Spread | Population | Source (location) | Status | URL (accessed 2026-11-17) |
|---|---|---|---|---|---|---|
| Burst speed in tunnels | means **13.5–16.8 cm s⁻¹** over 3, 5 and 7 cm tunnels (no significant width effect); fastest **20.8 cm s⁻¹** (7 cm tunnel) | range of means | 5 *Coluber constrictor* | S3 abstract | Verified (abstract) | https://doi.org/10.1242/jeb.156.1.539 |
| Endurance | longest mean **80 min** at 4.2 cm s⁻¹ in a 5 cm tunnel; **24 min** in a 3 cm tunnel; tested 4.2–5.6 cm s⁻¹ | — | same | S3 abstract | Verified (abstract) | https://doi.org/10.1242/jeb.156.1.539 |
| What changes with speed or width | Faster: higher frequency and faster moving phase; distance per cycle and fraction of time moving unchanged. Narrower tunnel: shorter distance per cycle, more static contacts, loops at a smaller angle to the wall | — | same | S3 abstract | Verified (abstract) | https://doi.org/10.1242/jeb.156.1.539 |
| Snakes (channel study) | L 61 cm, mass 42 g | ± 4 cm, ± 5 g | 3 six-month-old corn snakes, *Elaphe (Pantherophis) guttata* | S4 §2.1 | Verified | https://hu.gatech.edu/Publications/Hu12-marvi-concertina.pdf |
| Speed vs channel slope | **0.17 → 0.05** L per period from 0° to 60° (P = 0.0003) | — | same | S4 §5.3, Fig. 9a | Verified | https://hu.gatech.edu/Publications/Hu12-marvi-concertina.pdf |
| Wall push | up to **9×** body weight. Within a cycle, ~**8×** while moving the head or tail and ~**2×** (the minimum to avoid sliding) while moving the middle; up to 4× more than needed. Digging the ventral scales doubles friction | — | same | S4 Abstract and §5 | Verified | https://hu.gatech.edu/Publications/Hu12-marvi-concertina.pdf |

### 3.4 Rectilinear

| Quantity | Value | Spread | Population | Source (location) | Status | URL (accessed 2026-11-17) |
|---|---|---|---|---|---|---|
| Snakes and friction | Boa: L 53.3 cm, 0.06 kg, μ_f 0.3, μ_b 0.42. Dumeril's boa: 175.5 cm, 5.7 kg, μ_f 0.017, μ_b 0.06. Gaboon viper: 120 cm, 2.26 kg, μ_f 0.12, μ_b 0.32 | ± 1.5 / 0.01 / 0.06 / 0.05 (boa, N = 3); ± 10.6 / 1.1 / 0.002 / 0.01 (Dumeril's, N = 2); Gaboon N = 1 | *Boa constrictor*, *Acrantophis dumerili*, *Bitis gabonica* | S5 Table 1 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3673153/ |
| Speeds | **0.2–6 cm s⁻¹** (0.02–0.07 L s⁻¹) | range | same | S5 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3673153/ |
| Linear scaling with length L (cm), intercept 0 unless shown | mass 0.0267·L kg (r² 0.73); body speed 0.0275·L cm s⁻¹ (r² 0.79); wave speed 0.107·L (0.95); wave amplitude 0.01·L cm (0.86); frequency 0.0028·L Hz (0.81); wavelength 0.158·L + 15.4 cm (r² 0.64, n.s., P = 0.057); fraction of body at rest −0.0002·L + 0.27 (n.s., P = 0.77) | r² as printed; wave-parameter SDs < 20% (one red-tailed boa 40%) | same | S5 Table 2 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3673153/ |
| Frequency rises with size | **0.2 → 0.5 Hz** as L goes from 52 to 183 cm. In concertina and lateral undulation, larger snakes use lower frequencies | — | same | S5 §6 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3673153/ |
| Body lifting | local lifting raises model speed by **31%** | model | — | S5 Abstract | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3673153/ |
| Muscle roles | CCI active during static contact (propulsion); CCS during sliding contact (recovery); interscutalis held isometric | — | *Boa constrictor* | S6 abstract | Verified (abstract) | https://pubmed.ncbi.nlm.nih.gov/29212845/ |
| After a large meal (30–37% of body mass) | Still rectilinear. Of 11 kinematic variables only two changed: cyclic height change more than ×1.5, ventral-skin movement relative to the skeleton more than 25% lower | — | *B. constrictor* | S7 abstract | Verified (abstract) | https://pubmed.ncbi.nlm.nih.gov/38563306/ |
| Newman & Jayne speed range 0.6–2.2% SVL s⁻¹; cycle 4.70 ± 0.29 s; frequency 0.22 ± 0.01 Hz; duty factor 29.0 ± 3.2% (unfed) | — | — | *B. constrictor* | via Grok (from S6 and S7 full texts) | **Unverified**: JEB behind Cloudflare | https://doi.org/10.1242/jeb.247042 (not opened) |

### 3.5 Sidewinding

| Quantity | Value | Spread | Population | Source (location) | Status | URL (accessed 2026-11-17) |
|---|---|---|---|---|---|---|
| Snakes | mass 98 g; L (tip to tail) 48 cm | ± 18 g; ± 6 cm | 6 *Crotalus cerastes*; 54 trials (6 snakes × 3 inclines × 3) | S9 (arXiv v1), main text | Verified | https://arxiv.org/abs/1410.2945 |
| Two-wave model | horizontal and vertical waves, phase offset **1.51 rad** | ± 0.17 rad | same | S9 main text, Fig. 1e | Verified | https://arxiv.org/abs/1410.2945 |
| Track angle to direction of travel | **33°** | ± 8° (granular media) | same | S9 main text | Verified (see Sources disagree 5) | https://arxiv.org/abs/1410.2945 |
| Contacts on level sand | **two** contact patches at each instant; track line length = body length | — | same | S9 main text | Verified | https://arxiv.org/abs/1410.2945 |
| Contact length on slopes | contact length / L rises **41%** from 0° to 20° (P < 0.0001); lifted parts clear the sand; frequency and speed fall with slope; step length stays constant | — | same | S9 main text, Fig. 2c | Verified | https://arxiv.org/abs/1410.2945 |
| Sand intrusion | **1.4 cm** | ± 1.3 | same | S9 main text | Verified | https://arxiv.org/abs/1410.2945 |
| Sand limit | the sand's maximum stable slope is 27°; the robot failed on inclines of ≈ 10° until contact length was increased | — | sand from Yuma County, AZ | S9 main text | Verified | https://arxiv.org/abs/1410.2945 |
| Turning | differential turns **26.3°** per cycle (max 66.9°); reversal turns **89.4°** per cycle (max 176.6°). Reversal = a **180°** jump in vertical-wave phase (vs 1° in straight sidewinding or differential turns). Straight sidewinding uses a phase offset of ±90° | ± 18.3° / ± 28.2° SD; phase ± 20° / ± 11° | *C. cerastes* | S10 Abstract and Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC4434722/ |
| Moving vs static segment angle | 40.4° | ± 17.4° | same | S10 Results | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC4434722/ |
| Performance and cost | maximum burst **3.7 km h⁻¹** (SVL 41.9 cm snake). Endurance at 0.5 km h⁻¹: 33 to > 180 min; at 0.7 km h⁻¹: 9–52 min; mean at 0.60 km h⁻¹: 73 min. Net cost of transport **0.408 ml O₂ g⁻¹ km⁻¹** (8 snakes, mean 110 g), lower than lateral undulation or concertina in *Coluber*. Frequency rises linearly with speed. Log-log mass slopes: burst speed 0.29, endurance 1.01 | ranges as printed | *C. cerastes* | S8 abstract | Verified (abstract) | https://doi.org/10.1242/jeb.163.1.1 |

**Engine summary (as printed above, nothing new).** Lateral undulation: λ ≈ 0.4 L, amplitude ≈ 0.1 L, ≈ 0.2 L of travel per cycle, continuous sliding contact. Concertina: alternating anchor loops, much slower (≈ 0.05–0.17 L per cycle). Rectilinear: no bending, 1–6 cm s⁻¹, 0.2–0.5 Hz, about a quarter of the body at rest. Sidewinding: two orthogonal waves about 90° out of phase, two static contacts, tracks at about 33° to the direction of travel.

---

## 4. Not found

- **Videler & Hess 1984, Bainbridge 1958, Gillis 1998, Donley & Dickson 2000 and Dewar & Graham 1994 tables** (per-species wavelength, amplitude envelope, f against U with SD). These JEB PDFs sit behind a Cloudflare challenge from both curl and the box browser on 2026-11-17. Only abstracts were read; the Bainbridge, Videler and Gillis numbers stay Unverified.
- **Taylor, Nudds & Thomas 2003** per-species Strouhal tables: Nature paywall; abstract only.
- **Di Santo et al. 2021 per-species values** (Dataset S2 and the Dryad CC0 data): exist, but were not downloaded under the dataset rule. They are the best route to per-species wavelength and envelope coefficients.
- **A compiled multi-species table of longitudinal fin positions with spread**: none found. FISHMORPH covers only the pectoral fin's vertical position. FishBase gives single-picture % TL values, and none came back for trout, mackerel, tuna, eel or bluegill without a picture-specific ID.
- **Lateral-undulation speeds in body lengths per second across species** (Jayne 1986, Copeia): JSTOR, not opened.
- **Fraction of body in ground contact during lateral undulation**: S1 discusses lifting but prints no single value.
- **Absolute sidewinding contact length** at 0°, 10° and 20°: plotted in S9 Fig. 2c, with only the +41% change printed in the text.
- **Concertina per-cycle kinematics** (S3 Table 1): JEB full text not opened.
- **Webb 1975** (Bull. Fish. Res. Board Can. 190), the classic synthesis: there is a DFO scan, but tables were not extracted, to save usage.

## 5. Sources disagree

1. **Amplitude convention.** Tytell defines A as **half** the total excursion (eel tail A ≈ 0.07 L, St = 2fA/U ≈ 0.31–0.32). Di Santo's tail "amplitude" (median 0.18 L) matches a **peak-to-peak** reading, since their methods describe the envelope as "peak-to-peak amplitude". Taylor et al. define St = fA/U; their abstract does not say whether A is peak-to-peak. Halve Di Santo's values before combining them with Tytell's. Di Santo's methods also call amplitude the "maximal excursion on the y-axis", so the paper is not fully consistent with itself.
2. **Eel wavelength.** Tytell 0.60 L (3 eels, ~1.4 L s⁻¹) vs Di Santo 0.58 L (eel at 1 BL s⁻¹) vs the anguilliform median 0.75 L. The anguilliform median includes amphioxus, hagfish and lamprey, and amphioxus alone reaches 1.04–1.49 L.
3. **Hu et al. 2009 body length.** The methods give L = 36 ± 2.3 cm, but the kinematics are normalised by L = 30 cm. Speeds per period depend on which L is used.
4. **Tytell & Lauder 2004 Table 1 label.** It prints "Average tail velocity 4A/f", but the companion paper (Tytell 2004, Fig. 1A) defines U_tail = 4Af. 4Af matches the printed 0.856 L s⁻¹ (4 × 0.0693 × 3.11 = 0.862), so the table label is a typo.
5. **Sidewinding track angle.** The arXiv preprint (S9) gives 33 ± 8°. Grok reported 33.8° as "track angle α", and I could not find 33.8 in the preprint.
6. **Mode labels vs kinematics.** The classic scheme (Breder; Webb) expects head amplitude to fall and wavelength to rise from eel to tuna. Di Santo et al. found wavelength rising, but **no** decrease in head:tail amplitude. Head amplitude at ~1 BL s⁻¹ was actually highest in tuna, and across all speeds highest in anguilliforms.
7. **Thunniform wavelength.** Dewar & Graham 1994: yellowfin tuna 1.23–1.29 L (fork length). Di Santo: thunniform median 1.14 L; yellowfin at 1 BL s⁻¹: 1.17 L (total length). Part of the gap is fork length vs total length.

## 6. What the Grok pass added or contradicted

**Added, then confirmed in the primary source:** the open Lauder-lab copies of Tytell & Lauder 2004 and Tytell 2004 II (Table 1 values, α, slip and St all matched); Marvi et al. 2014 on arXiv (n, mass, length, 1.51 ± 0.17 rad, 1.4 ± 1.3 cm intrusion, +41% contact length); Marvi, Bridges & Hu 2013 (frequency 0.2 → 0.5 Hz); FISHMORPH (8,342 species, PFv; dataset CC BY 4.0); the Dryad deposit for Di Santo (CC0); Dewar & Graham 1994 (1.23–1.29 L); the Donley & Dickson sample; the Tytell 2004 Proc B steady St ≈ 0.32.

**Added but left Unverified:** Bainbridge's V = ¼L(3f − 4) rule and the 0.2 L maximum amplitude; Videler & Hess values; Gillis 1998 values; the Newman & Jayne and Petersen rectilinear speeds and duty factors.

**Contradicted or corrected:** Grok's sidewinding track angle of 33.8° vs 33 ± 8° in the preprint (Sources disagree 5). Grok described Hu 2009 as "L ≈ 30 cm"; that is the normalisation length, and the measured length is 36 ± 2.3 cm (Sources disagree 3). Grok's amplitude-convention warning was correct and is item 1 above.

**Leads I found that Grok did not report:** Jayne & Davis 1991 concertina burst and endurance figures, Secor et al. 1992 sidewinding performance and cost, Marvi & Hu 2012 concertina wall forces, and FishBase per-species % TL fin positions.

---

## 7. Sources

All accessed 2026-11-17. Link check (curl, browser user agent): PMC pages, Lauder-lab PDFs, the Hu-lab PDF, arXiv, Archimer, FishBase pages, GBIF IPT and Dryad returned HTTP 200. PubMed returned 203 with content. figshare returned 403, so the DataCite record is used instead. JEB (journals.biologists.com) returned a Cloudflare 403 challenge, so those rows use DOIs.

- **F1** Di Santo V, Goerig E, Wainwright DK, Akanyeti O, Liao JC, Castro-Santos T, Lauder GV. 2021. Convergence of undulatory swimming kinematics across a diversity of fishes. *PNAS* 118(49):e2113206118. doi:10.1073/pnas.2113206118. https://pmc.ncbi.nlm.nih.gov/articles/PMC8670443/. **Licence:** "Published under the PNAS license" (free to read, not CC). Data on Dryad, doi:10.5061/dryad.bg79cnp9x (https://doi.org/10.5061/dryad.bg79cnp9x), **CC0** per DataCite; not downloaded.
- **F2** Tytell ED, Lauder GV. 2004. The hydrodynamics of eel swimming: I. Wake structure. *J. Exp. Biol.* 207:1825–1841. doi:10.1242/jeb.00968. Author-hosted PDF: https://sites.harvard.edu/glauder/files/2022/03/Tytell.Lauder.2004.pdf. © Company of Biologists 2004; free copy on the lab site; no open licence.
- **F3** Tytell ED. 2004. The hydrodynamics of eel swimming II. Effect of swimming speed. *J. Exp. Biol.* 207:3265–3279. doi:10.1242/jeb.01139. https://sites.harvard.edu/glauder/files/2022/03/Tytell.2004.pdf. © Company of Biologists 2004; no open licence.
- **F4** Tytell ED. 2004. Kinematics and hydrodynamics of linear acceleration in eels, *Anguilla rostrata*. *Proc. R. Soc. B* 271:2535–2540. doi:10.1098/rspb.2004.2901. https://pmc.ncbi.nlm.nih.gov/articles/PMC1691903/. © Royal Society; free on PMC; abstract used.
- **F5** Dewar H, Graham JB. 1994. Studies of tropical tuna swimming performance in a large water tunnel. III. Kinematics. *J. Exp. Biol.* 192:45–59. doi:10.1242/jeb.192.1.45. Abstract: https://pubmed.ncbi.nlm.nih.gov/9317308/. © Company of Biologists; abstract only.
- **F6** Donley JM, Dickson KA. 2000. Swimming kinematics of juvenile kawakawa tuna (*Euthynnus affinis*) and chub mackerel (*Scomber japonicus*). *J. Exp. Biol.* 203:3103–3116. doi:10.1242/jeb.203.20.3103. https://pubmed.ncbi.nlm.nih.gov/11003821/. © Company of Biologists; abstract only.
- **F7** Taylor GK, Nudds RL, Thomas ALR. 2003. Flying and swimming animals cruise at a Strouhal number tuned for high power efficiency. *Nature* 425:707–711. doi:10.1038/nature02000. https://pubmed.ncbi.nlm.nih.gov/14562101/. © Nature Publishing Group; abstract only.
- **F8** Bainbridge R. 1958. *J. Exp. Biol.* 35:109–133. doi:10.1242/jeb.35.1.109. Not opened (Unverified rows only).
- **F9** Videler JJ, Hess F. 1984. *J. Exp. Biol.* 109:209–228. doi:10.1242/jeb.109.1.209. Not opened (Unverified rows only).
- **F10** Gillis GB. 1998. *J. Exp. Biol.* 201:949–961. doi:10.1242/jeb.201.7.949. Not opened (Unverified rows only).
- **F11** Brosse S, Charpin N, Su G, Toussaint A, Herrera-R GA, Tedesco PA, Villéger S. 2021. FISHMORPH: a global database on morphological traits of freshwater fishes. *Global Ecology and Biogeography* 30(12):2330–2336. doi:10.1111/geb.13395. Manuscript on Archimer: https://archimer.ifremer.fr/doc/00725/83668/99886.pdf (Wiley article; the publisher's version is not open). Dataset doi:10.6084/m9.figshare.14891412, **CC BY 4.0** (DataCite: https://api.datacite.org/dois/10.6084/m9.figshare.14891412); not downloaded.
- **F12** FishBase (Froese R, Pauly D, eds), Morphometrics pages (URLs in the §2 rows). **CC BY-NC 4.0** per the FishBase dataset on the GBIF IPT, https://www.gbif.se/ipt/resource?r=fishbase. FishBase's own citation page (https://www.fishbase.org/summary/citation.php, 403 from the box) restricts reuse to non-commercial use. **Non-commercial.**
- **S1** Hu DL, Nirody J, Scott T, Shelley MJ. 2009. The mechanics of slithering locomotion. *PNAS* 106(25):10081–10085. doi:10.1073/pnas.0812533106. https://pmc.ncbi.nlm.nih.gov/articles/PMC2700932/. © PNAS; free on PMC; no CC licence shown.
- **S2** Jayne BC. 2020. What defines different modes of snake locomotion? *Integrative and Comparative Biology* 60(1):156–170. doi:10.1093/icb/icaa017. https://pmc.ncbi.nlm.nih.gov/articles/PMC7391877/. **CC BY 4.0.**
- **S3** Jayne BC, Davis JD. 1991. Kinematics and performance capacity for the concertina locomotion of a snake (*Coluber constrictor*). *J. Exp. Biol.* 156:539–556. doi:10.1242/jeb.156.1.539 (https://doi.org/10.1242/jeb.156.1.539; abstract read through the search index of the DOI page, since direct fetches hit Cloudflare). © Company of Biologists; abstract only. A third-party full-text copy exists, but it is not a legitimate host and was not used.
- **S4** Marvi H, Hu DL. 2012. Friction enhancement in concertina locomotion of snakes. *J. R. Soc. Interface* 9(76):3067–3080. doi:10.1098/rsif.2012.0132. Author-hosted PDF: https://hu.gatech.edu/Publications/Hu12-marvi-concertina.pdf (PMC3479897 returned a bot check). © 2012 Royal Society; no open licence.
- **S5** Marvi H, Bridges J, Hu DL. 2013. Snakes mimic earthworms: propulsion using rectilinear travelling waves. *J. R. Soc. Interface* 10:20130188. doi:10.1098/rsif.2013.0188. https://pmc.ncbi.nlm.nih.gov/articles/PMC3673153/. © 2013 The Author(s), published by the Royal Society, "All rights reserved"; free on PMC.
- **S6** Newman SJ, Jayne BC. 2018. Crawling without wiggling: muscular mechanisms and kinematics of rectilinear locomotion in boa constrictors. *J. Exp. Biol.* 221:jeb166199. doi:10.1242/jeb.166199. https://pubmed.ncbi.nlm.nih.gov/29212845/. © Company of Biologists; abstract only.
- **S7** Petersen JC, Jayne BC, Wilde AD, Capano JG, Roberts TJ. 2024. Effects of ingesting large prey on the kinematics of rectilinear locomotion in *Boa constrictor*. *J. Exp. Biol.* 227(8):jeb247042. doi:10.1242/jeb.247042. https://pubmed.ncbi.nlm.nih.gov/38563306/. © Company of Biologists; abstract only.
- **S8** Secor SM, Jayne BC, Bennett AF. 1992. Locomotor performance and energetic cost of sidewinding by the snake *Crotalus cerastes*. *J. Exp. Biol.* 163:1–14. doi:10.1242/jeb.163.1.1 (https://doi.org/10.1242/jeb.163.1.1; abstract via the search index of the DOI page). © Company of Biologists; abstract only.
- **S9** Marvi H, Gong C, Gravish N, Astley H, Travers M, Hatton RL, Mendelson JR III, Choset H, Hu DL, Goldman DI. 2014. Sidewinding with minimal slip: snake and robot ascent of sandy slopes. *Science* 346:224–229. doi:10.1126/science.1255718. Read as arXiv:1410.2945v1 (preprint, 11 Oct 2014): https://arxiv.org/abs/1410.2945. © AAAS for the journal version; the preprint is under arXiv's default non-exclusive distribution licence, not CC. Values may differ slightly from the Science version (Sources disagree 5).
- **S10** Astley HC, Gong C, Dai J, Travers M, Serrano MM, Vela PA, Choset H, Mendelson JR III, Hu DL, Goldman DI. 2015. Modulation of orthogonal body waves enables high maneuverability in sidewinding locomotion. *PNAS* 112(19):6200–6205. doi:10.1073/pnas.1418965112. https://pmc.ncbi.nlm.nih.gov/articles/PMC4434722/. © PNAS; free on PMC; no CC licence shown.
