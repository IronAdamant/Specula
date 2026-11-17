# C9: Group behaviour (flocking, schooling, herding, flight distances, daily rhythm)

*Creature Data, item C9 of the Specula private library. These are private research notes, not for publication.*

- **Web reading:** 2026-11-17, about 10:44–11:18 AEDT. All URLs were accessed on 2026-11-17.
- **Grok pass:** grok-4.6 with live search, reasoning effort xhigh. It ran **2026-11-17 10:46–11:09 AEDT** as one focused pass with six questions. The output is in `/workspace/c9work/grok_c9.txt`, and the prompt is in `grok_prompt.txt`. I treated Grok's output as unverified leads: a Grok number is marked Verified only after I read it in the primary text or a publisher excerpt myself.
- **Scratch:** `/workspace/c9work`, not in any repo. No datasets were downloaded, only article PDFs and texts for reading.
- **Status key:** these follow C8.
  - **Verified:** read in the open full text.
  - **Verified (preprint):** read in the authors' arXiv version, not the published version of record.
  - **Verified (abstract).**
  - **Verified (search/publisher excerpt):** read in a search-index excerpt of the publisher's full text, because the publisher page blocks scripts.
  - **Verified (model parameter):** a simulation setting, not a measurement of animals.
  - **Unverified:** the reason is given in the row.
  - **Computed by me:** marked where I derived a summary such as a mean from a published table.
- **Licence flags:**
  - ⚠NC: non-commercial terms.
  - ⚠ND: no derivatives.
  - ⚠ARR: publisher copyright, or "free to read" without a reuse licence. Use the numbers and facts only; don't reuse the text or figures.
  - PNAS "open access option" papers in PMC are free to read, but PMC shows no CC licence for them, so I flag them ⚠ARR.

**Row count: 64 data rows: 59 Verified (all sub-kinds), 4 Unverified (rows 34, 43, 52, 56) and 1 partly verified (row 33: main-text values confirmed, SI values not).**

## 1. Steering models: boids-style rules and published parameter sets

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 1 | Boids rule order | Three rules in decreasing precedence: (1) collision avoidance, (2) velocity matching, (3) flock centring. Acceleration requests are combined by priority up to a maximum acceleration, which is a fraction of maximum speed | qualitative | Model (Reynolds boids) | — | Reynolds 1987, "Simulated Flocks" section (author-hosted OCR reprint) | ⚠ARR (ACM; author reprint) | Verified | https://www.red3d.com/cwr/papers/1987/SIGGRAPH87.pdf |
| 2 | Boids neighbourhood | A sphere around the boid with sensitivity falling as an inverse exponential of distance, so it has two parameters, a radius and an exponent. **The paper gives no numeric values** for either, or for rule weights. Linear distance weighting gave a "bouncy" flock; it was changed to inverse-square | qualitative | Model | — | Reynolds 1987, "Simulated Perception" section | ⚠ARR | Verified (model parameter) | https://www.red3d.com/cwr/papers/1987/SIGGRAPH87.pdf |
| 3 | Boids obstacle avoidance and cost | Steer-to-avoid aims one body length beyond the obstacle's silhouette edge. 80 boids took about 95 s of compute per frame (O(N²)), so a 10 s, 300-frame test took about 8 h on a Symbolics 3600 | body lengths; s/frame | Model, 1987 hardware | — | Reynolds 1987, "Environmental Obstacles" and "Notes on Implementation" | ⚠ARR | Verified (model parameter) | https://www.red3d.com/cwr/papers/1987/SIGGRAPH87.pdf |
| 4 | Couzin zonal model, parameter ranges | N 10–100; zone of repulsion r_r = 1 (the length unit); orientation-zone width Δr_o 0–15; attraction-zone width Δr_a 0–15; field of perception α 200–360°; turning rate θ 10–100 °/s; speed s 1–5 units/s; heading error SD 0–11.5° (0–0.2 rad); time step τ 0.1 s | units scaled to r_r; °, s | Model (3D, fish-like) | ranges explored | Couzin et al. 2002, Table 1 | ⚠ARR (Elsevier) | Verified (search/publisher excerpt; model parameter) | https://doi.org/10.1006/jtbi.2002.3065 |
| 5 | Couzin defaults for the phase diagrams | N = 100, r_r = 1, α = 270°, θ = 40 °/s, s = 3 units/s, error = 0.05 rad; each point is the mean of 30 runs. States: swarm, torus (mill), dynamic parallel, highly parallel; at some settings there is a >50 % chance the group splits | as #4 | Model | 30 replicates | Couzin 2002, Fig. 3 caption | ⚠ARR | Verified (search/publisher excerpt; model parameter) | https://doi.org/10.1006/jtbi.2002.3065 |
| 6 | Vicsek model | Constant speed v = 0.03 (distance unit = interaction radius r = 1; Δt = 1); each step a particle takes its neighbours' mean heading plus uniform noise in [−η/2, η/2]. Critical noise η_c(∞) = 2.9 ± 0.05 at density ρ = 0.4; exponents β = 0.45 ± 0.07 and δ = 0.35 ± 0.06. Example runs: N = 300; L = 25, η = 0.1 (groups form); L = 5, η = 0.1 (ordered) | dimensionless | Model (2D, periodic) | ± as given | Vicsek et al. 1995 (arXiv reprint cond-mat/0611743), text and Fig. 1 caption | ⚠ARR (APS); arXiv copy | Verified (preprint; model parameter) | https://arxiv.org/abs/cond-mat/0611743 |
| 7 | StarDisplay (starling) model: timing and flight | Integration step 5 ms; reaction time 50 ms; cruise speed 10 m/s; mass 0.08 kg; lift/drag 3.3; default lift 0.78 N; default drag = thrust 0.24 N; speed-control time constant 1 s | ms; m/s; kg; N | Model of *Sturnus vulgaris* | — | Hemelrijk & Hildenbrandt 2011, Table 1 | CC BY | Verified (model parameter) | https://doi.org/10.1371/journal.pone.0022479 |
| 8 | StarDisplay: neighbour rules | Topological range n_c = 6.5 neighbours; maximum perception radius 100 m; "hard sphere" 0.2 m; separation radius 4 m; Gaussian σ = 1.37 m; weights: separation 1 N, cohesion 1 N, alignment 0.5 N, random 0.01 N; rear blind angle for cohesion and alignment 2 × 45°; banking control w_in = 10, w_out = 1; interior if centrality < 0.35 | as stated | Model | — | Hemelrijk & Hildenbrandt 2011, Table 1 | CC BY | Verified (model parameter) | https://doi.org/10.1371/journal.pone.0022479 |
| 9 | StarDisplay: roost boundary | Roost radius 150 m; horizontal boundary weight 0.01 N/m; vertical boundary weight 0.2 N | m; N | Model | — | Hemelrijk & Hildenbrandt 2011, Table 1 | CC BY | Verified (model parameter) | https://doi.org/10.1371/journal.pone.0022479 |
| 10 | Surf scoter best-fit zonal model | Relative force weights: repulsion ω_rep = 8.5, attraction ω_att = 1.2, alignment ω_al = 0.74, frontal one-neighbour interaction ω_front = 0.08, noise ω_ξ = 0.37 (sensitivity tested at ±50 %). Zones are circular: short-range repulsion, middle-range alignment, longer-range attraction | dimensionless weights | Model fitted to *Melanitta perspicillata* flocks | ±50 % sensitivity range | Lukeman, Li & Edelstein-Keshet 2010, Fig. 4 caption | ⚠ARR (PNAS) | Verified (search/publisher excerpt; model parameter) | https://doi.org/10.1073/pnas.1001763107 |
| 11 | Sheep herd model (Ginelli) | Walking speed v₁ = 0.15 m/s; running speed v₂ = 1.5 m/s; τ(1→0) = 8 s; τ(0→1) = 35 s; τ(0,1→2) = τ(2→0) = N s; d_S = 6.3 m; d_R = 31.6 m (= 5 d_S); r_e = r₀ = 1 m (about one body length); noise η = 0.13 (range tested 0.1–0.5); cohesion β = 0.8. Realistic packing needs strong imitation (α > 5, δ > 2) | m/s; s; m | Model, N = 100 | ranges as given | Ginelli et al. 2015, Results "Parameter Estimation" and SI | ⚠ARR (PNAS) | Verified (search/publisher excerpt; model parameter) | https://doi.org/10.1073/pnas.1503749112 |

