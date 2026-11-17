# C8 — Coat patterns: reaction–diffusion, stripes/spots/rosettes, scale and feather layout

*Creature Data, item C8 of the Specula private library. Private research notes; not for publication.*

- **Web reading:** 2026-11-17, about 09:57–10:11 AEDT; Grok leads checked 10:39–10:42 AEDT. All URLs accessed 2026-11-17.
- **Grok pass:** grok-4.6 live search, reasoning effort xhigh. It ran **2026-11-17 10:16–10:39 AEDT**, as one focused pass on the gaps from the first draft (output: `/workspace/c8work/grok_c8.txt`).
  - An earlier attempt at 09:58 AEDT failed because the CLI was missing after the box restart. The Grok CLI was restored at about 10:13.
  - Grok output is treated as unverified leads. A Grok number is marked Verified only after I read it in the primary text myself (section 6).
- **Scratch:** `/workspace/c8work` (not in any repo).
- **Status key:**
  - **Verified:** I read the number in the open full text.
  - **Verified (abstract):** I read it in the abstract only.
  - **Verified (search excerpt):** I read it in a search-index excerpt of the publisher full text, because the page blocks scripts.
  - **Verified (as quoted):** the number is quoted by an open secondary paper; I have not read it in the primary source.
  - **Verified (model parameter):** a simulation setting, not a measurement of an animal.
  - **Unverified:** the reason is given.
- **Licence flags:**
  - ⚠NC: non-commercial terms.
  - ⚠ARR: all rights reserved or publisher copyright. Use the numbers and facts only; do not reuse text or figures.

**Row count: 76 data rows: 66 Verified (all sub-kinds), 10 Unverified (rows 10, 61, 62, 70–76).** Before the Grok pass the file had 62 rows (59/3). The pass added 14 rows (7 Verified, 7 Unverified), and row 17 is now fully Verified.

## 1. Reaction–diffusion: published parameter sets you can run

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 1 | Gray–Scott equations | ∂U/∂t = D_u∇²U − UV² + F(1−U); ∂V/∂t = D_v∇²V + UV² − (F+k)V | dimensionless | Model (Pearson 1993) | — | Pearson 1993, Eq. 2 (arXiv p. 2) | ⚠ARR (Science); arXiv preprint under arXiv's default non-exclusive licence | Verified (model parameter) | https://arxiv.org/abs/patt-sol/9304003 |
| 2 | Domain and boundaries | 2.5 × 2.5, periodic | dimensionless length | Pearson 1993 standard runs | — | arXiv p. 2–3 | as #1 | Verified (model parameter) | https://arxiv.org/abs/patt-sol/9304003 |
| 3 | Diffusion coefficients | D_u = 2×10⁻⁵, D_v = 1×10⁻⁵ (ratio 2) | dimensionless | Pearson 1993 | — | arXiv p. 3 | as #1 | Verified (model parameter) | https://arxiv.org/abs/patt-sol/9304003 |
| 4 | Grid and integrator | 256 × 256 points, forward Euler, Δt = 1 | — | Spot checks up to 1024² and Δt = 0.01 gave "no qualitative difference" | — | arXiv p. 3 | as #1 | Verified (model parameter) | https://arxiv.org/abs/patt-sol/9304003 |
| 5 | Initial condition | U = 1, V = 0 everywhere; central 20 × 20 points set to U = 1/2, V = 1/4; ±1 % random noise; run 200,000 steps | — | Pearson 1993 | — | arXiv p. 3 | as #1 | Verified (model parameter) | https://arxiv.org/abs/patt-sol/9304003 |
| 6 | Time for the disturbance to fill the grid | ~10,000–20,000 steps; front speed ~1×10⁻⁴ space units per time unit | time steps; space/time | Depends on F, k | range as given | arXiv p. 3–4 | as #1 | Verified (model parameter) | https://arxiv.org/abs/patt-sol/9304003 |
| 7 | Self-replicating chaotic spots (pattern ε) | F = 0.02, k = 0.059; Lyapunov exponent ~1.5×10⁻³; Lyapunov time 660 steps ≈ the time for one spot to divide | dimensionless; steps | 20 perturbed copies, averaged | — | arXiv p. 5 | as #1 | Verified (model parameter) | https://arxiv.org/abs/patt-sol/9304003 |
| 8 | Hopf limit | Bifurcating periodic orbit is stable for k < 0.035 and unstable for k > 0.035 | dimensionless | Reaction kinetics only (no diffusion) | — | arXiv p. 3; Fig. 1 caption | as #1 | Verified (model parameter) | https://arxiv.org/abs/patt-sol/9304003 |
| 9 | Where true Turing patterns occur | Only near F = k = 0.0625 at ratio 2; the "smallest ratio of diffusion coefficients that gives stable Turing patterns is about D_u/D_v = 2.8"; no patterns with equal D | dimensionless | Pearson's text as written (see Sources disagree) | — | arXiv p. 6 | as #1 | Verified (model parameter) | https://arxiv.org/abs/patt-sol/9304003 |
| 10 | Extent of the F–k pattern map | Tick labels 0.00–0.08 and 0.03–0.07 appear in the figure text layer; I inferred F is vertical (0–0.08) and k is horizontal (0.03–0.07) | dimensionless | Pearson Fig. 3 | — | arXiv figure pages | as #1 | **Unverified** (axis assignment inferred; figure not viewed) | https://arxiv.org/abs/patt-sol/9304003 |
| 11 | Kondo–Miura saturating linear activator–inhibitor model | ∂u/∂t = F(u,v) − d_u·u + D_u∆u; ∂v/∂t = G(u,v) − d_v·v + D_v∆v; F = a_u·u + b_u·v + c_u clipped to [0, F_max]; G = a_v·u + b_v·v + c_v clipped to [0, G_max] | dimensionless | Used for shark denticles | — | Cooper et al. 2018, Methods Eqs. 1–4 | CC BY 4.0 | Verified (model parameter) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6221541/ |
| 12 | Denticle parameter set (Figs. 1K–M, 4S–U) | d_u = 0.03, D_u = 0.02, a_u = 0.08, b_u = −0.08, c_u = 0.04, F_max = 0.2; d_v = 0.08, D_v = 0.6, a_v = 0.16, b_v = 0, c_v = −0.05, G_max = 0.5 (so D_v/D_u = 30, derived) | dimensionless | Cooper 2018 | — | Methods, "RD modeling" | CC BY 4.0 | Verified (model parameter) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6221541/ |
| 13 | Denticle domain and numerics | Square domain L = 75, end time T = 1500, no-flux boundaries; Δx = L/128 ≈ 0.58; Δt = Δx²/(8D_v) ≈ 0.07 (≈ 0.04 when D_v was changed) | dimensionless | Explicit finite differences | — | Methods | CC BY 4.0 | Verified (model parameter) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6221541/ |
| 14 | Initiator-row seed | 6 spots, R_spot = 4.5, centred at (iL/5, L/2); u₀ = 5 inside spots, else 0; v₀ = 0 | dimensionless | Represents the row of dorsal denticles | — | Methods Eqs. 5–6 | CC BY 4.0 | Verified (model parameter) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6221541/ |
| 15 | Sparser, skate-like spacing | Lower d_v and G_max and raise D_v (values in table S1, not read) | direction only | Cooper Fig. 5H | — | Results; Fig. 5 legend | CC BY 4.0 | Verified (direction); exact S1 values not read | https://pmc.ncbi.nlm.nih.gov/articles/PMC6221541/ |
| 16 | Hybrid patterns in the model | Between parameter regions giving spots and inverse spots there is always a region that gives labyrinths, so a cross gives a maze | qualitative | Several RD models | — | Miyazawa 2010, Results and Fig. 3 | ⚠NC (CC BY-NC-SA 3.0) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC2982180/ |
| 17 | Felid two-stage model | Make small flecks first, then change the parameters to turn them into leopard rosettes or jaguar rosettes with a central spot (parameter values in rows 63–66) | qualitative | Liu, Liaw & Maini 2006 | — | Abstract; full text (Oxford repository copy) | ⚠ARR (APS) | Verified | https://doi.org/10.1103/physreve.74.011914 |

