# E4: Where plants grow: vegetation by biome, density, self-thinning, forest layers, treeline

Research item E4 of `LIBRARY_RESEARCH_BRIEF.md`. Compiled 2026-11-16 (Australia/Sydney). **Every URL was accessed on 2026-11-16.**

## Fixes after verify (2026-11-16)

Applied from `E4-vegetation-by-biome.md.verify.md`. Finished 2026-11-17 after the usage pause (the research file was still at mtime 2026-11-16 10:22:51 AEDT, so none of these had been applied).

1. **BUR13 Table 2, p. 472:** the Europe and North America columns were swapped. Europe is now BA M 42.2 [30.7–69.2], OG 38.5 [24.4–57.4]; stems M 444 [259–2,182], OG 341 [124–1,835]; QMD OG 39.3 [14.6–64.3]. North America is now BA M 29.1 [24–59], OG 44.4 [28.7–379]; stems M 488.5 [217.5–1,000], OG 392 [96.7–1,720]; QMD OG 32.8 [15.7–171.6]. Confirmed against the author PDF table (column order Global, Asia, Australia, Europe, North America, South America). Global, Asia, Australia and South America were already right.
2. **GLICK16 Table 4 URL:** the numbers were already right, but `…/tables/4` is Nature's Table 3 (validation). Table 4 is at https://www.nature.com/articles/sdata201669/tables/5 (CC BY 4.0). Both the §4a URL and the reference were changed.
3. **JJ00 *Larix sibirica* 770 m:** Khakoma is at 67.9°N, not 67.8°N (Appendix 1). 67.8°N is Beldunchana at 765 m. The 200–770 m elevation band is unchanged.
4. **PK14 growing-season threshold:** now written '≥ 0.9 °C' (the GMBA PDF text layer has “[0.9 °C”, i.e. ≥). Season ≥ 94 days and season mean ≥ 6.4 °C were already right.
5. **EIVE SM08:** the 25 taxon rows in §7b are marked **UNVERIFIABLE**. The independent recheck could not download `EIVE_Paper_1.0_SM_08.xlsx` (Zenodo file 403). The record metadata still says CC BY 4.0. The numbers are left as first compiled and were not recomputed.

## 1. Method

- **The grok CLI was not used.** It is still signed out, so I didn't run it. Instead I used WebSearch to find sources, then read each one directly:
  - Europe PMC full-text XML for open-access papers;
  - the Crossref API for abstracts and metadata;
  - `pdftotext` for PDFs (USDA Forest Service, author-hosted and repository copies);
  - Nature table pages;
  - the Zenodo and ESA Ecological Archives datasets, opened in `/tmp/e4` only.
  
  Nothing was saved into any repo, and nothing was posted or sent.
- **Link check (curl, 2026-11-16):** all cited URLs returned HTTP 200, except two. The Zenodo record page returned 403 to curl, but its API and file download worked. bioRxiv returned 429 (rate limit) on recheck; it downloaded fine earlier the same day. DOIs were confirmed in Crossref; the NV06 ESA DOI is not in Crossref's API, so its archive page is cited instead.
- **Status of each number:** every number below was read on the linked page, PDF or dataset unless the row is flagged:
  - **[ABSTRACT]**: from the publisher abstract only;
  - **[SECONDARY]**: from a paper quoting another one;
  - **[DERIVED]**: my own arithmetic or summary of a source's raw data, with the method stated;
  - **[PREPRINT]**: from a preprint that was not peer reviewed.
- **Units:** as published. "Mean ± SD/SE/CI" are as the source states. Where I converted units, the row says so.

## 2. Sources and licences

| Key | Source | What it gives | Licence (plainly) |
|---|---|---|---|
| PAN13 | Pan, Birdsey, Phillips & Jackson 2013, "The structure, distribution, and biomass of the world's forests", *Annu. Rev. Ecol. Evol. Syst.* 44:593–622, doi 10.1146/annurev-ecolsys-110512-135914 (USDA Forest Service Treesearch PDF) | Table 1: biome temperature, precipitation, canopy height, biomass carbon | © Annual Reviews, all rights reserved. Facts cited. |
| WHIT | Stefan & Levin, `plotbiomes` R package: Whittaker biome polygons digitised from Ricklefs 2008, *The Economy of Nature*, Fig. 5.5 (after Whittaker 1975) | Biome boundaries in temperature–precipitation space | Package **MIT licence**. The underlying figure is © W.H. Freeman. The package's own polygons are an independent digitisation. |
| GLICK16 | Glick, Bettigole, Maynard, Covey, Smith & Crowther 2016, "Spatially-explicit models of global tree density", *Sci. Data* 3:160069 | Field plots and tree totals by biome | **CC BY 4.0** |
| CROW17 | Crowther, Glick, Maynard, Ashley-Cantello, Evans & Routh 2017, "Predicting Global Forest Reforestation Potential", bioRxiv 210062 | Mean current tree density per biome (trees ≥ 10 cm DBH) | **CC BY-ND 4.0 (no derivatives)** **[PREPRINT]** |
| BUR13 | Burrascano, Keeton, Sabatini & Blasi 2013, "Commonality and variability in the structural attributes of moist temperate old-growth forests: A global review", *For. Ecol. Manage.* 291:458–479 (author copy, Univ. Vermont) | Basal area, stem density, QMD; mature vs old-growth by continent | © Elsevier. Author copy. Facts cited. |
| LEW13 | Lewis et al. 2013, "Above-ground biomass and structure of 260 African tropical forests", *Phil. Trans. R. Soc. B* 368:20120295 (PMC3720018) | African stem density and basal area | **CC BY 3.0** |
| OST14 | Ostertag et al. 2014, "Forest structure in low-diversity tropical forests: a study of Hawaiian wet and dry forests", *PLoS ONE* 9:e103268 (PMC4146472) | Table 2: 21 CTFS-ForestGEO tropical plots (trees/ha, basal area) | **CC BY** |
| JAC21 | Jackson, Shenkin, Majalap et al. 2021, *Biotropica* 53:110–120, doi 10.1111/btp.12850 | Tallest tropical tree, 100.8 m | **CC BY 4.0** [ABSTRACT] |
| SK19 | Stefańska-Krzaczek, Staniaszek-Kik, Szczepańska & Szymura 2019, "Species diversity patterns in managed Scots pine stands in ancient forest sites", *PLoS ONE* 14:e0219620 (PMC6622550) | Layer heights and cover across stand age (chronosequence) | **CC BY** |
| REI33 | Reineke, L.H. 1933, "Perfecting a stand-density index for even-aged forests", *J. Agric. Res.* 46(7):627–638 (USDA Forest Service PDF) | Self-thinning slope −1.605; SDI | **US Government work, public domain** |
| PB05 | Pretzsch & Biber 2005, "A re-evaluation of Reineke's rule and stand density index", *For. Sci.* 51(4):304–320, doi 10.1093/forestscience/51.4.304 | Species slopes | © Society of American Foresters / OUP. [ABSTRACT] |
| FVSNE | USDA Forest Service, *Northeast (NE) Variant Overview of the Forest Vegetation Simulator* (Table 3.5.1) | Maximum SDI by species | **US Government work, public domain** |
| SEW18 | Salas-Eljatib & Weiskittel 2018, "Evaluation of modeling strategies for assessing self-thinning behavior and carrying capacity", *Ecol. Evol.* 8:10768–10779, doi 10.1002/ece3.4525 (PMC6262744) | Statement of the −3/2 law (Yoda et al. 1963) | **CC BY** (used as [SECONDARY]) |
| ZHA15 | Zhang, Zhang, Duan et al. 2015, "A Hierarchical Bayesian Model to Predict Self-Thinning Line for Chinese Fir in Southern China", *PLoS ONE* 10:e0139788 (PMC4594911) | Fixed slopes −1.605 / −1.5; Chinese fir posterior slope | **CC BY** |
| NV06 | Niinemets & Valladares 2006, *Ecol. Monogr.* 76:521–547; data in Ecological Archives M076-020-A1 | Shade, drought and waterlogging tolerance (0–5) for 806 woody species | Archive data **CC BY 4.0** (ESA Ecological Archives copyright page). Paper © ESA. |
| EIVE | Dengler et al. 2023, "Ecological Indicator Values for Europe (EIVE) 1.0", *Veg. Classif. Surv.* 4:7–29, doi 10.3897/vcs.98324; data Zenodo 7534792 (SM08) | Light, moisture and temperature niche position and width (0–10) | Paper and data **CC BY 4.0** (metadata). The SM08 file was 403 on independent recheck; §7b is **UNVERIFIABLE**. |
| FEIS | USDA Forest Service, Fire Effects Information System: Uchytil 1991, *Pseudotsuga menziesii* var. *menziesii* | Elevation range | **US Government work, public domain** |
| PK14 | Paulsen & Körner 2014, "A climate-based model to predict potential treeline position around the globe", *Alpine Botany* 124:1–12, doi 10.1007/s00035-014-0124-0 (GMBA / Univ. Bern PDF) | TREELIM thresholds; NZ treeline table | © Swiss Botanical Society / Springer. Facts cited. |
| KP04 | Körner & Paulsen 2004, *J. Biogeogr.* 31:713–732, doi 10.1111/j.1365-2699.2003.01043.x | Treeline soil isotherm | © Wiley. [ABSTRACT] |
| JJ00 | Jobbágy & Jackson 2000, "Global controls of forest line elevation in the northern and southern hemispheres", *Global Ecol. Biogeogr.* 9:253–268 (author PDF, Jackson Lab) | Forest-line regressions; Appendix 1 with 115 sites (latitude, elevation) | © Blackwell Science. Author copy. Facts cited. |
| TES23 | "Global distribution and climatic controls of natural mountain treelines", *Global Change Biol.* 29:7001–7011, doi 10.1111/gcb.16885 | Treeline shift rates | © Wiley. [ABSTRACT] |

**Non-commercial / restricted flags:**
- CROW17 is **CC BY-ND** (no derivatives), so cite its values and do not adapt the table.
- Nothing else here is non-commercial. The © items (PAN13, BUR13, PK14, JJ00) are cited as facts only.

## 3. Whittaker biome boundaries (mean annual temperature × precipitation)

**Status [DERIVED]:** I took the minimum and maximum vertex of each biome polygon in `plotbiomes::Whittaker_biomes` (775 vertices). Those polygons were digitised from Ricklefs 2008 Fig. 5.5, which is after Whittaker 1975. The polygons are not rectangles, so a biome does **not** fill the whole box below. Use the polygons themselves for point-in-biome tests. The primary Whittaker (1975) figure was not opened (copyrighted book).

| Biome (WHIT names) | Mean annual temp min–max (°C) | Annual precipitation min–max (cm) | Vertices | Key | URL |
|---|---|---|---|---|---|
| Tundra | −15.6 to −4.1 | 1.6–108.1 | 125 | WHIT | https://github.com/valentinitnelav/plotbiomes |
| Boreal forest | −6.7 to 4.4 | 18.7–201.3 | 96 | WHIT | same |
| Temperate grassland/desert | −10.2 to 17.9 | 0.0–52.6 | 39 | WHIT | same |
| Woodland/shrubland | −10.2 to 19.1 | 1.6–126.2 | 118 | WHIT | same |
| Temperate seasonal forest | −0.7 to 19.2 | 37.8–234.7 | 96 | WHIT | same |
| Temperate rain forest | 3.2 to 20.6 | 160.7–343.3 | 61 | WHIT | same |
| Tropical seasonal forest/savanna | 17.9 to 29.6 | 52.6–281.3 | 98 | WHIT | same |
| Subtropical desert | 14.1 to 30.0 | 0.0–99.9 | 40 | WHIT | same |
| Tropical rain forest | 19.2 to 28.5 | 234.7–444.2 | 102 | WHIT | same |

Dataset docs: https://rdrr.io/github/valentinitnelav/plotbiomes/src/R/data.R ; licence MIT: https://github.com/valentinitnelav/plotbiomes/blob/develop/DESCRIPTION

### 3b. FAO-zone forest biomes with climate and canopy height (PAN13 Table 1, pp.598–599)

| Domain | Forest biome | Mean annual temp | Annual precip (mm) | Seasonality | Canopy height (m) | Total biomass C (Mg C/ha, mean ± SD) | Key | URL |
|---|---|---|---|---|---|---|---|---|
| Tropical | Rainforest | ~20–25 °C | > 1,500 | no dry season | 25–50 | 145 ± 53 | PAN13 | https://research.fs.usda.gov/download/treesearch/45295.pdf |
| Tropical | Moist deciduous (monsoon) | > 15 °C | 1,000–2,000 | 3–5 dry months | 15–30 | 73 ± 47 | PAN13 | same |
| Tropical | Dry forest | > 15 °C | 500–1,500 | 5–8 dry months | 5–20 | 53 ± 35 | PAN13 | same |
| Tropical | Shrublands | > 15 °C | 200–500 | 8–11 dry months | 3–15 | 71 ± 45 | PAN13 | same |
| Tropical | Mountain systems | < 18 °C | 700–2,000 | 0–11 dry months | 3–35 | 124 ± 54 | PAN13 | same |
| Tropical | Mangrove | > 18 °C | 700–2,000+ | highly variable | 3–30 | 218 ± 173 | PAN13 | same |
| Tropical | Peat swamp | > 18 °C | 1,500–2,000+ | < 5 dry months | 12–50 | 206 ± 100 | PAN13 | same |
| Subtropical | Humid forest | 14–22 °C | 600–1,000+ | no dry season | 10–35 | 66 ± 46 | PAN13 | same |
| Subtropical | Dry (Mediterranean) | > 7 °C | 300–1,000 | winter rain, dry summer | 6–30 | 67 ± 60 | PAN13 | same |
| Subtropical | Mountain | < 12 °C | 500–2,000 | 0–8 dry months | 10–30+ | 77 ± 41 | PAN13 | same |
| Temperate | Oceanic | 5–11 °C (coldest month > 0 °C) | 600–3,500+ | year-round growing season | 50–100+ | 208 ± 131 | PAN13 | same |
| Temperate | Continental | ~10 °C (coldest month < 0 °C) | 750–1,500+ | 120–250 growing days | 25–40 | 61 ± 31 | PAN13 | same |
| Temperate | Mountain | < 10 °C | 1,000–2,500 | variable | 10–75+ | 59 ± 22 | PAN13 | same |
| Boreal | Coniferous | −12 to 6 °C | < 500 | < 100 growing days | < 15 | 48 ± 24 | PAN13 | same |
| Boreal | Tundra woodland | −15 to 0 °C (summer 6–14 °C) | 150–250 | 35–65 growing days | < 15 | 7 ± 6 | PAN13 | same |
| Boreal | Mountain | −14 to 5 °C (summer 6–16 °C) | 400+ | 50–80 growing days | < 15 | 19 ± 14 | PAN13 | same |

## 4. Stem density and basal area by biome

### 4a. Mean tree density per biome (trees ≥ 10 cm DBH)

| Biome (WWF) | Mean trees/ha | Predicted trees, billions (± 2 SD) | Field plots | Key | URL |
|---|---|---|---|---|---|
| Boreal forests / taiga | 967.59 | 749.3 ± 50.1 | 8,688 | CROW17 Table 2; GLICK16 Table 4 | https://www.biorxiv.org/content/biorxiv/early/2017/11/07/210062.full.pdf ; https://www.nature.com/articles/sdata201669/tables/5 |
| Tundra | 1,042.03 | 94.9 ± 6.3 | 2,268 | same | same |
| Mediterranean forests, woodlands and scrub | 881.39 | 53.4 ± 1.2 | 16,727 | same | same |
| Montane grasslands and shrublands | 799.31 | 60.3 ± 24.0 | 138 | same | same |
| Tropical and subtropical moist broadleaf | 779.26 | 799.4 ± 24.0 | 5,321 | same | same |
| Flooded grasslands and savannas | 497.24 | 64.6 ± 14.2 | 271 | same | same |
| Mangroves | 494.08 | 8.2 ± 0.3 (model borrowed from tropical moist) | 21 | same | same |
| Temperate broadleaf and mixed | 486.41 | 362.6 ± 2.9 | 278,395 | same | same |
| Temperate conifer | 426.19 | 150.6 ± 1.3 | 85,144 | same | same |
| Tropical and subtropical coniferous | 426.19 | 22.2 ± 0.4 (model borrowed from temperate conifer) | — | same | same |
| Tropical and subtropical dry broadleaf | 372.35 | 156.4 ± 63.4 | 115 | same | same |
| Tropical and subtropical grasslands, savannas, shrublands | 292.88 | 318.0 ± 35.5 | 999 | same | same |
| Temperate grasslands, savannas, shrublands | 285.47 | 148.3 ± 4.9 | 17,051 | same | same |
| Deserts and xeric shrublands | 244.57 | 53.0 ± 2.9 | 14,637 | same | same |
| **Global** | — | 3,041.2 ± 96.1 | 429,775 | GLICK16 Table 4 | same |

Notes:
- The per-hectare means are **[PREPRINT]** (CROW17). The totals and plot counts are from peer-reviewed GLICK16.
- GLICK16 defines a tree as ≥ 10 cm DBH and truncates predictions at 10,000 trees/ha.
- Nature's `/tables/5` page is article Table 4 (field-plot totals). The `/tables/4` URL is Table 3 (validation) and is not used.
- These are biome-wide means. They already include the forest-cover scaling, so they are lower than closed-stand values.

### 4b. Tropical forest plots (CTFS-ForestGEO; all stems ≥ 1 cm DBH)

All rows are from OST14 Table 2: https://pmc.ncbi.nlm.nih.gov/articles/PMC4146472/

| Plot (country) | Lat | Rain (mm/yr) | Elev (m) | Plot (ha) | Trees/ha | Basal area (m²/ha) | AGB (Mg/ha) |
|---|---|---|---|---|---|---|---|
| Lambir (Malaysia) | 4.19 | 2,664 | 170 | 52 | 6,915 | 43.5 | 497.2 |
| Yasuní (Ecuador) | −0.69 | 3,081 | 230 | 50 | 3,026 | 33.0 | 282.4 |
| Pasoh (Malaysia) | 2.98 | 1,788 | 80 | 50 | 6,708 | 31.0 | 339.8 |
| Korup (Cameroon) | 5.07 | 5,272 | 200 | 50 | 6,580 | 32.0 | — |
| BCI (Panama) | 9.15 | 2,551 | 140 | 50 | 4,168 | 32.1 | 306.5 |
| Palanan (Philippines) | 17.04 | 3,379 | 110 | 16 | 4,125 | 39.8 | 290.1 |
| Sinharaja (Sri Lanka) | 6.4 | 5,016 | 500 | 25 | 7,736 | 45.6 | 357.9 |
| Huai Kha Khaeng (Thailand, seasonal) | 15.63 | 1,474 | 590 | 50 | 1,450 | 31.2 | 211.2 |
| Mudumalai (India, dry deciduous) | 11.6 | 1,250 | 1,050 | 50 | 510 | 25.5 | 174.2 |
| La Planada (Colombia, montane) | 1.16 | 4,084 | 1,840 | 25 | 4,216 | 29.8 | 177.6 |
| Luquillo (Puerto Rico) | 18.33 | 3,548 | 380 | 16 | 4,194 | 38.3 | 276.1 |
| Nanjenshan (Taiwan) | 22.06 | 3,582 | 320 | 3 | 12,133 | 36.3 | — |
| Laupāhoehoe (Hawaiʻi, montane wet) | 19.93 | 3,440 | 1,150 | 4 | 3,078 | 67.36 | 247.9 |
| Pālamanui (Hawaiʻi, lowland dry) | 19.74 | 835 | 240 | 4 | 3,487 | 8.6 | 29.4 |

Notes:
- The mean stem density of the other CTFS plots in OST14's comparison is **4,733 ± 722 SE** (n = 14).
- Laupāhoehoe's high basal area is mostly tree ferns, which form a mid-canopy layer at about 5 m.

### 4c. Large-tree (≥ 10 cm) tropical plots

| Region | Stems ≥ 10 cm per ha | Basal area (m²/ha) | Other | Key | URL |
|---|---|---|---|---|---|
| African closed-canopy tropical forest, 260 plots (mean 1.2 ha), 12 countries | 425.6 (95% CI ± 11.1) | 30.3 (CI ± 0.77) | AGB 395.7 Mg/ha (CI ± 14.3); wood density 0.648 g/cm³ | LEW13 | https://pmc.ncbi.nlm.nih.gov/articles/PMC3720018/ |
| Amazonian and Bornean forests (comparison quoted by LEW13) | "cf. approx. 600" | — | **[SECONDARY]** | LEW13 | same |
| African plots vs latitude (quadratic fit) | — | BA = 32.7 − 0.150 × latitude² (r² = 0.18) | AGB = 451.6 − 3.57 × latitude² (r² = 0.31) | LEW13 Fig. 3 | same |

### 4d. Moist temperate mature and old-growth forests (global review)

All values are from BUR13 Table 2 (p.472): median, with min–max in brackets. M = mature; OG = old-growth.

https://www.uvm.edu/giee/pubpdfs/Burrascano_2013_Forest_Ecology_and_Management.pdf

| Region | Basal area M (m²/ha) | Basal area OG (m²/ha) | Stems/ha M | Stems/ha OG | QMD OG (cm) |
|---|---|---|---|---|---|
| Global | 41.7 [15.6–144.5] | 47.3 [24.4–379] | 536 [217.5–6,015] | 501 [96.7–9,680] | 34.6 [13.4–171.6] |
| Europe | 42.2 [30.7–69.2] | 38.5 [24.4–57.4] | 444 [259–2,182] | 341 [124–1,835] | 39.3 [14.6–64.3] |
| North America | 29.1 [24–59] | 44.4 [28.7–379] | 488.5 [217.5–1,000] | 392 [96.7–1,720] | 32.8 [15.7–171.6] |
| South America | 67 [47.4–97.9] | 94.6 [32.2–242.8] | 893 [360–3,600] | 865.5 [451–9,680] | 37.4 [13.5–57.3] |
| Asia | 42.2 [15.6–54.7] | 50.2 [34.3–69.4] | 632.5 [336–2,805] | 630 [281.3–4,570] | 28.4 [13.4–51.4] |
| Australia | 68.2 [61–144.5] | 62.6 [55.9–69.2] | 1,660 [1,216–6,015] | 444.7 | 40 |

Notes:
- **Large living trees** (DBH > 50 cm): global median 16.3/ha (mature) vs 72.6/ha (old-growth).
- **Stand age:** median 125 yr (mature) vs 250 yr (old-growth).
- **Minimum DBH:** differs between the source studies. BUR13 tested this and found no systematic difference by age class (p = 0.198).

### 4e. Boreal and other stand values found

| Biome | Value | Key | URL |
|---|---|---|---|
| Boreal coniferous: canopy height and biomass | < 15 m; 48 ± 24 Mg C/ha | PAN13 Table 1 | https://research.fs.usda.gov/download/treesearch/45295.pdf |
| Boreal forests / taiga: mean density | 967.59 trees/ha (≥ 10 cm) | CROW17 | see 4a |

## 5. Self-thinning (maximum size–density)

| Rule / quantity | Value | Units, form, data | Status | Key | URL |
|---|---|---|---|---|---|
| Reineke's reference curve | log N = −1.605 log D + k | N trees/acre; D = diameter of the tree of mean basal area (inches); k varies by species. k = 4.605 passes through 1,000 trees/acre at 10 in (p.628) | verified | REI33 | https://www.fs.usda.gov/psw/publications/cfres/cfres_1933_reineke001.pdf |
| SDI definition | trees per acre at a 10-inch mean diameter | (× 2.471 gives trees/ha at 25.4 cm: my conversion) | verified | REI33 | same |
| Reineke maximum SDI, white fir (*Abies concolor*) | 830 | trees/acre at 10 in (Fig. 3) | verified | REI33 | same |
| Reineke maximum SDI, Douglas-fir (WA/OR and N California) | "approximately 595" for both groups | Fig. 4 | verified | REI33 | same |
| California red fir (*Abies magnifica*) | fits the reference curve (1,000 at 10 in) | Fig. 2 | verified | REI33 | same |
| Yoda's −3/2 law | mean plant mass or volume ∝ N^(−3/2) | Yoda et al. 1963 | [SECONDARY]: original not opened | SEW18; ZHA15 | https://pmc.ncbi.nlm.nih.gov/articles/PMC6262744/ ; https://pmc.ncbi.nlm.nih.gov/articles/PMC4594911/ |
| Fixed slopes compared | −1.605 (Reineke, ln N vs ln D) and −1.5 (Yoda, ln size vs ln N) | — | [SECONDARY] | ZHA15 | same |
| Chinese fir (*Cunninghamia lanceolata*) plantations, Bayesian fit | posterior mean slope −1.47 (prior N(−1.52, 0.39)) | Southern China | verified | ZHA15 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4594911/ |
| Species slopes b in ln N = a + b ln d, 28 fully stocked pure stands in Germany since 1870 | *Fagus sylvatica* −1.789; *Picea abies* −1.664; *Pinus sylvestris* −1.593; *Quercus petraea* −1.424 | Beech differs significantly from the other species and from −1.605; spruce and oak differ from each other | [ABSTRACT] | PB05 | https://doi.org/10.1093/forestscience/51.4.304 |

### 5b. Maximum SDI by species (USDA FVS Northeast variant, Table 3.5.1)

Units are trees/acre at a 10-inch QMD. The source notes these come from "an unpublished analysis of FIA data by John Shaw".

https://www.fs.usda.gov/sites/default/files/forest-management/fvs-ne-overview.pdf

| Species | FVS code | SDI max |
|---|---|---|
| *Abies balsamea* (balsam fir) | BF | 655 |
| *Larix laricina* (tamarack) | TA | 387 |
| *Picea glauca* (white spruce) | WS | 412 |
| *Picea rubens* (red spruce) | RS | 506 |
| *Picea mariana* (black spruce) | BS | 500 |
| *Pinus resinosa* (red pine) | RN | 505 |
| *Pinus strobus* (eastern white pine) | WP | 529 |
| *Tsuga canadensis* (eastern hemlock) | EH | 510 |
| *Acer rubrum* (red maple) | RM | 421 |
| *Acer saccharum* (sugar maple) | SM | 371 |
| *Betula alleghaniensis* (yellow birch) | YB | 369 |
| *Betula papyrifera* (paper birch) | PB | 466 |
| *Fagus grandifolia* (American beech) | AB | 364 |
| *Populus tremuloides* (quaking aspen) | QA | 562 |
| *Quercus alba* (white oak) | WO | 361 |
| *Quercus rubra* (northern red oak) | RO | 414 |

FVS uses stand SDI max = basal-area-weighted mean of the species values (Zeide method). The default "actual maximum" is 0.85 of the theoretical value (SDIU).

### 5c. How a stand changes with age (one managed Scots pine chronosequence)

All rows are from SK19 Table 1 (Poland, 200 m² plots, mean ± SD; the Kruskal–Wallis p < 0.001 for every row).

https://pmc.ncbi.nlm.nih.gov/articles/PMC6622550/

| Stand age class | DBH (mm) | Top height T1 (m) | Cover T1 > 10 m | T2 8.1–10 m | S1 3.1–8 m | S2 0.6–3 m | Herb ≤ 0.5 m | Moss |
|---|---|---|---|---|---|---|---|---|
| 1 Initiation (4–10 yr) | — | 1.80 ± 0.42 | 0 % | 0.5 ± 1.58 % | 0.5 ± 1.58 % | 64.5 ± 14.99 % | 44.5 ± 15.17 % | 24.5 ± 15.54 % |
| 2 Young (20–35 yr) | 156.65 ± 12.25 | 15.80 ± 3.39 | 67.0 ± 6.75 % | 0 % | 4.5 ± 7.98 % | 7.3 ± 6.41 % | 32.5 ± 11.37 % | 68.5 ± 13.75 % |
| 3 Middle-aged (45–60 yr) | 225.35 ± 25.05 | 23.30 ± 2.36 | 57.5 ± 7.91 % | 3.1 ± 5.32 % | 4.6 ± 4.88 % | 10.3 ± 7.02 % | 66.5 ± 10.81 % | 77.5 ± 10.34 % |
| 4 Pre-mature (70–85 yr) | 281.85 ± 25.77 | 27.00 ± 3.46 | 51.5 ± 7.84 % | 10.0 ± 11.79 % | 15.1 ± 7.96 % | 7.1 ± 4.04 % | 63.5 ± 9.14 % | 76.0 ± 14.87 % |
| 5 Mature (95–110 yr) | 298.77 ± 22.76 | 27.10 ± 2.85 | 46.5 ± 7.09 % | 15.5 ± 11.65 % | 19.0 ± 12.87 % | 6.2 ± 3.61 % | 68.0 ± 14.57 % | 67.5 ± 13.59 % |

## 6. Forest layers: typical heights and cover

| Layer | Height definition / typical height | Typical cover | Context | Key | URL |
|---|---|---|---|---|---|
| Layer scheme A (Polish pine relevés) | high tree > 10 m; low tree 8.1–10 m; high shrub 3.1–8 m; low shrub 0.6–3 m; herb ≤ 0.5 m (herbs, dwarf shrubs, ferns, tree seedlings); moss/lichen layer | see §5c | managed *Pinus sylvestris* stands | SK19 | https://pmc.ncbi.nlm.nih.gov/articles/PMC6622550/ |
| Layer scheme B (Italian relevés) | tree > 7 m; shrub 20 cm–7 m; herb < 20 cm | — | sub-Mediterranean hardwood vs *Ailanthus* forests | MON20: Montecchiari, Tesei, Allegrezza et al. 2020, *Plants* 9:1404 (CC BY) | https://pmc.ncbi.nlm.nih.gov/articles/PMC7589998/ |
| Canopy height by biome | e.g. tropical rainforest 25–50 m; temperate oceanic 50–100+ m; boreal < 15 m (full list in §3b) | — | FAO zones | PAN13 Table 1 | https://research.fs.usda.gov/download/treesearch/45295.pdf |
| Emergent maximum, tropics | 100.8 m (*Shorea faguetiana* "Menara", Sabah) | — | tallest known tropical tree | JAC21 [ABSTRACT] | https://doi.org/10.1111/btp.12850 |
| Tree-fern mid-canopy layer | about 5 m above ground | tree ferns are a large share of basal area (67.3 m²/ha total) | Hawaiian montane wet forest | OST14 | https://pmc.ncbi.nlm.nih.gov/articles/PMC4146472/ |

## 7. Which plants grow where: light, moisture, temperature, elevation

### 7a. Shade, drought and waterlogging tolerance (0 = none, 5 = maximal; mean ± SE across sources)

All rows are from NV06 Appendix A: https://esapubs.org/archive/mono/M076/020/appendix-A.htm

| Species | Shade | Drought | Waterlogging |
|---|---|---|---|
| *Tsuga canadensis* | 4.83 ± 0.14 | 1 | 1.25 ± 0.25 |
| *Acer saccharum* | 4.76 ± 0.11 | 2.25 ± 0.25 | 1.09 ± 0.08 |
| *Fagus grandifolia* | 4.75 ± 0.14 | 1.5 ± 0.32 | 1.5 ± 0.06 |
| *Abies alba* | 4.6 ± 0.06 | 1.81 ± 0.28 | 1.02 ± 0.02 |
| *Fagus sylvatica* | 4.56 ± 0.11 | 2.4 ± 0.43 | 1.02 ± 0.01 |
| *Picea abies* | 4.45 ± 0.5 | 1.75 ± 0.41 | 1.22 ± 0.12 |
| *Picea glauca* | 4.15 ± 0.17 | 2.88 ± 0.12 | 1.02 ± 0.02 |
| *Picea mariana* | 4.08 ± 0.18 | 2 | 2 |
| *Acer pseudoplatanus* | 3.73 ± 0.21 | 2.75 ± 0.16 | 1.1 ± 0.08 |
| *Pinus strobus* | 3.21 ± 0.2 | 2.29 ± 0.38 | 1.03 ± 0.02 |
| *Pinus cembra* | 2.87 ± 0.3 | 3.01 ± 0.43 | 1.04 |
| *Quercus alba* | 2.85 ± 0.17 | 3.56 ± 0.3 | 1.43 ± 0.14 |
| *Pseudotsuga menziesii* | 2.78 ± 0.18 | 2.62 ± 0.41 | 1.79 ± 0.12 |
| *Quercus rubra* | 2.75 ± 0.18 | 2.88 ± 0.12 | 1.12 ± 0.06 |
| *Quercus petraea* | 2.73 ± 0.27 | 3.02 ± 0.15 | 1.2 ± 0.2 |
| *Quercus robur* | 2.45 ± 0.28 | 2.95 ± 0.31 | 1.89 ± 0.18 |
| *Betula pendula* | 2.03 ± 0.09 | 1.85 ± 0.21 | 1.67 ± 0.12 |
| *Salix alba* | 1.99 ± 0.18 | 2 ± 0.21 | 4.1 ± 0.03 |
| *Betula pubescens* | 1.85 ± 0.07 | 1.27 ± 0.18 | 2.98 ± 0.21 |
| *Pinus sylvestris* | 1.67 ± 0.33 | 4.34 ± 0.47 | 2.63 ± 0.08 |
| *Pinus ponderosa* | 1.64 ± 0.15 | 4.32 ± 0.32 | 1.02 ± 0.02 |
| *Betula papyrifera* | 1.54 ± 0.16 | 2.02 ± 0.3 | 1.25 ± 0.15 |
| *Larix decidua* | 1.46 ± 0.29 | 2.31 ± 0.55 | 1.1 ± 0.08 |
| *Populus tremuloides* | 1.21 ± 0.18 | 1.77 ± 0.23 | 1.77 ± 0.23 |

### 7b. European indicator values (EIVE 1.0)

The scale is 0–10, where 0 is the lowest and 10 the highest value present in Europe. M = soil moisture, L = light, T = temperature. The niche-width column ("nw") follows each position; n = number of source systems behind each value. Values are rounded by me to 2 dp.

Data: https://zenodo.org/records/7534792 (file `EIVE_Paper_1.0_SM_08.xlsx`); paper https://doi.org/10.3897/vcs.98324

**UNVERIFIABLE (independent recheck, 2026-11-16):** the Zenodo file download for `EIVE_Paper_1.0_SM_08.xlsx` returned 403, so these 25 taxon rows could not be recomputed from the spreadsheet. The record metadata still states **CC BY 4.0** for the data. The numbers are left as first compiled.

| Taxon | M | M nw | L | L nw | T | T nw | n (M/L/T) |
|---|---|---|---|---|---|---|---|
| *Fagus sylvatica* | 4.57 | 3.23 | 3.00 | 7.78 | 4.11 | 3.76 | 24/18/20 |
| *Abies alba* | 5.18 | 4.18 | 3.04 | 8.37 | 3.68 | 3.25 | 18/15/18 |
| *Acer pseudoplatanus* | 5.05 | 3.23 | 3.80 | 6.15 | 4.16 | 4.18 | 24/18/17 |
| *Picea abies* | 4.98 | 4.46 | 4.36 | 8.18 | 2.52 | 3.53 | 18/16/16 |
| *Pseudotsuga menziesii* (introduced) | 4.77 | 3.07 | 5.08 | 8.52 | 4.26 | 3.86 | 6/7/5 |
| *Quercus ilex* | 2.94 | 4.29 | 4.97 | 9.60 | 6.74 | 3.28 | 10/10/9 |
| *Quercus petraea* | 4.20 | 3.35 | 5.83 | 5.83 | 4.75 | 3.10 | 21/17/18 |
| *Pinus cembra* | 4.60 | 2.92 | 5.97 | 8.81 | 1.93 | 4.28 | 12/9/10 |
| *Pteridium aquilinum* | 4.61 | 3.08 | 5.97 | 5.81 | 4.22 | 4.61 | 26/20/21 |
| *Quercus robur* | 4.80 | 4.26 | 6.24 | 6.88 | 4.69 | 3.27 | 22/19/19 |
| *Betula pubescens* | 6.29 | 4.22 | 6.75 | 9.70 | 3.10 | 3.62 | 15/10/8 |
| *Betula pendula* | 4.45 | 4.43 | 6.84 | 4.86 | 3.44 | 4.51 | 20/16/15 |
| *Juncus effusus* | 6.24 | 3.49 | 6.88 | 4.89 | 4.22 | 4.57 | 27/19/21 |
| *Phragmites australis* | 7.48 | 5.58 | 6.98 | 4.55 | 4.30 | 5.54 | 27/20/19 |
| *Pinus sylvestris* | 4.37 | 6.67 | 7.10 | 5.51 | 3.71 | 4.59 | 20/18/15 |
| *Calluna vulgaris* | 4.56 | 5.48 | 7.25 | 5.70 | 3.57 | 5.27 | 23/19/17 |
| *Larix decidua* | 3.99 | 3.42 | 7.48 | 5.32 | 2.80 | 5.10 | 18/15/11 |
| *Typha latifolia* | 7.98 | 3.08 | 7.77 | 3.61 | 4.48 | 3.68 | 25/18/19 |
| *Lolium perenne* | 4.27 | 3.24 | 7.86 | 3.05 | 4.27 | 4.14 | 26/20/22 |
| *Olea europaea* | 3.21 | 4.37 | 7.96 | 4.48 | 7.58 | 3.52 | 5/5/5 |
| *Pinus halepensis* | 2.56 | 5.01 | 8.26 | 5.83 | 7.26 | 4.44 | 5/5/5 |
| *Eucalyptus globulus* (introduced) | 3.36 | 2.57 | 8.79 | 3.96 | 6.75 | 3.32 | 2/2/2 |
| *Salix alba* | 6.53 | 3.12 | 5.77 | 5.82 | 4.76 | 3.66 | 24/19/21 |
| *Alnus glutinosa* | 7.17 | 3.56 | 5.22 | 5.55 | 4.23 | 3.79 | 25/18/20 |
| *Vaccinium myrtillus* | 4.68 | 4.72 | 4.81 | 8.38 | 2.91 | 5.27 | 25/20/18 |

### 7c. Elevation ranges (species)

| Species | Elevation | Key | URL |
|---|---|---|---|
| *Pseudotsuga menziesii* var. *menziesii* | near sea level to > 1,524 m (WA, OR); 609–1,829 m (S Oregon Cascades, N Sierra Nevada); up to 2,286 m (S Sierra Nevada). Does not tolerate frost below −10 °C for more than a week | FEIS (Uchytil 1991) | https://research.fs.usda.gov/feis/species-reviews/psemenm |
| Treeline dominants by site (examples from JJ00 Appendix 1) | e.g. *Larix gmelinii* forest line at 195 m (Bulun, 70.6°N); *Larix sibirica* 200–770 m (69.5–67.9°N); *Nothofagus* 450 m (Hermite Is., 56.8°S) to 1,900 m (38.3°S) | JJ00 | see §8 |

### 7d. Slope

**Not found** as species rules with numbers (see §9). The only numbers found came from search summaries about avalanche protection forests (starting zones of about 30–55°; Bebi et al. 2009, *For. Ecol. Manage.*). **[UNVERIFIED]** I did not open that paper, so it is not used.

## 8. Treeline against latitude and temperature

| Quantity | Value | Status | Key | URL |
|---|---|---|---|---|
| TREELIM growing-season definition | days with daily mean ≥ 0.9 °C; season ≥ 94 days; season mean ≥ 6.4 °C | verified | PK14 | https://www.gmba.unibe.ch/unibe/portal/microsites/micro_gmba/content/e1594357/e1599410/e1599467/pane1599468/e1599482/KrnerandPaulsen._AlpineBotany_2014_eng.pdf |
| Calibration and fit | 376 satellite-identified treelines; predicted = 1.005 × measured − 1 m, r² = 0.997, RMSE = 78 m; lapse rate −0.0055 K/m | verified | PK14 Fig. 5 | same |
| Treeless below | < 200 mm annual precipitation, regardless of climate zone | verified | PK14 | same |
| Potential forest area | 88 Mio km² total (12 montane; 58 boreal, temperate, subtropical and seasonal tropical; 18 per-humid tropics); 33 Mio km² too dry | verified | PK14 | same |
| New Zealand treelines, measured | 854 m (45.45°S) to 1,535 m (41.97°S); 18 sites; mean model error 11 m | verified | PK14 Table 1 | same |
| Treeline root-zone isotherm | 6.7 °C ± 0.8 SD (46 sites, 68°N–42°S); 7–8 °C temperate and Mediterranean; 5–6 °C equatorial; 6–7 °C subarctic and boreal | [ABSTRACT] | KP04 | https://doi.org/10.1111/j.1365-2699.2003.01043.x |
| Forest-line regression (global, 115 extratropical sites) | elevation = 128·MAT + 63·STA − 613 (m; MAT = sea-level mean annual temp in °C; STA = warmest minus coldest month in K); R² = 0.79 | verified | JJ00 Table 1 | https://jacksonlab.stanford.edu/sites/g/files/sbiybj20871/files/media/file/geb162.pdf |
| Northern hemisphere (n = 93) | 134·MAT + 70·STA − 847, R² = 0.79; MAT only: 64·MAT + 1,392, R² = 0.50 | verified | JJ00 Table 1 | same |
| Southern hemisphere (n = 22) | 110·MAT + 42·STA − 164, R² = 0.88; MAT only: 126·MAT + 90, R² = 0.83 | verified | JJ00 Table 1 | same |
| Shape of the latitude pattern | elevation rises from 70°N to 30°N, dips slightly from 30°N to 20°S, then falls steeply toward southern South America and NZ. At the same latitude it is 1,000–1,500 m higher in the NH; oceanic islands are 1,000–2,000 m lower than nearby continents | [SECONDARY] (JJ00 quoting Körner 1998, Leuschner 1996) | JJ00 Introduction | same |
| Life-form temperatures at the forest line (cold month / warm month, °C) | broadleaf evergreen −1.9 / 6.8; broadleaf deciduous −4.3 / 8.5; conifer evergreen −18.0 / 9.3; conifer deciduous −32.3 / 6.9 | verified | JJ00 Table 2 | same |
| Treeline shift, 2000s | 70 % of closed-loop treelines moved upward; mean 1.2 m/yr (tropics 3.1 m/yr); ~916,425 km of treeline in 243 ranges | [ABSTRACT] | TES23 | https://doi.org/10.1111/gcb.16885 |

### 8b. Forest-line elevation by latitude band

**Status [DERIVED]:** I parsed 113 of the 115 sites in JJ00 Appendix 1 (two lines did not parse) and grouped them into 10° bands. All sites are extratropical; the study covers only above 30° latitude.

| Band | n | Min (m) | Max (m) | Mean (m) |
|---|---|---|---|---|
| 30–40°N | 8 | 2,200 | 3,550 | 3,049 |
| 40–50°N | 27 | 1,100 | 3,406 | 1,976 |
| 50–60°N | 33 | 300 | 2,300 | 1,362 |
| 60–70°N | 22 | 200 | 1,200 | 709 |
| 70–80°N | 1 | 195 | 195 | 195 |
| 30–40°S | 2 | 1,750 | 1,900 | 1,825 |
| 40–50°S | 17 | 1,000 | 1,550 | 1,273 |
| 50–60°S | 3 | 450 | 850 | 633 |

## 9. Not found

- **Tropical treeline elevations by latitude**, as a primary table. JJ00 is extratropical only. KP04's Table 1 (with tropical sites) is paywalled: the Wiley page and the third-party PDF both returned 403.
- **Slope rules with numbers per species or biome.** Nothing usable was found. The avalanche-forest slope range is **[UNVERIFIED]**.
- **Boreal stand tables** (stems/ha and basal area for real stands, by age), from a source I could open. The Finnish De Meo et al. (SEEFOR) PDF returned an HTML page instead of a PDF. Only biome means (CROW17) and the PAN13 structure are given.
- **Typical tropical rainforest strata heights** (understorey, sub-canopy, canopy) as numbers, apart from PAN13 canopy ranges and the emergent maximum.
- **Silvics of North America** (USDA Agriculture Handbook 654): srs.fs.usda.gov failed the TLS handshake from the box and returned 500 to WebFetch. Species elevation and shade-tolerance summaries could not be read there; NV06 and FEIS are used instead.
- **EIVE SM08 spreadsheet:** the Zenodo file download returned 403 on the independent recheck (2026-11-16). The 25 taxon rows in §7b are **UNVERIFIABLE**. The record metadata still says CC BY 4.0.
- **Yoda et al. 1963** original paper: not opened. The −3/2 rule is cited [SECONDARY].
- **Whittaker 1975** original figure: not opened (copyrighted book). Boundaries come from the MIT-licensed digitisation of Ricklefs 2008.
- **Pretzsch & Biber 2005 intercepts and maximum densities:** abstract only.
- **GLICK16 Table 3 per-biome means:** the unit is per ~1 km² model pixel (0.805 km² in the Goode projection), so I did not use or convert them. CROW17 per-hectare means are used instead.

## 10. Where sources disagree

| Topic | Source A | Source B | Comment |
|---|---|---|---|
| Boreal temperature range | WHIT polygon −6.7 to 4.4 °C | PAN13 −12 to 6 °C | Different classifications (Whittaker vs FAO zones). |
| Boreal precipitation | WHIT 18.7–201.3 cm (polygon extremes) | PAN13 < 500 mm | WHIT's top vertex is an outer boundary of the polygon, not a typical value. |
| Self-thinning slope | REI33 −1.605 for "many species" | PB05 −1.424 (oak) to −1.789 (beech) | Species-specific slopes; Scots pine (−1.593) agrees with Reineke. |
| Tree density counted | CTFS ≥ 1 cm DBH: about 1,450–12,133/ha (OST14) | ≥ 10 cm DBH: about 426/ha in Africa (LEW13), "~600" in Amazonia/Borneo | Threshold-dependent; never mix the two. |
| Douglas-fir maximum SDI | REI33 ≈ 595 | — | FVS PN variant uses plant-association values capped at 950 (not tabulated here). |
| Treeline temperature | KP04 6.7 °C root-zone mean | PK14 6.4 °C air temperature over the season | Different measures (soil vs modelled air). |

## 11. References (all accessed 2026-11-16)

**Biomes, density and stand structure**

1. Pan, Y., Birdsey, R.A., Phillips, O.L. & Jackson, R.B. 2013. *Annu. Rev. Ecol. Evol. Syst.* 44:593–622.
   - DOI: https://doi.org/10.1146/annurev-ecolsys-110512-135914
   - PDF: https://research.fs.usda.gov/download/treesearch/45295.pdf
2. Stefan, V. & Levin, S. `plotbiomes` (MIT).
   - Code: https://github.com/valentinitnelav/plotbiomes
   - Data: https://raw.githubusercontent.com/valentinitnelav/plotbiomes/develop/data/Whittaker_biomes.rda
   - Archive: https://zenodo.org/records/7145245
   - Digitised from Ricklefs, R.E. 2008, *The Economy of Nature*, W.H. Freeman, Fig. 5.5.
3. Glick, H.B. et al. 2016. "Spatially-explicit models of global tree density." *Sci. Data* 3:160069.
   - Paper: https://www.nature.com/articles/sdata201669
   - Table 4: https://www.nature.com/articles/sdata201669/tables/5
4. Crowther, T.W. et al. 2017. "Predicting Global Forest Reforestation Potential." bioRxiv 210062 (CC BY-ND). https://www.biorxiv.org/content/biorxiv/early/2017/11/07/210062.full.pdf
5. Burrascano, S., Keeton, W.S., Sabatini, F.M. & Blasi, C. 2013. *For. Ecol. Manage.* 291:458–479. https://www.uvm.edu/giee/pubpdfs/Burrascano_2013_Forest_Ecology_and_Management.pdf
6. Lewis, S.L. et al. 2013. *Phil. Trans. R. Soc. B* 368:20120295. https://pmc.ncbi.nlm.nih.gov/articles/PMC3720018/
7. Ostertag, R. et al. 2014. *PLoS ONE* 9:e103268. https://pmc.ncbi.nlm.nih.gov/articles/PMC4146472/
8. Jackson, T.D., Shenkin, A.F., Majalap, N. et al. 2021. *Biotropica* 53:110–120. https://doi.org/10.1111/btp.12850
9. Stefańska-Krzaczek, E., Staniaszek-Kik, M., Szczepańska, K. & Szymura, T.H. 2019. *PLoS ONE* 14:e0219620. https://pmc.ncbi.nlm.nih.gov/articles/PMC6622550/
10. Montecchiari, S., Tesei, G., Allegrezza, M. et al. 2020. "*Ailanthus altissima* forests determine a shift in herbaceous layer richness…" *Plants* 9:1404 (CC BY). https://doi.org/10.3390/plants9101404 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC7589998/

**Self-thinning**

11. Reineke, L.H. 1933. *J. Agric. Res.* 46(7):627–638. https://www.fs.usda.gov/psw/publications/cfres/cfres_1933_reineke001.pdf ; https://research.fs.usda.gov/treesearch/60134
12. Pretzsch, H. & Biber, P. 2005. *For. Sci.* 51(4):304–320. https://doi.org/10.1093/forestscience/51.4.304
13. USDA Forest Service. Northeast (NE) Variant Overview, Forest Vegetation Simulator. https://www.fs.usda.gov/sites/default/files/forest-management/fvs-ne-overview.pdf
14. Salas-Eljatib, C. & Weiskittel, A.R. 2018. "Evaluation of modeling strategies for assessing self-thinning behavior and carrying capacity." *Ecol. Evol.* 8:10768–10779. https://doi.org/10.1002/ece3.4525 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC6262744/
15. Zhang, X., Zhang, J., Duan, A. et al. 2015. "A Hierarchical Bayesian Model to Predict Self-Thinning Line for Chinese Fir in Southern China." *PLoS ONE* 10:e0139788. https://doi.org/10.1371/journal.pone.0139788 ; https://pmc.ncbi.nlm.nih.gov/articles/PMC4594911/

**Species niches**

16. Niinemets, Ü. & Valladares, F. 2006. *Ecol. Monogr.* 76:521–547.
    - Appendix A: https://esapubs.org/archive/mono/M076/020/appendix-A.htm
    - Licence: https://esapubs.org/archive/copyright.htm
17. Dengler, J. et al. 2023. *Veg. Classif. Surv.* 4:7–29.
    - Paper: https://doi.org/10.3897/vcs.98324
    - Data: https://zenodo.org/records/7534792
18. Uchytil, R.J. 1991. *Pseudotsuga menziesii* var. *menziesii*. FEIS. https://research.fs.usda.gov/feis/species-reviews/psemenm

**Treeline**

19. Paulsen, J. & Körner, C. 2014. *Alpine Botany* 124:1–12.
    - DOI: https://doi.org/10.1007/s00035-014-0124-0
    - PDF: https://www.gmba.unibe.ch/unibe/portal/microsites/micro_gmba/content/e1594357/e1599410/e1599467/pane1599468/e1599482/KrnerandPaulsen._AlpineBotany_2014_eng.pdf
20. Körner, C. & Paulsen, J. 2004. *J. Biogeogr.* 31:713–732. https://doi.org/10.1111/j.1365-2699.2003.01043.x
21. Jobbágy, E.G. & Jackson, R.B. 2000. *Global Ecol. Biogeogr.* 9:253–268.
    - DOI: https://doi.org/10.1046/j.1365-2699.2000.00162.x
    - PDF: https://jacksonlab.stanford.edu/sites/g/files/sbiybj20871/files/media/file/geb162.pdf
22. "Global distribution and climatic controls of natural mountain treelines." *Global Change Biol.* 29:7001–7011 (2023). https://doi.org/10.1111/gcb.16885