## 2. Bird flocks: measured values

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 12 | Topological interaction range | n_c = 6.5 ± 0.9 neighbours; the text says "6–7". The interaction does not depend on metric distance: n_c vs r₁ gives R² = 0.00021 (P = 0.97, n = 10); r_c vs r₁ gives R² = 0.78 (P = 0.00072) | birds | *S. vulgaris*, winter roost in Rome, 10 flocks | ± SE | Ballerini et al. 2008 PNAS (arXiv 0709.1916), text with Fig. 3 | ⚠ARR (PNAS) | Verified (preprint) | https://arxiv.org/abs/0709.1916 · https://doi.org/10.1073/pnas.0711437105 |
| 13 | Nearest-neighbour distance r₁ | 0.68 m in the densest flock to 1.51 m in the sparsest. Distance to the n-th neighbour scales as r ≈ r₁·n^(1/3) | m | As #12 | range over 10 flocks | Ballerini 2008 PNAS, text; Table 1 of Ballerini 2008 *Anim Behav* | ⚠ARR | Verified (preprint) | https://arxiv.org/abs/0709.1916 |
| 14 | Flock size, density and speed (10 events) | N = 448–2631 birds; density 0.04–0.80 birds/m³; r₁ 0.68–1.51 m; centre-of-mass speed 6.9–15.2 m/s. Mean speed 10.6 m/s (SD 2.3) is computed by me from the table | birds; m⁻³; m; m/s | *S. vulgaris*, Rome, 10 events | range; SD computed by me | Ballerini et al. 2008 *Anim Behav* 76:201, Table 1 (arXiv 0802.1667) | ⚠ARR (Elsevier) | Verified (preprint) | https://arxiv.org/abs/0802.1667 · https://doi.org/10.1016/j.anbehav.2008.02.004 |
| 15 | Flock shape | Thin and flat. Thickness I₁ 5.29–19.0 m; aspect ratios I₂/I₁ = 2.8 ± 0.4 and I₃/I₁ = 5.6 ± 1.0. The thin axis is nearly vertical (absolute I₁·G = 0.93 ± 0.04), and the velocity is nearly horizontal (absolute V·G = 0.13 ± 0.05) | m; ratio | As #14 | 95 % CI | Ballerini 2008 *Anim Behav*, Results and Table 1 | ⚠ARR | Verified (preprint) | https://arxiv.org/abs/0802.1667 |
| 16 | Flocking-event selection | About 500 events were filmed over 40 sessions; 10 were analysable (N > 400, sharp borders). Wind was never above 12 m/s; cameras ran at 10 fps | count; m/s | As #14 | — | Ballerini 2008 *Anim Behav*, Methods | ⚠ARR | Verified (preprint) | https://arxiv.org/abs/0802.1667 |
| 17 | Polarisation (alignment order) | Φ = 0.96 ± 0.03 over 24 flocks; per-flock range 0.844–0.995 (Table S1) | 0–1 | *S. vulgaris*, Rome, 24 flocks of 122–4268 birds | ± SD | Cavagna et al. 2010, text and Table S1 (arXiv 0911.4393) | ⚠ARR (PNAS) | Verified (preprint) | https://arxiv.org/abs/0911.4393 · https://doi.org/10.1073/pnas.1005766107 |
| 18 | Flock speed and size (24 flocks) | Centre-of-mass speed 6.8–19.1 m/s; mean 11.6 m/s (SD 2.5), computed by me. Flock linear size L 9.1–85.7 m | m/s; m | As #17 | range; SD computed by me | Cavagna 2010, Table S1 | ⚠ARR | Verified (preprint) | https://arxiv.org/abs/0911.4393 |
| 19 | Correlation length | Velocity-fluctuation correlation length ξ grows linearly with flock size L ("scale-free"); it isn't fixed in metres or in birds | qualitative / proportional | As #17 | — | Cavagna 2010, abstract and Fig. 2C | ⚠ARR | Verified (abstract) | https://doi.org/10.1073/pnas.1005766107 |
| 20 | Leadership delay in pigeon flocks | Directional-correlation delay between pairs: 0.37 s (± 0.27 SD) with C_min = 0.5, and 0.32 s (± 0.20 SD) with C_min = 0.9. Delays in example pairs are as short as 0.07–0.14 s | s | Homing pigeons *Columba livia*, flocks of up to 10, GPS tracks | ± SD | Nagy et al. 2010 *Nature* 464:890 (arXiv 1010.5394), text and Fig. 3 | ⚠ARR (Nature) | Verified (preprint) | https://arxiv.org/abs/1010.5394 · https://doi.org/10.1038/nature08891 |
| 21 | Surf scoter spacing and speed | Mean NND 1.45 body lengths (SD 0.25 BL); mean speed 2.0 BL/s; preferred neighbour distance about 1.5 BL; there's a preference for a neighbour directly in front | BL; BL/s | *Melanitta perspicillata* flocks of hundreds on water | ± SD | Lukeman 2010, Results; Fig. 4 caption | ⚠ARR (PNAS) | Verified (search/publisher excerpt) | https://doi.org/10.1073/pnas.1001763107 |
| 22 | Bird visual field (as stated by Reynolds) | Field of view about 300°; binocular overlap 10–15° | degrees | "real birds", general | — | Reynolds 1987, "Simulated Perception" (secondary statement, no citation) | ⚠ARR | Verified (as quoted; general claim, not a measurement) | https://www.red3d.com/cwr/papers/1987/SIGGRAPH87.pdf |