## 2. Scales: lizard scale-by-scale colouring, shark denticles, reptile placodes

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 18 | Diffusion cut at scale borders needed in a 2D model to get one colour per scale | Reduce all diffusion coefficients by > 80 % at scale borders | % | Ocellated lizard (*Timon lepidus*) model | — | Fofonjka & Milinkovitch 2021, Introduction | CC BY 4.0 | Verified (model parameter) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| 19 | Same result from 3D skin geometry alone | Thick scales with thin skin between them give scale-by-scale colour and cellular-automaton switching with no border factor; works with Gray–Scott, Schnakenberg and a 3-component model | qualitative | Hexagonal prisms and HREM-reconstructed real scales | — | Results, Fig. 2 | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| 20 | Gray–Scott colour states per scale (3D runs) | Green (u, v) = (0.5, 0.25); black (u, v) = (0, 1); ±1 % noise per node | dimensionless | Fig. 2G, I | — | Methods | CC BY 4.0 | Verified (model parameter) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| 21 | 2D shortcut vs full 3D | 2D is 10–20× faster (typically 2 min vs 30 min). Wrong-colour scales: 6.4 % (Fick–Jacobs), 1.5 % (Bradley). Mean colour difference per element: 7.0 % and 2.2 % | ×; min; % | Large skin-patch simulations | — | Results | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| 22 | Effect of body growth on the pattern | Relative length scale falls 27 % and mean border curvature rises 40 % (run seeded with the real 18-week pattern). Across 20 random-start runs: ≈ 25 % and 34 % | % | Quasi-static growth simulations | 20 runs (spread not given in text) | Results, Fig. 4 | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| 23 | Share of black scales at 18 weeks | ≈ 30 % | % of scales | Ocellated lizard, 18 weeks old | approximate | Results, Fig. 5 legend | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| 24 | Width of pattern motifs | First 2–3 scales wide, then 1–2 scales wide | scales | Ocellated lizard, juvenile → adult | — | Fig. 1 description | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| 25 | Scale size | Dorsal skin patch about 1 × 0.5 cm = 8 × 4 scales (≈ 1.25 mm per scale, my derivation) | cm; scales | One young adult ocellated lizard | n = 1 | Methods (episcopic microscopy) | CC BY 4.0 | Verified (derived spacing is mine) | https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| 26 | Ages followed in the real animal | 18, 28, 30, 32, 41, 45, 71 and 106 weeks; traces of the juvenile ocelli are still visible at 106 weeks and gone later | weeks | One tracked lizard | — | Fig. 4 legend | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| 27 | Shark denticle timeline | Stage 31 (~70 dpf): no placodes. Stage 32 (~80 dpf): two dorsolateral rows of dorsal-denticle placodes. Later stage 32 (~100 dpf): body-denticle rows appear next to and parallel with them | days post-fertilisation | Small-spotted catshark (*Scyliorhinus canicula*) embryos | approximate days | Cooper 2018, Results | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6221541/ |
| 28 | Blocking FGF leaves a gap in the row | Gap kept in 6/8 rows at 50 days after treatment (0/7 controls) and 7/8 at 75 days; smaller body denticles fill the gap | counts | Bead implants at ~75 dpf | n as given | Results, Fig. 4 | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6221541/ |
| 29 | Reptile scales start as placodes laid out in tracts | Crocodile, lizard and snake scales start from an anatomical placode (except crocodilian face and jaw); several tracts develop in space and time much like feather tracts | qualitative | Nile crocodile, bearded dragon, corn snake embryos | — | Di-Poï & Milinkovitch 2016, Results | CC BY 4.0 (per PMC XML) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC5392058/ |