## 3. Fish schools and insect swarms

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 23 | Golden shiner set-up | Mean length about 5 cm (BL = 5.2 cm in Methods); groups of 30, 70 and 150 (7 replicates each) and 300 (3 replicates); tank 2.1 × 1.2 m, 5 cm water depth; 56 min at 30 fps | cm; count | *Notemigonus crysoleucas*, lab | — | Tunstrøm et al. 2013, Results and Methods | CC BY | Verified | https://doi.org/10.1371/journal.pcbi.1002915 |
| 24 | State thresholds (school, mill, swarm) | Polar: O_p > 0.65 and O_r < 0.35. Milling: O_p < 0.35 and O_r > 0.65. Swarm: both < 0.35 (O_p = polarisation, O_r = rotation order, each 0–1) | 0–1 | As #23 | — | Tunstrøm 2013, Results and Methods | CC BY | Verified | https://doi.org/10.1371/journal.pcbi.1002915 |
| 25 | Time in each state vs group size | Polar: 0.57 → 0.26 → 0.18 → 0.05 (for 30 → 70 → 150 → 300 fish). Milling: 0.03 → 0.18 → 0.36 → 0.45. Swarm: 0.09–0.11; transition region: 0.31–0.44 (Fig. 4A) | fraction of time | As #23 | over replicates (SD in figure only) | Tunstrøm 2013, Results (Fig. 2; Fig. 4A) | CC BY | Verified | https://doi.org/10.1371/journal.pcbi.1002915 |
| 26 | Local-order radius | Local polarisation measured within a 15.5 cm radius (Results) or 15.6 cm, about 3 BL (Methods); neighbourhoods of 1–3 BL tested | cm; BL | As #23 | — | Tunstrøm 2013 | CC BY | Verified (two values in the paper; see Sources disagree) | https://doi.org/10.1371/journal.pcbi.1002915 |
| 27 | Golden shiner interaction rules | Speed regulation dominates; alignment emerges from attraction and repulsion; fish copy direction changes of those ahead; no explicit orientation matching; three-body effects matter (2- and 3-fish groups; structure persists at 10 and 30) | qualitative | *N. crysoleucas* | — | Katz et al. 2011, abstract | ⚠ARR (PNAS) | Verified (abstract) | https://doi.org/10.1073/pnas.1107583108 |
| 28 | Mosquitofish interaction rules | Attraction keeps the group together; only weak alignment; repulsion acts mainly through speed changes; each fish responds only to its single nearest neighbour | qualitative | *Gambusia holbrooki* | — | Herbert-Read et al. 2011, abstract | ⚠ARR (PNAS) | Verified (abstract) | https://doi.org/10.1073/pnas.1109355108 |
| 29 | Midge swarm polarisation | p ≤ 0.09 in every swarm (bird flocks are near 1) | 0–1 | *Chironomus riparius*, lab, 91 cm enclosure, 23 °C | max | Kelley & Ouellette 2013, Results | ⚠NC ⚠ND (CC BY-NC-ND 3.0) | Verified | https://doi.org/10.1038/srep01073 |
| 30 | Midge swarm size | Swarms typically 20–30 cm across; swarm size R_s ∝ ⟨N⟩^(1/3), so number density is roughly constant. Midges act as if held in a quadratic (spring-like) potential well, i.e. a linear restoring force; mean-square displacement follows a power law up to t ≈ 0.5 s | cm; scaling | As #29 | — | Kelley & Ouellette 2013, Results and Methods | ⚠NC ⚠ND | Verified | https://doi.org/10.1038/srep01073 |