## 3. Feather layout

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 30 | Number of feather tracts | 14 | tracts | Chicken | — | Mou et al. 2011, Introduction | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3057954/ |
| 31 | Start of tract layout (macropattern) | Embryonic day 7 (E7) | day | Chicken | — | Mou 2011, Introduction | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3057954/ |
| 32 | Two levels of layout | Macropattern = tracts with bare skin (apteria) between them; micropattern = regular spacing of feathers within a tract | definition | Birds | — | Mou 2011, Introduction | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3057954/ |
| 33 | Start of feather-row patterning | E6.5: first row of each tract at fixed anatomical sites, then new rows added in parallel as a wave sweeps outward | day | Chicken embryo | — | Ho et al. 2019, Introduction and Results | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6383868/ |
| 34 | Row timing in culture | Midline row of high cell density first; two broad lateral high-density bands at 10–25 h; first lateral rows from ~25–30 h; over the next ~70 h the midline stripe breaks into spots and paired rows are added on both sides | hours | E6.5 dorsal skin explants | — | Ho 2019, Results (Fig. 4) | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6383868/ |
| 35 | Cell density at which patterning starts | Normal 66 vs EDA-stimulated 49 mesenchymal cells per 100 µm (24 h). With EDA blocked, patterning only where density reached 98 cells per 100 µm (48 h). Which number is "normal" is read from the text's "lower than normal" | cells / 100 µm | Edge of the patterned region, chicken explants | not given in text | Ho 2019, Results (Fig. 5C, D) | CC BY 4.0 | Verified (control/EDA assignment from wording) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6383868/ |
| 36 | Width of the patterned skin with EDA added everywhere | "Almost doubled" | ratio ≈ 2 | 24 h explants | — | Ho 2019, Results (Fig. 3D, E) | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6383868/ |
| 37 | Offset between rows | Each new row's buds sit out of line with the previous row's (staggered, near-hexagonal packing) | qualitative | Chicken | — | Ho 2019, Discussion | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6383868/ |

## 4. Fish stripes (zebrafish, angelfish) and pattern mixing in hybrids

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 38 | Adult zebrafish stripe width | ≈ 600 µm | µm | Adult *Danio rerio* | approximate | Volkening & Sandstede 2018, quoting their ref. 38 | CC BY 4.0 | Verified (as quoted) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6089994/ |
| 39 | Reach of the long-range cell interaction | At least half a stripe width (so ≳ 300 µm, derived) | fraction of stripe | Melanophore–xanthophore interaction | — | Volkening & Sandstede 2018, quoting Nakamasu 2009 | CC BY 4.0 | Verified (as quoted) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6089994/ |
| 40 | Melanophores lost when the yellow stripes on both sides are cleared | 10 % to ≈ 15 % of melanophores died; melanophore density ≈ 20 % below control at day 6 after ablation; ≈ 10 % increase on the untreated side | % | Adult zebrafish, laser ablation | approximate | Nakamasu et al. 2009, Results (Fig. 2) | ⚠ARR (PNAS) | Verified (search excerpt) | https://doi.org/10.1073/pnas.0808622106 |
| 41 | Stripe timeline in the model (stage, size, age) | PB 7.5 mm SSL, 21 dpf; PR 8.5 mm, 30 dpf; SP 9.5 mm, 39 dpf; SA 10.1 mm, 44 dpf; J 10.9 mm, 51 dpf (stripes 2D/2V faint); J+ 13.0 mm, 70 dpf (2D/2V forming) | mm SSL; dpf | Zebrafish; milestones approximated from published images | — | Volkening & Sandstede 2018, Table 1 | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6089994/ |
| 42 | Skin growth in the model | 38 µm/day lengthwise, 27 µm/day in height | µm/day | Model domain (full body height, about a third of body length) | — | Methods | CC BY 4.0 | Verified (model parameter) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6089994/ |
| 43 | Melanophore death rule | Probability 0.0333 per day under the death condition | per day | Model, taken from a cited reference | — | Methods | CC BY 4.0 | Verified (model parameter) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6089994/ |
| 44 | Simulated span | 21 dpf to ≈ 75 dpf; plots average 100 runs with SD error bars | dpf; runs | Model | SD (in figures) | Results | CC BY 4.0 | Verified (model parameter) | https://pmc.ncbi.nlm.nih.gov/articles/PMC6089994/ |
| 45 | Stage sizes, Frohnhöfer staging | AR 6.2, PB 7.2, PR 8.6, SP 9.6, J 11.0, J+ 13, J++ 16 | mm SSL | Metamorphic zebrafish (after Parichy et al. 2009) | — | Frohnhöfer et al. 2013, Methods | CC BY 3.0 per PMC XML; Crossref says CC BY-NC-SA 3.0 → treat as ⚠NC | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3912879/ |
| 46 | Stripe order | First light interstripe X0 forms at the horizontal myoseptum; dark stripes 1D and 1V form above and below it; more stripes and interstripes are added dorsally and ventrally as the fish grows | qualitative | Zebrafish | — | Frohnhöfer 2013, Introduction and Discussion | as #45 ⚠NC | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC3912879/ |
| 47 | Angelfish stripes keep their spacing as the fish grows | Stripes rearrange so the gaps between them stay the same, unlike mammal patterns that just stretch; an RD simulation predicts the rearrangement | qualitative | Marine angelfish *Pomacanthus* | — | Kondo & Asai 1995, abstract | ⚠ARR (Nature) | Verified (abstract) | https://doi.org/10.1038/376765a0 |
| 48 | Hybrid salmonid patterns | *Salvelinus* × *Salvelinus* (2 crosses, 30 fish each): light spots. *Oncorhynchus*/*Salmo* crosses (4 crosses, 11–30 fish each): dark spots. *Salvelinus* × *Oncorhynchus*/*Salmo* (8 crosses, 12–30 fish each): all labyrinthine | crosses; fish | F1 hybrids from an earlier breeding study | n as given | Miyazawa et al. 2010, Results, citing earlier experiments | ⚠NC (CC BY-NC-SA 3.0) | Verified (as quoted) | https://pmc.ncbi.nlm.nih.gov/articles/PMC2982180/ |