## 4. Herds (sheep)

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 31 | Grazing herd cycle | 100 same-age ewes in an 80 × 80 m flat arena; five 1-h trials filmed at 1 frame/s from a 7 m tower. The herd slowly spreads while grazing, then packs fast; the largest packing events come about every 15 min. Packing is started by a peripheral sheep running to the centre, and there are no alarm calls | m; min | Merino *Ovis aries* | — | Ginelli et al. 2015, Results | ⚠ARR (PNAS) | Verified (search/publisher excerpt) | https://doi.org/10.1073/pnas.1503749112 |
| 32 | Sheep speeds | Maximum recorded individual speed about 1.5 m/s; error on group (centre-of-mass) speed about 0.03 m/s. Speed distribution of the centre of mass has a power-law tail, exponent γ = −2.3(2). Walking about 0.1 m/s and running about 1 m/s (order of magnitude, Fig. S4A) | m/s | As #31 | as given | Ginelli 2015, Results (Fig. 2) and SI | ⚠ARR | Verified (search/publisher excerpt) | https://doi.org/10.1073/pnas.1503749112 |
| 33 | Sheep packing density and gait fractions | Main text (verified): packed density about 1.3 sheep/m², spread to about 1 sheep per 23 m²; about 74 % of time motionless; speed threshold about 0.41 m/s. SI only (unverified Grok reading): walking mean 0.18 m/s, running mean 0.76 m/s, walking/running time shares 0.17/0.09 | m⁻²; m/s; fraction | As #31 | — | Ginelli 2015, main text; SI (Grok) | ⚠ARR | **Partly verified**: density, 74 % and 0.41 m/s confirmed in the main text by Library Verify; walking/running speeds and shares still Unverified (SI blocked) | https://doi.org/10.1073/pnas.1503749112 |
| 34 | Mosquitofish spacing and speed | NND mean 6.2 cm, median 3.8 cm, mode 2.6 cm; switch from repulsion to attraction at about 6 cm (about 2 BL); cruise speed 6.9 cm/s; polarisation 0.84 ± 0.26 (2 fish), 0.71 ± 0.26 (4), 0.63 ± 0.25 (8) (Grok's reading) | cm; cm/s; 0–1 | *G. holbrooki*, groups of 2, 4, 8 | ± SD | Herbert-Read 2011, Fig. S1A, Fig. 2D, SI (Grok) | ⚠ARR | **Unverified** (full text blocked; a search found no excerpt) | https://doi.org/10.1073/pnas.1109355108 |
| 35 | Sheep reaction to a dog | 46 sheep (GPS backpacks) and a herding dog (Australian Kelpie), 3 trials. Sheep start responding when the dog is about 70 m from the flock centroid. They move toward the centroid ("selfish herd"); cohesion (mean distance to the centroid) settles at about 4 m once packed side by side. This isn't a nearest-neighbour distance | m | Sheep, 3 trials | approx. | King et al. 2012 *Curr Biol* 22:R561, text and Fig. 1 | CC BY | Verified | https://discovery.ucl.ac.uk/id/eprint/1366736/ · https://doi.org/10.1016/j.cub.2012.05.008 |

## 5. Flight-initiation distance (FID)

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 36 | European birds: overall FID | Mean FID 11.46 m (SE 1.03), back-transformed; population means range 1–180 m. Based on 12,495 FIDs from 714 populations of 159 species | m | Breeding birds at 9 rural–urban site pairs, Spain to Finland (37–66°N), 2009–2010 | SE; range | Díaz et al. 2013, Results | CC BY 4.0 | Verified | https://doi.org/10.1371/journal.pone.0064634 |
| 37 | Rural vs urban FID | Rural 13.34 m (SE 1.05) vs urban 7.12 m (SE 1.05); paired t = −13.99, df = 229. Effect size 27 % after body mass and phylogeny | m | As #36 | SE | Díaz 2013, Results and Table 1 | CC BY 4.0 | Verified | https://doi.org/10.1371/journal.pone.0064634 |
| 38 | What sets FID | Body mass is the strongest predictor (effect r = 0.603, 95 % CI 0.554–0.648); then habitat (r = 0.524); then latitude (r = 0.219), with FID shorter further north. Model adjusted R² = 0.33; phylogenetic λ = 0.472 | effect size r | As #36 | 95 % CI | Díaz 2013, Table 1 | CC BY 4.0 | Verified | https://doi.org/10.1371/journal.pone.0064634 |
| 39 | FID protocol | One observer walks at normal speed straight at the bird, counting 1 m steps; approach starts from at least 30 m; FID is the straight-line distance including height | m | Method | — | Díaz 2013, Methods | CC BY 4.0 | Verified | https://doi.org/10.1371/journal.pone.0064634 |
| 40 | Australian large birds: FID (walker) | Emu 58.7 m (SD 36.2, n 6); Black Swan 50.4 (35.8, 19); Australian Pelican 32.6 (25.4, 39); Great Cormorant 32.3 (20.6, 34); Eastern Great Egret 39.9 (24.8, 79); Cattle Egret 63.1 (46.8, 11); Australian White Ibis 32.8 (20.4, 48); Pacific Black Duck 38.9 (29.0, 50) | m | Australian birds, approached mostly by a single walker | SD, n | Weston et al. 2012, Appendix 1 (author PDF); compiled from source studies listed per row | ⚠ARR (CSIRO) | Verified (review compilation of primary studies) | https://blumsteinlab.eeb.ucla.edu/wp-content/uploads/sites/104/2017/05/Weston_etal_2012_Emu.pdf · https://doi.org/10.1071/MU12026 |
| 41 | Australian shorebirds: FID, with study-to-study spread | Eastern Curlew 65.5 m (SD 41.6, n 42) in one study vs 126.1 (29.2, 22) in another; Masked Lapwing 46.8 (30.5, 37) vs 62.6 (43.1, 55); Hooded Plover 54.4 (35.4, 30), 41.1 (17.1, 8), 26.3 (3.3, 4) | m | As #40 | SD, n | Weston 2012, Appendix 1 | ⚠ARR | Verified (review compilation) | https://blumsteinlab.eeb.ucla.edu/wp-content/uploads/sites/104/2017/05/Weston_etal_2012_Emu.pdf |
| 42 | Habituation and maximum | Black Swan at an urban lake: 3.6 m (SD 3.8, n 92) vs 50.4 m in the general sample. The longest Australian FID on record is 196 m (Eastern Curlew; Glover et al. 2011) | m | As #40 | SD, n | Weston 2012, Appendix 1 and text | ⚠ARR | Verified (review compilation) | https://blumsteinlab.eeb.ucla.edu/wp-content/uploads/sites/104/2017/05/Weston_etal_2012_Emu.pdf |
| 43 | Black-tailed deer FID | FID = −7.68 + 0.86·SD − 1.96×10⁻³·SD², where SD is the starting distance (m); FID peaks at 87 m when SD = 219 m; maximum alert distance seen 169 m (Grok's reading) | m | *Odocoileus hemionus columbianus*, 78 approaches | — | Stankowich & Coss 2006 *Behav Ecol* 17:246 (Grok) | ⚠ARR | **Unverified** (not read by me) | https://doi.org/10.1093/beheco/arj020 |

## 6. Daily rhythm

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 44 | Mammal activity patterns | 69 % nocturnal, 20 % diurnal, 8.5 % cathemeral, 2.5 % crepuscular | % of species | 4,477 mammal species analysed (the excerpt says 612 non-fossorial species were left out; the reason is cut off in the excerpt) | — | Bennie et al. 2014, Results | ⚠ARR (PNAS; author copy online) | Verified (search/publisher excerpt) | https://doi.org/10.1073/pnas.1216063110 |
| 45 | Where each pattern dominates | Diurnal species make up most species at high altitude (Tibetan Plateau, Andes); cathemeral richness is highest at mid–high northern latitudes; crepuscular species cluster in southern Africa, India and China; nocturnal species are the largest share in arid regions. Activity pattern is strongly conserved across the family tree (Pagel's λ = 0.947) | qualitative; λ | As #44 | — | Bennie 2014, Results | ⚠ARR | Verified (search/publisher excerpt) | https://doi.org/10.1073/pnas.1216063110 |

## 7. Extra rows: rules of thumb an engine can use (each is traceable to a row above)

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 46 | Starling neighbour count to use | Interact with the nearest 6–7 birds, not everything within a radius | birds | From #12 | ±0.9 SE | Ballerini 2008 | ⚠ARR | Verified (preprint) | https://arxiv.org/abs/0709.1916 |
| 47 | Starling spacing | NND about 0.7–1.5 m; 0.2 m hard-core minimum | m | From #13, #8 | range | Ballerini 2008; Hemelrijk 2011 | ⚠ARR / CC BY | Verified | https://arxiv.org/abs/0709.1916 |
| 48 | Starling flock speed | About 7–19 m/s, typically 10–12 m/s | m/s | From #14, #18 | range | Ballerini 2008; Cavagna 2010 | ⚠ARR | Verified (preprint) | https://arxiv.org/abs/0911.4393 |
| 49 | Flock shape for a starling murmuration | Flat: thickness : width : length ≈ 1 : 2.8 : 5.6, with the thin axis vertical | ratio | From #15 | 95 % CI | Ballerini 2008 *Anim Behav* | ⚠ARR | Verified (preprint) | https://arxiv.org/abs/0802.1667 |
| 50 | Alignment level for flocks | Polarisation about 0.96 for starlings; at most 0.09 for midge swarms | 0–1 | From #17, #29 | SD | Cavagna 2010; Kelley 2013 | ⚠ARR / ⚠NC ⚠ND | Verified | https://arxiv.org/abs/0911.4393 |
| 51 | School state shares | Small schools (30) are polarised 57 % of the time; large ones (300) mill 45 % of the time | fraction | From #25 | — | Tunstrøm 2013 | CC BY | Verified | https://doi.org/10.1371/journal.pcbi.1002915 |
| 52 | Fish school NND in body lengths (Partridge) | Herring keep proportionately larger spacing than saithe or cod; NND falls with school size and speed. No species table was read | BL | *Clupea harengus*, *Pollachius virens*, *Gadus morhua* | — | Partridge et al. 1980 *Behav Ecol Sociobiol* 6:277 (Grok's reading of the abstract) | ⚠ARR | **Unverified** (abstract not read by me) | https://doi.org/10.1007/BF00292770 |
| 53 | Sheep herd travel | Grazing drift about 0.1–0.15 m/s; packing runs up to about 1.5 m/s | m/s | From #11, #32 | approx. | Ginelli 2015 | ⚠ARR | Verified (search/publisher excerpt) | https://doi.org/10.1073/pnas.1503749112 |
| 54 | Sheep alert distance to a dog | About 70 m | m | From #35 | approx. | King 2012 | CC BY | Verified | https://doi.org/10.1016/j.cub.2012.05.008 |
| 55 | Bird FID typical scale | About 11 m average in Europe; about 30–65 m for large waterbirds and shorebirds in Australia; urban birds about half the rural distance | m | From #36–#41 | SE/SD | Díaz 2013; Weston 2012 | CC BY / ⚠ARR | Verified | https://doi.org/10.1371/journal.pone.0064634 |
| 56 | Ungulate FID by species (meta-analysis means) | Stankowich 2008 reports effect sizes (r), not mean FIDs in metres, e.g. deer approached fast vs slow r = 0.21, direct vs tangential r = 0.31 (Grok's reading) | r | Ungulates | — | Stankowich 2008 *Biol Conserv* 141:2159, Table 1 (Grok) | ⚠ARR | **Unverified** (paywalled; not read) | https://doi.org/10.1016/j.biocon.2008.06.026 |
| 57 | Reaction delay to neighbours | Pigeons follow leaders with about 0.3–0.4 s delay; StarDisplay uses 50 ms reaction time; Couzin uses a 0.1 s step ("fish response latency" per Grok; I didn't read that sentence) | s | From #20, #7, #4 | SD | Nagy 2010; Hemelrijk 2011; Couzin 2002 | mixed | Verified (values); Couzin rationale unverified | https://arxiv.org/abs/1010.5394 |
| 58 | Surf scoter spacing for a water-bird raft | About 1.5 BL between neighbours; swim at about 2 BL/s | BL | From #21 | SD 0.25 BL | Lukeman 2010 | ⚠ARR | Verified (search/publisher excerpt) | https://doi.org/10.1073/pnas.1001763107 |
| 59 | Midge swarm size scaling | Size ∝ N^(1/3), about 20–30 cm across | cm | From #30 | — | Kelley 2013 | ⚠NC ⚠ND | Verified | https://doi.org/10.1038/srep01073 |
| 60 | Vicsek critical noise | About 2.9 rad at ρ = 0.4. Below that, order emerges | rad | From #6 | ±0.05 | Vicsek 1995 | ⚠ARR | Verified (preprint) | https://arxiv.org/abs/cond-mat/0611743 |
| 61 | Couzin state recipe | Swarm: Δr_o ≈ 0. Torus/mill: small Δr_o with large Δr_a. Parallel: larger Δr_o (Fig. 3 regions, with the defaults in #5). Exact region boundaries come from the figure, not from text | units | From #5 | — | Couzin 2002, Fig. 3 | ⚠ARR | Verified (search/publisher excerpt; boundaries qualitative) | https://doi.org/10.1006/jtbi.2002.3065 |
| 62 | Activity pattern default for a mammal | Assume nocturnal unless known (69 %); diurnal 20 % | % | From #44 | — | Bennie 2014 | ⚠ARR | Verified (search/publisher excerpt) | https://doi.org/10.1073/pnas.1216063110 |
| 63 | Flock size range seen in field | Starlings 122–4268 birds (24 flocks); pigeons up to 10; sheep 46–100 | count | From #17, #20, #31, #35 | — | as cited | mixed | Verified | https://arxiv.org/abs/0911.4393 |
| 64 | Fish school density effect | Shrinking the tank to one-tenth the area (30 fish in 0.66 × 0.38 m, matching the density of 300 fish) did **not** increase milling, so milling depends on group size, not density | qualitative | *N. crysoleucas* | 4 replicates | Tunstrøm 2013, Results | CC BY | Verified | https://doi.org/10.1371/journal.pcbi.1002915 |

**Note on section 7:** rows 46–64 restate values from sections 1–6 in engine terms, plus a few small extras (rows 52, 56, 57, 63, 64). They aren't independent measurements. A strict count of independent measured or model rows is 45 (#1–#45: 42 Verified, 3 Unverified).

## Not found

- **Deer, wildebeest and bison:** no herd nearest-neighbour distance or group travel speed with n and spread, either by me or by Grok.
- **Species-mean FID in metres for ungulates:** Stankowich 2008 gives effect sizes only (Grok); the paper is paywalled.
- **Partridge 1980:** the species table of fish NND in body lengths (paywalled).
- **Zone radii for the surf scoter model:** the excerpt gives weights only; the radii are in the figure.
- **Couzin 2002 Fig. 3 boundaries:** not available as numbers.
- **Herbert-Read 2011 spacing and speed values:** full text and SI were blocked (row 34 is Unverified).
- **Ginelli 2015 SI gait statistics:** blocked (row 33 is Unverified).
- **Daily activity budgets in hours for named herd species** (e.g. hours grazing per day): not searched in depth because of the usage limit. Bennie 2014 gives only category shares.
- **Reynolds 1987 numeric radius or weights:** the paper gives none. Both Grok and I confirm this.

## Sources disagree

1. **Same starling events, different numbers.**
   - Ballerini 2008 *Anim Behav* Table 1 and Cavagna 2010 Table S1 share event labels, but their counts and speeds differ.
   - For example, 16-05 has 2631 birds at 15.2 m/s in one and 2941 birds at 15.2 m/s in the other; 32-06 has 781 at 9.6 vs 809 at 9.8; 31-01 has 1856 at 6.9 vs 2126 at 6.8.
   - The reconstructions and averaging windows are probably different. Use one table consistently.
2. **Tunstrøm 2013 local-order radius:** the Results say 15.5 cm and the Methods say 15.6 cm (about 3 BL). Grok quoted 15.6.
3. **Starling speed:** the 10-event mean is 10.6 m/s (Ballerini) and the 24-flock mean is 11.6 m/s (Cavagna); both are computed by me. StarDisplay's cruise speed is 10 m/s.
4. **Sheep walking speed:** the model's v₁ is 0.15 m/s (fixed). The text gives about 0.1 m/s as an order of magnitude. Grok's SI reading gives a walking mean of 0.18 m/s (Unverified).
5. **Same species, different FID:** Eastern Curlew is 65.5 m in one study vs 126.1 m in another; Black Swan is 50.4 m in the general sample vs 3.6 m at an urban lake. Habituation and site matter more than the species average (Weston 2012).

## What Grok added or contradicted

The pass ran 10:46–11:09 AEDT; the output is in `/workspace/c9work/grok_c9.txt`.

- **Added, then confirmed by me:**
  - Couzin 2002 Table 1 and the Fig. 3 defaults (rows 4–5), confirmed in a publisher excerpt.
  - The Weston 2012 Appendix 1 FIDs (rows 40–42), confirmed in the author PDF. Grok gave one row per species; I added the other study rows for Masked Lapwing and Hooded Plover.
  - King 2012: 70 m and about 4 m (row 35), confirmed in the UCL Discovery PDF, which is CC BY.
  - Cavagna Table S1 speeds 6.8–19.1 m/s (row 18), confirmed in the arXiv SI.
  - The Díaz rural/urban values (row 37), confirmed in PMC.
- **Matched what I had already read:** the Ballerini r₁, n_c and Table 1 values; Tunstrøm's thresholds and time fractions; the Reynolds points (no numeric radius or weights; 80 boids, 95 s per frame; 300° field of view).
- **Added but not confirmed (rows 33, 34, 43, 52, 56 are Unverified):**
  - the Ginelli SI gait statistics;
  - the Herbert-Read spacing, speed and polarisation values;
  - the Stankowich & Coss 2006 deer FID curve;
  - the Partridge 1980 qualitative points;
  - the Stankowich 2008 effect sizes.
  - Grok also wrote "analysis after ≤5000 steps (8.3 min)" for Couzin; I didn't see it, so I didn't use it.
  - Grok gave King 2012 as CC BY **3.0**. The UCL record says CC BY without a version.
- **Contradicted:** nothing in my rows. The one numeric mismatch, 15.5 vs 15.6 cm, is the paper's own inconsistency.
- **Gaps Grok also couldn't fill:** NND and speed for deer, wildebeest and bison; species-mean ungulate FID; the Partridge NND table.

## Sources (all accessed 2026-11-17)

| Source | Licence | URL |
|---|---|---|
| Reynolds CW 1987. Flocks, herds and schools: a distributed behavioral model. *Comput Graph* 21(4):25–34 (author's OCR reprint) | ⚠ARR (ACM) | https://doi.org/10.1145/37402.37406 · https://www.red3d.com/cwr/papers/1987/SIGGRAPH87.pdf |
| Couzin ID, Krause J, James R, Ruxton GD, Franks NR 2002. Collective memory and spatial sorting in animal groups. *J Theor Biol* 218:1–11 | ⚠ARR (Elsevier) | https://doi.org/10.1006/jtbi.2002.3065 |
| Vicsek T et al. 1995. Novel type of phase transition in a system of self-driven particles. *Phys Rev Lett* 75:1226–1229 (arXiv cond-mat/0611743) | ⚠ARR (APS) | https://doi.org/10.1103/PhysRevLett.75.1226 · https://arxiv.org/abs/cond-mat/0611743 |
| Hemelrijk CK, Hildenbrandt H 2011. Some causes of the variable shape of flocks of birds. *PLoS ONE* 6:e22479 | CC BY | https://doi.org/10.1371/journal.pone.0022479 · https://pmc.ncbi.nlm.nih.gov/articles/PMC3150374/ |
| Ballerini M et al. 2008. Interaction ruling animal collective behavior depends on topological rather than metric distance. *PNAS* 105:1232–1237 (arXiv 0709.1916) | ⚠ARR (PNAS) | https://doi.org/10.1073/pnas.0711437105 · https://arxiv.org/abs/0709.1916 |
| Ballerini M et al. 2008. Empirical investigation of starling flocks: a benchmark study in collective animal behaviour. *Anim Behav* 76:201–215 (arXiv 0802.1667) | ⚠ARR (Elsevier) | https://doi.org/10.1016/j.anbehav.2008.02.004 · https://arxiv.org/abs/0802.1667 |
| Cavagna A et al. 2010. Scale-free correlations in starling flocks. *PNAS* 107:11865–11870 (arXiv 0911.4393) | ⚠ARR (PNAS) | https://doi.org/10.1073/pnas.1005766107 · https://arxiv.org/abs/0911.4393 |
| Nagy M, Ákos Z, Biro D, Vicsek T 2010. Hierarchical group dynamics in pigeon flocks. *Nature* 464:890–893 (arXiv 1010.5394) | ⚠ARR (Nature) | https://doi.org/10.1038/nature08891 · https://arxiv.org/abs/1010.5394 |
| Lukeman R, Li Y-X, Edelstein-Keshet L 2010. Inferring individual rules from collective behavior. *PNAS* 107:12576–12580 | ⚠ARR (PNAS) | https://doi.org/10.1073/pnas.1001763107 |
| Tunstrøm K et al. 2013. Collective states, multistability and transitional behavior in schooling fish. *PLoS Comput Biol* 9:e1002915 | CC BY | https://doi.org/10.1371/journal.pcbi.1002915 · https://pmc.ncbi.nlm.nih.gov/articles/PMC3585391/ |
| Katz Y et al. 2011. Inferring the structure and dynamics of interactions in schooling fish. *PNAS* 108:18720–18725 (abstract) | ⚠ARR (PNAS) | https://doi.org/10.1073/pnas.1107583108 |
| Herbert-Read JE et al. 2011. Inferring the rules of interaction of shoaling fish. *PNAS* 108:18726–18731 (abstract) | ⚠ARR (PNAS) | https://doi.org/10.1073/pnas.1109355108 |
| Kelley DH, Ouellette NT 2013. Emergent dynamics of laboratory insect swarms. *Sci Rep* 3:1073 | ⚠NC ⚠ND (CC BY-NC-ND 3.0) | https://doi.org/10.1038/srep01073 · https://pmc.ncbi.nlm.nih.gov/articles/PMC3545223/ |
| Ginelli F et al. 2015. Intermittent collective dynamics emerge from conflicting imperatives in sheep herds. *PNAS* 112:12729–12734 | ⚠ARR (PNAS) | https://doi.org/10.1073/pnas.1503749112 · https://pmc.ncbi.nlm.nih.gov/articles/PMC4611628/ |
| King AJ et al. 2012. Selfish-herd behaviour of sheep under threat. *Curr Biol* 22:R561–R562 | CC BY | https://doi.org/10.1016/j.cub.2012.05.008 · https://discovery.ucl.ac.uk/id/eprint/1366736/ |
| Díaz M et al. 2013. The geography of fear: a latitudinal gradient in anti-predator escape distances of birds across Europe. *PLoS ONE* 8:e64634 | CC BY 4.0 | https://doi.org/10.1371/journal.pone.0064634 · https://pmc.ncbi.nlm.nih.gov/articles/PMC3665823/ |
| Weston MA, McLeod EM, Blumstein DT, Guay P-J 2012. A review of flight-initiation distances and their application to managing disturbance to Australian birds. *Emu* 112:269–286 (author PDF) | ⚠ARR (CSIRO) | https://doi.org/10.1071/MU12026 · https://blumsteinlab.eeb.ucla.edu/wp-content/uploads/sites/104/2017/05/Weston_etal_2012_Emu.pdf |
| Bennie JJ, Duffy JP, Inger R, Gaston KJ 2014. Biogeography of time partitioning in mammals. *PNAS* 111:13727–13732 | ⚠ARR (PNAS) | https://doi.org/10.1073/pnas.1216063110 · https://pmc.ncbi.nlm.nih.gov/articles/PMC4183310/ |
| Stankowich T, Coss RG 2006. Effects of predator behavior and proximity on risk assessment by Columbian black-tailed deer. *Behav Ecol* 17:246–254 (Grok lead; not read) | ⚠ARR | https://doi.org/10.1093/beheco/arj020 |
| Stankowich T 2008. Ungulate flight responses to human disturbance: a review and meta-analysis. *Biol Conserv* 141:2159–2173 (Grok lead; not read) | ⚠ARR | https://doi.org/10.1016/j.biocon.2008.06.026 |
| Partridge BL, Pitcher T, Cullen JM, Wilson J 1980. The three-dimensional structure of fish schools. *Behav Ecol Sociobiol* 6:277–288 (Grok lead; not read) | ⚠ARR | https://doi.org/10.1007/BF00292770 |

*Link check, 2026-11-17 at about 11:18 AEDT:*
- 35 unique URLs were checked. All returned success: 21 DOI and arXiv redirects (302), and 14 pages returned 200 (PMC, arXiv, the red3d PDF, the UCLA author PDF and UCL Discovery).
- The PNAS and PMC full text for the PNAS papers is blocked to scripts, so rows 10, 11, 21, 31, 32, 44 and 45 rely on search-index excerpts of the publisher text.
- Scratch is in /workspace/c9work (not in any repo). Article PDFs and texts were saved there for reading only; no datasets were downloaded.