## 5. Mammal coats: felids, giraffe, zebra, domestic cat

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 49 | Melanism vs habitat in cats | More common in species that prefer temperate forest (n = 35, λ = 0.58, r² = 0.35, β = 0.26, p < 0.0001) and in habitat generalists (n = 35, r² = 0.24, p = 0.003) | species; stats | Felid species, phylogenetically controlled | as given | Allen et al. 2011, Results | ⚠ARR (Royal Society) | Verified (search excerpt) | https://doi.org/10.1098/rspb.2010.1734 |
| 50 | Melanism vs flexible activity and locomotion | Both day- and night-active: r² = 0.25, β = −0.79. Both ground and tree use: r² = 0.19, β = −0.78, p < 0.008. Open + closed habitat: no effect (r² = 0.04, p = 0.23) | stats | n = 35 felid species | as given | Allen 2011, Results | ⚠ARR | Verified (search excerpt) | https://doi.org/10.1098/rspb.2010.1734 |
| 51 | Felid pattern vs ecology | Whether a cat is patterned, and how complex and irregular the pattern is, relates to habitat, tree-climbing and night activity; little phylogenetic signal | qualitative | Felid flank patterns matched to RD model outputs | — | Allen 2011, abstract | ⚠ARR | Verified (abstract) | https://doi.org/10.1098/rspb.2010.1734 |
| 52 | Cat pattern prepattern timing | Stage 13 (≈ mouse E11): uniform single-layer epidermis. ≈ 16 days later, stage 16 (≈ mouse E15): epidermis split into alternating thick and thin regions that prefigure the coat pattern, before hair follicles form | days; stage | Domestic cat fetuses | approximate | Kaelin, McGowan & Barsh 2021, Results | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8423757/ |
| 53 | Dkk4 domains vs hair spacing | Dkk4 expression domains in fetal cat skin are much broader and appear earlier than hair-follicle placode spacing | qualitative | Domestic cat | — | Kaelin 2021, Discussion | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC8423757/ |
| 54 | Giraffe spot count per standard area | Mean 18.9, SD 7.5, CV 0.40; repeatability 0.78 | spots per analysis rectangle | Masai giraffe, northern Tanzania; shoulder–rib rectangle | SD | Lee, Cavener & Bond 2018, Table 1 | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6173159/ |
| 55 | Giraffe spot size | Area 0.04 ± 0.01 GU²; perimeter 0.99 ± 0.25 GU; maximum caliper 0.29 ± 0.06 GU (1 GU = height of the analysis rectangle, back to the fold behind the foreleg) | GU, GU² | As #54 | SD | Table 1; Methods | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6173159/ |
| 56 | Giraffe spot shape | Circularity 0.51 ± 0.08; aspect ratio 1.69 ± 0.15; roundness 0.63 ± 0.04; solidity 0.84 ± 0.04; angle 87.96 ± 15.39° | dimensionless; degrees | As #54 | SD | Table 1 | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6173159/ |
| 57 | Which spot traits pass mother to calf | Circularity: PO slope 0.52 ± 0.16, h² 1.04, P = 0.0037. Solidity: 0.53 ± 0.17, h² 1.06, P = 0.0041. Size traits not significant (e.g. number 0.20 ± 0.23, P = 0.39) | slope ± SE; h² | 31 mother–calf pairs; Bonferroni α = 0.0045 | SE | Table 1 | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6173159/ |
| 58 | Spots and calf survival | Larger spots (fewer per area) and rounder or irregular spots (lower aspect ratio) went with higher first-season survival; for number and aspect ratio the beta CIs excluded zero; top models ΔAICc 0 and 0.44 | qualitative + AICc | 258 calves, 26 surveys Jan 2012–Feb 2016 | CI | Abstract; Table 3 | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6173159/ |
| 59 | Photo protocol for measuring spots | Right side, from ~150 m, perpendicular, 100–400 mm lens; spots cut by the rectangle edge and speckles < 0.00001 GU² excluded | m; GU² | Lee 2018 | — | Methods | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC6173159/ |
| 60 | How much of zebra stripe variation the environment explains | 30–63 % for 12 of 18 stripe traits. Torso stripe number 63 % (prediction r² 0.42, p = 0.04); hind-leg stripe definition 63 %; foreleg definition 60 %; hind-leg thickness 51 % | % variance | Plains zebra, 16 sites (one is a single quagga photo) plus 8 test sites | as given | Larison et al. 2015, Table 1 | CC BY 4.0 | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC4448797/ |
| 61 | Zebra skin temperature vs other grazers | 29.2 °C vs 32.5 °C | °C | Preliminary observations cited by Larison (their ref. 39) | — | Larison 2015, Discussion | CC BY 4.0 | **Unverified** (secondary citation; primary not read) | https://pmc.ncbi.nlm.nih.gov/articles/PMC4448797/ |
| 62 | Body size and tail taper decide spots vs stripes vs plain (Murray's rule) | Very small and very large bodies give no pattern; mid sizes give spots or stripes; a tapering tail gives spots near the base and stripes towards the tip | qualitative | Murray 1981 RD model | — | Murray 1981, J Theor Biol 88:161–199 (not read; widely cited) | ⚠ARR (Elsevier) | **Unverified** (not read) | https://doi.org/10.1016/0022-5193(81)90334-9 |

## 6. Added from the Grok pass (10:16–10:39 AEDT), checked against primary text where possible

| # | Quantity | Value | Units | Population / conditions | Spread | Source (location) | Licence | Status | URL |
|---|---|---|---|---|---|---|---|---|---|
| 63 | Liu–Liaw–Maini felid model | ∂u/∂t = Dδ∇²u + αu + v − r₂uv − αr₃uv²; ∂v/∂t = δ∇²v + γu + βv + r₂uv + αr₃uv², with γ = −α. D = activator/inhibitor diffusion ratio (must be < 1); δ sets the length scale; quadratic terms favour spots, cubic favour stripes | dimensionless | Model (Barrio-type RD system) | — | Liu, Liaw & Maini 2006, Eq. 1, p. 011914-1 | ⚠ARR (APS); publisher PDF in Oxford ORA repository | Verified (model parameter) | https://doi.org/10.1103/physreve.74.011914 · https://ora.ox.ac.uk/objects/uuid:ca05b88a-0de3-4b0e-9c82-bde6776ad097 |
| 64 | Stage 1, spots (both cats) | D = 0.45, δ = 6, α = 0.899, β = −0.91, r₂ = 2, r₃ = 3.5; 200 × 200 grid; zero-flux edges; u, v start random in [0, 1]; most-enhanced mode k = 0.27 | dimensionless | Model | — | p. 011914-2, Eq. 3, Fig. 3 | as #63 | Verified (model parameter) | https://ora.ox.ac.uk/objects/uuid:ca05b88a-0de3-4b0e-9c82-bde6776ad097 |
| 65 | Stage 2, leopard rosettes | From the spot pattern: raise r₂ 2 → 7, then lower δ 6 → 3.8, then lower D 0.45 → 0.15 to freeze the pattern | dimensionless | Model | — | Figs 4, 6 legends | as #63 | Verified (model parameter) | https://ora.ox.ac.uk/objects/uuid:ca05b88a-0de3-4b0e-9c82-bde6776ad097 |
| 66 | Stage 2, jaguar rosettes (with a central spot) | Raise r₂ 2 → 7 (held longer), lower δ 6 → 1.8, then D 0.45 → 0.15. Other (α, β) pairs that also work: (0.77, −0.91), (0.77, −0.99). Power-spectrum peak moves from 0.041 (spots) to 0.081 (rings) | dimensionless | Model | — | Figs 5, 7, 9–10; text | as #63 | Verified (model parameter) | https://ora.ox.ac.uk/objects/uuid:ca05b88a-0de3-4b0e-9c82-bde6776ad097 |
| 67 | Leopard pattern by age | Spots at 2 days, rings at 8 weeks, rosettes as adult (photo examples) | age | Leopard, photographs | — | Liu 2006, Fig. 1 caption | as #63 | Verified | https://ora.ox.ac.uk/objects/uuid:ca05b88a-0de3-4b0e-9c82-bde6776ad097 |
| 68 | Tiger flank stripe count | Mean flank stripes < 23 in mainland tigers vs > 24 in Sunda tigers (separates 75 % of each group) | stripes per flank (mean of left and right) | *Panthera tigris*, photos of 114 tigers (skins, camera traps, zoos) | 75 % cut-off | Wilting et al. 2015, Table 1 / Fig. 4C; Methods | ⚠NC (CC BY-NC 4.0) | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC4640610/ |
| 69 | Tiger stripe look | Mainland: stripes thicker and less dense, especially on the hindquarters. Sunda: thin and particularly dense on the hindquarters | qualitative | As #68 | — | Wilting 2015, Table 1 | ⚠NC | Verified | https://pmc.ncbi.nlm.nih.gov/articles/PMC4640610/ |
| 70 | Leopard and cheetah spot sizes | Leopard rosettes up to 50–65 mm across. Caucasian leopard compact spots ~2–3 cm. Cheetah back spots 1.0 × 2.0 or 1.5 × 2.0 cm, others ~1.5 × 1.5 cm, tiny ~0.5 cm (Grok's reading) | mm; cm | Soviet-range specimens; n not stated | — | Heptner & Sludskii 1992 (English translation), *Mammals of the Soviet Union* II(2), pp. 207, 229, 706 (Grok's page numbers) | Copyright (book scan) ⚠ARR | **Unverified** (Grok lead; book scan not read by me) | https://archive.org/details/mammalsofsov221992gept |
| 71 | Grévy's zebra stripes | Flank stripes 25–38 mm wide, spaced ~20 mm; 18–26 body stripes between the shoulder and hip chevrons; 9–16 neck stripes, broadest up to 76 mm; ~80 stripes on a ~32 mm, 5-week embryo (Grok's reading) | mm; count | *Equus grevyi*; review citing Willoughby 1974, Bard | — | Churcher 1993, *Mammalian Species* 453, p. 2 | ⚠ARR (ASM) | **Unverified** (I downloaded the archive.org PDF but it is an image scan with no text layer; not read) | https://doi.org/10.2307/3504222 |
| 72 | Zebra stripe width by body region | Face 1.21, legs 1.35, neck 3.17, side 3.74, rump 3.90 cm (averaged over three species; from Egri et al. 2012) (Grok's reading) | cm | Three zebra species | — | Caro et al. 2014, Fig. 3 | ⚠ARR (Nature; free PDF) | **Unverified** (the values are in the figure; not in the PDF text I extracted) | https://doi.org/10.1038/ncomms4535 |
| 73 | Embryonic zebra stripe spacing | ~0.4 mm in all three main species when stripes first form (3rd, 4th and 5th week for plains, mountain and Grévy's) (Grok's reading of the abstract) | mm | Zebra embryos | — | Bard 1977, *J Zool* 183:527–539 | ⚠ARR | **Unverified** (abstract not obtained by me) | https://doi.org/10.1111/j.1469-7998.1977.tb04204.x |
| 74 | Angelfish stripe count over time | *Pomacanthus semicirculatus*: 3 → 6 → 12 vertical stripes at ~2, 6, 12 months; in *P. arcuatus*, pigment cells form a uniform grey at 5–7 mm body length (the model's starting point) (Grok's reading) | stripes; months; mm | Juvenile angelfish | — | Painter, Maini & Othmer 1999, Fig. 1 | ⚠ARR (PNAS) | **Unverified** (abstract read; the numbers aren't in it, and the full text was blocked) | https://doi.org/10.1073/pnas.96.10.5549 |
| 75 | Chicken feather-bud spacing | Row-to-row centre spacing 270 µm (SD 38.5, n = 280), 307 µm (SD 53.7, n = 187), 291 µm (SD 43.5, n = 238); in vivo 282 ± 29.6 µm (n = 202) (Grok's reading) | µm | Chicken dorsal skin, stage ~29–31 (≈ E6.5–E7.5) | SD | Jung 1998, PhD thesis (UCL), Tables 5.2, 6.2 | Thesis; not peer-reviewed | **Unverified** (not peer-reviewed; not read by me) | https://discovery.ucl.ac.uk/id/eprint/10098638/ |
| 76 | Feather primordium size | Primordia 200–300 µm across (reconstituted skin, not intact) (Grok's reading of snippets) | µm | Chicken, dissociated-mesenchyme assay | — | Jiang, Jung, Widelitz & Chuong 1999, *Development* 126:4997–5009 | ⚠ARR | **Unverified** (snippet only) | https://doi.org/10.1242/dev.126.22.4997 |

**Unverified, no row:**
- Cooper table S1 values (row 15).
- Turing 1952 (foundation paper, not read, ⚠ARR).
- Kondo & Miura 2010 review (abstract only, ⚠ARR).

## Not found

- **Jaguar rosette diameter in mm:** not found. Grok also found none; *Mammalian Species* 340 is qualitative.
- **Tiger body-stripe spacing in cm:** not found. Only counts are available (row 68).
- **Pomacanthus stripe interval in mm vs body size:** not found (Kondo & Asai 1995 and Painter 1999 full texts not read).
- **Intact chicken dorsal feather-bud spacing in µm, in a peer-reviewed paper:** not found. The only values are from a PhD thesis (row 75).
- **Zebra stripe widths:** in this file only as Unverified secondary values (rows 71–72). No open primary measurement was read.
- **Allen 2011 RD parameter ranges per species:** full text blocked to scripts.
- **Snake or fish scale-row counts; butterfly eyespots:** not covered.

## Sources disagree

1. **Frohnhöfer 2013 licence:** the PMC XML says CC BY 3.0, but Crossref metadata says CC BY-NC-SA 3.0. Treat as ⚠NC until checked on the Development page.
2. **Zebrafish stage sizes:** Volkening & Sandstede 2018 Table 1 gives PB 7.5, PR 8.5, SP 9.5 mm SSL, while Frohnhöfer 2013 gives PB 7.2, PR 8.6, SP 9.6 mm (both cite Parichy et al. 2009). The differences are small (0.1–0.3 mm), but use one table consistently.
3. **Pearson 1993 "smallest ratio" sentence:** the text says the smallest ratio giving stable Turing patterns is "about D_u/D_v = 2.8". But U is the substrate and D_u > D_v here (ratio 2 in his runs), so the 2.8 is a threshold on D_u/D_v specific to this model. This is not a contradiction between sources, but it is easy to misread. Check against the Science version.
4. **Murray's size rule (row 62)** is widely repeated, but I have not read the primary source. Do not treat it as confirmed.

5. **Leopard rosette size:** Heptner & Sludskii (Grok's reading) give rosettes up to 50–65 mm, while Caucasian leopard "compact spots" are ~2–3 cm. These are different spot types and populations, and both are Unverified.
6. **Ho 2019 cell densities (row 35):** Grok reports "66 vs 49 cells/100 µm" without saying which is normal. My reading (normal 66, EDA-stimulated 49) rests on the text's "lower than normal".

## What Grok added or contradicted

The pass ran 10:16–10:39 AEDT; output is in `/workspace/c8work/grok_c8.txt`.

- **Added, and confirmed by me in the primary text:**
  - Liu, Liaw & Maini 2006: full parameter set and both stage-2 paths (rows 63–67). Read in the publisher PDF held in Oxford's ORA repository; every value matched Grok's.
  - Wilting et al. 2015: tiger flank-stripe counts (rows 68–69). Confirmed in PMC; licence is CC BY-NC 4.0 (⚠NC), as Grok said.
- **Added but not confirmed (rows 70–76, all Unverified):**
  - Heptner & Sludskii big-cat spot sizes;
  - Churcher 1993 Grévy's zebra stripes (the archive.org PDF is an image scan with no text);
  - Caro 2014 per-region stripe widths (figure values);
  - Bard 1977 embryonic spacing;
  - Painter 1999 angelfish stripe counts;
  - Jung 1998 thesis feather spacing;
  - Jiang 1999 primordium size.
- **Contradicted or corrected:** nothing in my first draft. Grok's Ho 2019 density values match row 35.
  - One note: Grok cites Krausman & Morales 2005 (*Mammalian Species* 771, the cheetah account) for qualitative cheetah spots only, with no mm. Not used.
- **Gaps Grok also could not fill:**
  - jaguar rosette mm;
  - tiger stripe spacing in cm;
  - Pomacanthus interval in mm;
  - peer-reviewed intact feather spacing.

## Sources (all accessed 2026-11-17)

| Source | Licence | URL |
|---|---|---|
| Pearson JE 1993. Complex patterns in a simple system. *Science* 261:189–192 (preprint arXiv patt-sol/9304003) | ⚠ARR (Science); arXiv default non-exclusive licence | https://doi.org/10.1126/science.261.5118.189 · https://arxiv.org/abs/patt-sol/9304003 |
| Cooper RL et al. 2018. An ancient Turing-like patterning mechanism regulates skin denticle development in sharks. *Sci Adv* 4:eaau5484 | CC BY 4.0 | https://doi.org/10.1126/sciadv.aau5484 · https://pmc.ncbi.nlm.nih.gov/articles/PMC6221541/ |
| Fofonjka A, Milinkovitch MC 2021. Reaction-diffusion in a growing 3D domain of skin scales generates a discrete cellular automaton. *Nat Commun* 12:2433 | CC BY 4.0 | https://doi.org/10.1038/s41467-021-22525-1 · https://pmc.ncbi.nlm.nih.gov/articles/PMC8065134/ |
| Di-Poï N, Milinkovitch MC 2016. The anatomical placode in reptile scale morphogenesis… *Sci Adv* 2:e1600708 | CC BY 4.0 (per PMC XML) | https://doi.org/10.1126/sciadv.1600708 · https://pmc.ncbi.nlm.nih.gov/articles/PMC5392058/ |
| Mou C et al. 2011. Cryptic patterning of avian skin… *PLoS Biol* 9:e1001028 | CC BY 4.0 | https://doi.org/10.1371/journal.pbio.1001028 · https://pmc.ncbi.nlm.nih.gov/articles/PMC3057954/ |
| Ho WKW et al. 2019. Feather arrays are patterned by interacting signalling and cell density waves. *PLoS Biol* 17:e3000132 | CC BY 4.0 | https://doi.org/10.1371/journal.pbio.3000132 · https://pmc.ncbi.nlm.nih.gov/articles/PMC6383868/ |
| Volkening A, Sandstede B 2018. Iridophores as a source of robustness in zebrafish stripes and variability in Danio patterns. *Nat Commun* 9:3231 | CC BY 4.0 | https://doi.org/10.1038/s41467-018-05629-z · https://pmc.ncbi.nlm.nih.gov/articles/PMC6089994/ |
| Nakamasu A et al. 2009. Interactions between zebrafish pigment cells responsible for the generation of Turing patterns. *PNAS* 106:8429–8434 | ⚠ARR (PNAS) | https://doi.org/10.1073/pnas.0808622106 |
| Frohnhöfer HG et al. 2013. Iridophores and their interactions with other chromatophores are required for stripe formation in zebrafish. *Development* 140:2997–3007 | CC BY 3.0 (PMC) / CC BY-NC-SA 3.0 (Crossref) ⚠NC | https://doi.org/10.1242/dev.096719 · https://pmc.ncbi.nlm.nih.gov/articles/PMC3912879/ |
| Kondo S, Asai R 1995. A reaction–diffusion wave on the skin of the marine angelfish *Pomacanthus*. *Nature* 376:765–768 | ⚠ARR | https://doi.org/10.1038/376765a0 |
| Miyazawa S, Okamoto M, Kondo S 2010. Blending of animal colour patterns by hybridization. *Nat Commun* 1:66 | ⚠NC (CC BY-NC-SA 3.0) | https://doi.org/10.1038/ncomms1071 · https://pmc.ncbi.nlm.nih.gov/articles/PMC2982180/ |
| Liu RT, Liaw SS, Maini PK 2006. Two-stage Turing model for generating pigment patterns on the leopard and the jaguar. *Phys Rev E* 74:011914 | ⚠ARR (APS) | https://doi.org/10.1103/physreve.74.011914 |
| Allen WL, Cuthill IC, Scott-Samuel NE, Baddeley R 2011. Why the leopard got its spots… *Proc R Soc B* 278:1373–1380 | ⚠ARR (Royal Society) | https://doi.org/10.1098/rspb.2010.1734 · https://pmc.ncbi.nlm.nih.gov/articles/PMC3061134/ |
| Kaelin CB, McGowan KA, Barsh GS 2021. Developmental genetics of color pattern establishment in cats. *Nat Commun* 12:5127 (plus Author Correction, doi 10.1038/s41467-021-25886-9, not read) | CC BY 4.0 | https://doi.org/10.1038/s41467-021-25348-2 · https://pmc.ncbi.nlm.nih.gov/articles/PMC8423757/ |
| Lee DE, Cavener DR, Bond ML 2018. Seeing spots: … coat pattern traits in a wild population of giraffes. *PeerJ* 6:e5690 | CC BY 4.0 | https://doi.org/10.7717/peerj.5690 · https://pmc.ncbi.nlm.nih.gov/articles/PMC6173159/ |
| Larison B et al. 2015. How the zebra got its stripes: a problem with too many solutions. *R Soc Open Sci* 2:140452 | CC BY 4.0 | https://doi.org/10.1098/rsos.140452 · https://pmc.ncbi.nlm.nih.gov/articles/PMC4448797/ |
| Murray JD 1981. A pre-pattern formation mechanism for animal coat markings. *J Theor Biol* 88:161–199 (not read) | ⚠ARR | https://doi.org/10.1016/0022-5193(81)90334-9 |
| Turing AM 1952. The chemical basis of morphogenesis. *Phil Trans R Soc B* 237:37–72 (not read) | ⚠ARR | https://doi.org/10.1098/rstb.1952.0012 |
| Kondo S, Miura T 2010. Reaction-diffusion model as a framework for understanding biological pattern formation. *Science* 329:1616–1620 (abstract only) | ⚠ARR | https://doi.org/10.1126/science.1179047 |
| Liu RT, Liaw SS, Maini PK 2006 (full text read in the Oxford ORA copy of the publisher PDF) | ⚠ARR (APS) | https://ora.ox.ac.uk/objects/uuid:ca05b88a-0de3-4b0e-9c82-bde6776ad097 |
| Wilting A et al. 2015. Planning tiger recovery: understanding intraspecific variation for effective conservation. *Sci Adv* 1:e1400175 | ⚠NC (CC BY-NC 4.0) | https://doi.org/10.1126/sciadv.1400175 · https://pmc.ncbi.nlm.nih.gov/articles/PMC4640610/ |
| Heptner VG, Sludskii AA 1992. *Mammals of the Soviet Union* II(2) (Grok lead, not read) | Copyright ⚠ARR | https://archive.org/details/mammalsofsov221992gept |
| Churcher CS 1993. *Equus grevyi*. *Mammalian Species* 453:1–9 (Grok lead; scan not read) | ⚠ARR (ASM) | https://doi.org/10.2307/3504222 |
| Caro T et al. 2014. The function of zebra stripes. *Nat Commun* 5:3535 (Grok lead; figure values) | ⚠ARR | https://doi.org/10.1038/ncomms4535 |
| Bard JBL 1977. A unity underlying the different zebra striping patterns. *J Zool* 183:527–539 (Grok lead) | ⚠ARR | https://doi.org/10.1111/j.1469-7998.1977.tb04204.x |
| Painter KJ, Maini PK, Othmer HG 1999. Stripe formation in juvenile *Pomacanthus*… *PNAS* 96:5549–5554 (abstract read) | ⚠ARR | https://doi.org/10.1073/pnas.96.10.5549 |
| Jung H-S 1998. PhD thesis, University College London (Grok lead; not peer-reviewed) | Thesis | https://discovery.ucl.ac.uk/id/eprint/10098638/ |
| Jiang T-X, Jung H-S, Widelitz RB, Chuong C-M 1999. Self-organization of periodic patterns by dissociated feather mesenchymal cells… *Development* 126:4997–5009 (Grok lead) | ⚠ARR | https://doi.org/10.1242/dev.126.22.4997 |

*Link check, 2026-11-17 about 10:10 AEDT:*
- 32 unique URLs were checked.
- All 19 DOIs resolve at doi.org (HTTP 302), including the Murray 1981 DOI with parentheses.
- All 12 PMC links return 200, and the arXiv abstract returns 200/206.
- Publisher pages (PNAS, Royal Society) block scripted requests. That is why rows 40 and 49–50 rely on search-index excerpts.
- **Re-check, 2026-11-17 10:42 AEDT,** covering the 10 URLs added after the Grok pass: the ORA record, PMC4640610, archive.org and UCL Discovery return 200, and 6 DOIs return 302.
- Scratch is in /workspace/c8work (not in any repo).
