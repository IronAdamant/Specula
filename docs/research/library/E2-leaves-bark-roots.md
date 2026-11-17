# E2: Leaves, bark and roots: leaf shape, margin, venation and size; bark type, thickness and furrows; root flare and buttresses

Research item E2 of `LIBRARY_RESEARCH_BRIEF.md`. Compiled 2026-11-16 (Australia/Sydney). **Every URL was accessed on 2026-11-16.**

## Fixes after verify (2026-11-16)

Applied from `E2-leaves-bark-roots.md.verify.md`. Finished 2026-11-17 after the usage pause (the research file was still at mtime 2026-11-16 10:39:15 AEDT, so none of these had been applied).

1. **VicFlora licence:** text is **CC BY 4.0**; most images are **CC BY-NC-SA 4.0** (https://vicflora.rbg.vic.gov.au/pages/copyright). Only text and facts are used here; images are not. The two taxon pages that failed the 10:39 AEDT recheck (*Olea*, *A. pseudoplatanus*) opened on the verifier's retry (HTTP 200) and the leaf/bark wording matches, so the 'failed recheck' note is removed.
2. **Cushman 2014:** now cited from the public-domain ForestGEO PDF (U.S. Government work in the USA), https://si-forestgeo01.si.edu/Public/pdfs/CushmanConditEtAl_2014_MEE.pdf. The PDF confirms 190 stems, uncorrected −0.21 %/yr becoming +0.18 %/yr (1985–2010), and that more than half of forest biomass at BCI is in trees measured above 1.3 m. The [ABSTRACT] flag is removed.
3. **MGP15 coppice bark thickness (Table 3):** the five 2nd-rotation spacings are now listed with their own SDs: **8.5 ± 2.6, 7.7 ± 1.5, 8.6 ± 2.0, 7.9 ± 2.6 and 8.6 ± 1.6 mm**. The old range '7.7 ± 1.5 to 8.6 ± 2.6 mm' mis-paired 8.6 with ± 2.6. The 1st-rotation range 6.6 ± 2.8 to 10.3 ± 2.5 mm was already correctly paired.
4. **FOFEM:** the main source is now the public-domain FOFEM 6.7 User Guide (Lutes, Feb 2020), p. 58, https://research.fs.usda.gov/sites/default/files/2023-03/rmrs-fofem_6-7_user_guide.pdf. Index 36 is **0.063** there (and in the FFE / RMRS-GTR-116 addendum for Douglas-fir and ponderosa pine). `xfblib.cpp` is kept as a secondary source (all rights reserved); its `Fofem_Sbt[36]` is **0.065**, used for Douglas-fir, ponderosa pine and western larch. The table now uses 0.063, with derived bark thickness 1.89 cm at 30 cm DBH and 3.78 cm at 60 cm DBH, and the 0.065 disagreement is under 'Sources disagree'. FOFEM 4 is cited at https://www.fs.usda.gov/rm/pubs_int/int_gtr344.pdf (the Treesearch URL also serves the PDF).

## 1. Method

- **The grok CLI was not used** because it is still signed out. I used WebSearch to find sources and then read each source directly:
  - Flora of North America (FNA) and VicFlora species pages, through WebFetch;
  - USDA Forest Service PDFs, read with `pdftotext`;
  - open-access papers from the publisher or Europe PMC full text;
  - the Crossref and Semantic Scholar APIs for metadata and licences;
  - one open dataset (UK EIDC, Open Government Licence), unzipped into `/tmp/e2` only.

  Nothing was saved into any repo, and nothing was posted or sent.
- **Link check (curl, 2026-11-16):** 42 URLs were checked.
  - 200 for the USDA, FEIS, EIDC, Silva Fennica, PMC, MLA99, Oregon State, bibliotekanauki, frames.gov and Springer article pages.
  - Expected bot blocks (curl only), all of which opened with WebFetch earlier the same day: FNA (500 to curl), MDPI (403) and OUP (403). The doi.org redirects for 10.1007/s00468-014-0995-0 (500), 10.1111/2041-210X.12187 (403) and 10.1093/jpe/rts031 (403) also failed; the first has a working Springer page.
  - The two VicFlora taxon pages (*Olea*, *A. pseudoplatanus*) that had returned 502 / no response at the 10:39 AEDT recheck opened on retry (HTTP 200); the leaf and bark wording matches. Copyright page: https://vicflora.rbg.vic.gov.au/pages/copyright.
- **Status of each number:** every number was read on the linked page, PDF or dataset unless the row carries a flag:
  - **[ABSTRACT]**: from the publisher abstract only;
  - **[DERIVED]**: my own arithmetic or summary, with the method stated;
  - **[OCR]**: from a scanned USDA PDF whose text layer has OCR errors. The value was cross-checked where possible.
- **Text:** descriptive wording is paraphrased. No flora wording or figures are copied beyond short technical terms (e.g. "doubly serrate"). FNA ranges use flora notation: (x–)a–b(–y) means usually a–b, with extremes x and y.
- **Leaf size class** uses the Webb (1955) blade-area classes as given in the Manual of Leaf Architecture (MLA99). Blade area ≈ length × width × 2/3 (MLA99's approximation). This approximation overstates the area of deeply lobed leaves (oaks, maples), so treat those classes as upper bounds.

## 2. Sources and licences

| Key | Source | What it gives | Licence (plainly) |
|---|---|---|---|
| FNA | Flora of North America Editorial Committee, *Flora of North America North of Mexico*, floranorthamerica.org. V2 (1993: *Pinus*, *Picea*, *Pseudotsuga*), V3 (1997: *Quercus*, *Fagus*, *Betula*), V13 (*Acer*, Sapindaceae) | Leaf size, shape, margin, veins; bark description | © Flora of North America Association. No open licence found. Facts only. |
| VIC | VicFlora, Royal Botanic Gardens Victoria: *Eucalyptus globulus* (from Brooker & Slee 1996), *Olea europaea* subsp. *europaea* (Jeanes 1999), *Acer pseudoplatanus* (Walsh 1999); "Multi-access key to the Eucalypts of Victoria: character descriptions" | Leaf and bark descriptions; eucalypt bark-type definitions | **Text CC BY 4.0**; most images **CC BY-NC-SA 4.0** (https://vicflora.rbg.vic.gov.au/pages/copyright). Only text and facts are used; images are not. |
| MLA99 | Leaf Architecture Working Group 1999, *Manual of Leaf Architecture*, Smithsonian Institution, ISBN 0-9677554-0-9 (author-hosted PDF, Penn State) | Leaf-size classes (after Webb 1955); vein category definitions | **© 1999 Smithsonian Institution, all rights reserved.** Class limits are cited as facts; figures are not reproduced. |
| FEIS | Uchytil 1991, *Pseudotsuga menziesii* var. *menziesii*, Fire Effects Information System, USDA Forest Service | Mature bark thickness, needle length | **US Government work, public domain** |
| FOFEM4 | Reinhardt, Keane & Brown 1997, *First Order Fire Effects Model: FOFEM 4.0, User's Guide*, USDA Forest Service Gen. Tech. Rep. INT-GTR-344, Table 1 | Bark-thickness equations for 12 western species | **Public domain** (scanned. **[OCR]**) |
| FOFEM67 | Lutes, D.C. 2020, *FOFEM 6.7 First Order Fire Effects Model: User Guide*, USDA Forest Service, February 2020, p. 58 | Official single-bark-thickness coefficient *v* by index (1–39) | **US Government work, public domain** |
| BPX | Bevins, C.D., `xfblib.cpp` (BehavePlus 5 fire library), arrays `FofemSpecies[]` and `Fofem_Sbt[]`, labelled "FOFEM 5.0 equations" | FOFEM 5 single-bark-thickness multiplier per species (secondary; species-to-index mapping) | **Code © 2002–2011 Collin D. Bevins, all rights reserved.** Only the numeric coefficients (facts from the public FOFEM model) are cited. No code is copied. |
| FVSNE | USDA Forest Service, *Northeast (NE) Variant Overview of the Forest Vegetation Simulator*, §4.2, Table 4.2.1 (bark ratios), Table 3.2.1 (species groups) | Diameter inside bark / outside bark ratio by species group | **Public domain** |
| KPS09 | Kaźmierczak, Pazdrowski, Szymański & Nawrot 2009, "Statistical characteristics of double bark thickness at breast height based on pine (*Pinus sylvestris* L.), larch (*Larix decidua* Mill.) and oak (*Quercus robur* L.)", *Colloquium Biometricum* 39:157–168 | Measured double bark thickness, regressions | No licence statement found on bibliotekanauki.pl. Treated as all rights reserved; facts only. |
| VAS24 | Vasilescu, Cieszewski, Ferraz Filho & De Araújo 2024, "Bark Biometry Along the Stem for Three Commercial Tree Species in Romania", *Forests* 15(12):2264 | Relative double bark thickness ranges, sample sizes | **CC BY 4.0** (MDPI) |
| MGP15 | Miranda, Gominho & Pereira 2015, "Heartwood, sapwood and bark variation in coppiced *Eucalyptus globulus* trees in 2nd rotation…", *Silva Fennica* 49(1):1141, doi 10.14214/sf.1141 | *E. globulus* bark thickness | **CC BY-SA 4.0 (share-alike)** |
| IK14 | Ilek & Kucza 2014, "Hydrological properties of bark of selected forest tree species. Part I: the coefficient of development of the interception surface of bark", *Trees* 28:831–839, doi 10.1007/s00468-014-0995-0 | Bark surface-roughness coefficient for 8 European species | **CC BY 4.0.** An erratum was published on 7 Oct 2014; I did not read it (see §8). |
| MIT20 | Mitchell, Hewison, Beaton, Haghi, Robertson, Main, Owen & Douglass 2020, *Functional and epiphytic biodiversity differences between nine tree species in the UK*, NERC EIDC, doi 10.5285/f539567f-a8cd-482e-89b8-64a951b52d93 | Measured furrow depth and width, ridge width, bark pattern on 234 trees | **Open Government Licence** (© NERC; © James Hutton Institute) |
| SHE13 | Sheridan, Puettmann, Huso, Hagar & Falk 2013, "Management, morphological, and environmental factors influencing Douglas-fir bark furrows in the Oregon Coast Range", *West. J. Appl. For.* 28(3):97–106 (author PDF, Oregon State Univ.) | Douglas-fir furrow depth, area, roughness | The article says it was prepared by a US Government employee and is **in the public domain** (p. 97). |
| MKC12 | Mehedi, Kundu & Chowdhury 2012, "Patterns of tree buttressing at Lawachara National Park, Bangladesh", *J. For. Res.* 23:461–466, doi 10.1007/s11676-012-0285-x | Buttress height and length ranges across 11 species | © Springer (subscription). **[ABSTRACT]** |
| WAN25 | Wang, X., Mwangi, B.N., Zhou, G., Yang, M., Li, Y. 2025, "An assessment of the role of buttress roots in the carbon stocks of tropical forests", *Front. Plant Sci.* 16:1538583, doi 10.3389/fpls.2025.1538583 (PMC12078330) | Buttress count per tree; buttress share of biomass; measurement definitions | **CC BY** |
| CUS14 | Cushman, Muller-Landau, Condit & Hubbell 2014, "Improving estimates of biomass change in buttressed trees using tree taper models", *Methods Ecol. Evol.* 5:573–582, doi 10.1111/2041-210X.12187 (ForestGEO PDF) | How trunks taper above buttresses (BCI, Panama) | **U.S. Government work, public domain in the USA** (running head of the ForestGEO PDF). |
| WHA84 | Wharton, E.H. 1984, *Predicting Diameter at Breast Height from Stump Diameters for Northeastern Tree Species*, USDA Forest Service Res. Note NE-322, doi 10.2737/NE-RN-322 | Regressions of DBH on stump diameter (a proxy for basal flare) | **Public domain** (scanned. **[OCR]**) |

**Restricted or conditional licences:**
- MGP15 is **share-alike** (CC BY-SA). Anything derived from its tables must carry the same licence.
- BPX code is **all rights reserved**. Only its numbers are used (secondary to the public-domain FOFEM 6.7 table).
- VicFlora **images** are mostly **CC BY-NC-SA 4.0** and are not used. VicFlora **text** is CC BY 4.0.
- MLA99, FNA, KPS09 and MKC12 are copyrighted. Only facts and numbers are taken from them. CUS14 is a U.S. Government work (public domain in the USA).
- No text or numbers used here come from a non-commercial (NC) source. I found one CC BY-NC paper (De Ridder et al. 2010 on *Terminalia superba*), but I did not use it (see §8).

## 3. Leaves by species

### 3a. Broadleaf blades (FNA, VIC)

Sizes are blade length × width. The size class is **[DERIVED]**: L × W × 2/3 over the usual range, set against the Webb classes in §3c.

| Species | Blade size | Shape | Margin and lobing | Venation (as described) | Petiole | Size class [DERIVED] | Source / URL |
|---|---|---|---|---|---|---|---|
| *Quercus alba* (white oak) | (79–)120–180(–230) × (40–)70–110(–165) mm | obovate to narrowly elliptic/obovate; base narrowly cuneate; apex broadly rounded | moderately to deeply lobed; sinuses run 1/3–7/8 of the way to the midrib | secondaries arched, divergent, (3–)5–7 per side | (4–)10–25(–30) mm | mesophyll (≈5,600–13,200 mm²) | FNA V3, http://floranorthamerica.org/Quercus_alba |
| *Quercus rubra* (northern red oak) | 120–200 × 60–120 mm | ovate to elliptic or obovate; base broadly cuneate to nearly truncate; apex acute | 7–11 lobes carrying 12–50 bristle tips (awns); sinuses usually less than 1/2 of the way to the midrib | secondaries raised on both faces | 25–50 mm | mesophyll (≈4,800–16,000 mm²) | FNA V3, http://floranorthamerica.org/Quercus_rubra |
| *Quercus robur* (pedunculate oak) | (50–)70–150(–200) × (20–)35–85(–100) mm | obovate to narrowly elliptic/obovate; base **strongly cordate** (earlobed); apex broadly rounded | moderately to deeply lobed; sinuses 1/3–7/8 of the way to the midrib; lobes rounded | secondaries arched, divergent, (3–)5–7 per side | **3–6 mm** (FNA key: under 10 mm, against over 10 mm in *Q. alba*) | microphyll–mesophyll (≈1,600–8,500 mm²) | FNA V3, http://floranorthamerica.org/Quercus_robur |
| *Fagus grandifolia* (American beech) | 60–120 × 25–75 mm | ovate or narrowly ovate, rarely obovate; base cuneate; apex acuminate | genus: each secondary vein ends in a sharp or faint tooth | genus: secondaries unbranched, ± parallel, running to the margin (= *craspedodromous* in MLA99 terms; my mapping) | 4–12 mm | microphyll–mesophyll (≈1,000–6,000 mm²) | FNA V3, http://floranorthamerica.org/Fagus_grandifolia ; genus: http://floranorthamerica.org/Fagus |
| *Fagus sylvatica* (European beech) | **Not found** in an open primary flora (see §8) | — | FNA: **crenate** margin without distinct teeth (separates it from *F. grandifolia*) | — | — | — | FNA V3, *F. grandifolia* discussion (URL above) |
| *Betula papyrifera* (paper birch) | 5–9(–12) × 4–7 cm | ovate; base rounded, cuneate or truncate; apex acute to short-acuminate | sharply to coarsely or irregularly **doubly serrate**, or serrate-dentate | **9 or fewer** pairs of lateral veins | — | microphyll–notophyll (≈1,300–4,200 mm²) | FNA V3, http://floranorthamerica.org/Betula_papyrifera |
| *Betula pendula* (silver birch) | 3–7 × 2.5–5 cm | broadly ovate to **rhombic**; base cuneate; apex acuminate | coarsely and sharply doubly serrate | 5–18 pairs of lateral veins | — | microphyll–notophyll (≈500–2,300 mm²) | FNA V3, http://floranorthamerica.org/Betula_pendula |
| *Acer saccharum* (sugar maple) | 7–15 × 9–18(–20) cm (wider than long is common) | simple, palmately lobed; base truncate to cordate; apex caudate-acuminate | entire, or sparsely and coarsely toothed or lobulate; 5 lobes, or 3 plus 2 basal lobes; sinuses less than 1/2 of the blade, rounded at the base | palmate (3–5 lobes; *actinodromous* in MLA99 terms; my mapping) | — | notophyll–mesophyll (≈4,200–18,000 mm²; upper bound, lobed) | FNA V13, http://floranorthamerica.org/Acer_saccharum |
| *Acer rubrum* (red maple) | 5–15 × 3–18 cm | 3–5-lobed, rarely almost unlobed; base cuneate to cordate; apex acute | shallowly crenate to singly or doubly serrate; sinuses less than 1/2 of the blade, acute to obtuse at the base | palmate | — | microphyll–mesophyll (≈1,000–18,000 mm²) | FNA V13, http://floranorthamerica.org/Acer_rubrum |
| *Acer pseudoplatanus* (sycamore) | 8–15 cm long and wide | palmately 3–5-lobed, lobes cut about halfway to the petiole; base cordate | irregularly toothed | palmate | 4–15 cm | notophyll–mesophyll (≈4,300–15,000 mm²) | VIC, https://vicflora.rbg.vic.gov.au/flora/taxon/f9f0b3aa-8980-44fb-b0b5-ce9af79b4e58 |
| *Eucalyptus globulus* (blue gum), adult | 15–30 × 1.7–3 cm | falcate or lanceolate; alternate; petiolate; both faces the same colour, glossy | entire (implied; not stated) | reticulation moderately dense; many oil glands, mostly at vein intersections | petiolate (length not given) | microphyll–mesophyll (≈1,700–6,000 mm²) | VIC, https://vicflora.rbg.vic.gov.au/flora/taxon/1dfdd922-7fa3-4fc4-bb6e-96cfa112337a |
| *E. globulus*, juvenile | to 15 × 10.5 cm | elliptic to ovate; sessile; opposite for many pairs on conspicuously winged stems; glaucous | — | — | sessile | up to mesophyll (≈10,500 mm² at maximum size) | VIC (same URL) |
| *Olea europaea* subsp. *europaea* (olive) | mostly 3–7 cm × 8–10 mm | narrow-oblong to elliptic; acute | **entire, rolled under (recurved)** | — (two-tone leaf: grey-green above, dense silver peltate scales below) | to about 10 mm | nanophyll–microphyll (≈160–470 mm²) | VIC, https://vicflora.rbg.vic.gov.au/flora/taxon/d89d5b58-21a1-4acc-9d0e-0502dd9b7b83 |

### 3b. Conifer needles (FNA, FEIS)

| Species | Needle length × width | Per fascicle / arrangement | Section and other traits | Source / URL |
|---|---|---|---|---|
| *Pinus sylvestris* var. *sylvestris* (Scots pine) | 4–6(–8) cm × 2 mm | **2 per fascicle**; sheath 0.3–0.6 cm | strongly twisted, somewhat flattened; stomatal lines on all faces; margin entire to finely serrulate; kept for 2–4 years; blue-green, grey-green or yellow-green | FNA V2, http://floranorthamerica.org/Pinus_sylvestris_var._sylvestris |
| *Picea abies* (Norway spruce) | 1–2.5 cm | single (spruces carry needles on peg-like bases; not stated on this page) | **4-angled** in cross-section; rigid; stomata on all faces; blunt tip | FNA V2, http://floranorthamerica.org/Picea_abies |
| *Pseudotsuga menziesii* (Douglas-fir) | 15–30(–40) × 1–1.5 mm | single | apex obtuse to acute; var. *menziesii* yellowish green; var. *glauca* bluish to dark or grey-green | FNA V2, http://floranorthamerica.org/Pseudotsuga_menziesii |
| *P. menziesii* var. *menziesii* (coast) | about 1 inch (2.5 cm) | spirally arranged on the branchlets | — | FEIS, https://www.fs.usda.gov/database/feis/plants/tree/psemenm/all.html |

### 3c. Reference scales

**Leaf blade size classes** (Webb 1955, as tabulated in MLA99 §17). Area approximation: L × W × 2/3. Source: https://personal.ems.psu.edu/~pwilf/1999_MLA.pdf

| Class | Blade area (mm²) |
|---|---|
| leptophyll | < 25 |
| nanophyll | 25–225 |
| microphyll | 225–2,025 |
| notophyll | 2,025–4,500 |
| mesophyll | 4,500–18,225 |
| macrophyll | 18,225–164,025 |
| megaphyll | > 164,025 |

**Vein categories used above** (MLA99 §§28–29, paraphrased):

| Term | Meaning |
|---|---|
| pinnate (1°) | a single primary vein |
| actinodromous (1°) | three or more primaries radiating from one point |
| craspedodromous (2°) | secondary veins end at the margin, usually in teeth |
| semicraspedodromous (2°) | secondaries fork just inside the margin; one branch reaches the margin and the other joins the next secondary |
| brochidodromous (2°) | secondaries join in a series of prominent loops |
| eucamptodromous (2°) | secondaries curve upward and fade inside the margin without forming loops |

MLA99 lists the margin types as crenate, dentate, entire, erose and serrate (field 26). It lists lobation as unlobed, bilobed, palmately lobed and pinnately lobed (field 27).

## 4. Bark type by species (descriptive)

Paraphrased from the floras. "Exfoliating" means the bark peels in sheets or strands.

| Species | Bark description | Source / URL |
|---|---|---|
| *Quercus alba* | light grey, scaly | FNA (URL §3a) |
| *Quercus rubra* | grey to dark grey; broad, shiny ridges with **shallow** fissures; inner bark pinkish | FNA |
| *Quercus robur* | light grey, scaly (FNA, for North American material) | FNA |
| *Fagus grandifolia* | grey, **smooth** | FNA |
| *Betula papyrifera* | young bark dark reddish-brown and smooth; mature bark creamy to chalky white (sometimes brown), **peeling in paper-thin sheets**; pale horizontal lenticels that darken and widen with age | FNA |
| *Betula pendula* | mature bark creamy to silvery white, smooth, **peeling in long strands**; dark, horizontally widened lenticels | FNA |
| *Pinus sylvestris* | ashy grey to brown and **furrowed**, with irregularly rectangular scaly ridges; **orange and platy on the upper trunk** | FNA V2 var. *sylvestris* |
| *Picea abies* | grey-brown, scaly | FNA V2 |
| *Pseudotsuga menziesii* var. *menziesii* | young: thin, smooth, grey, with resin blisters. Mature: **thick (4–12 in, 10–30 cm) and corky** | FEIS |
| *Eucalyptus globulus* | smooth, except for persistent slabs at the base; sheds in large strips and slabs; yellowish or grey | VIC |

**Eucalypt bark vocabulary** (VicFlora multi-access key, paraphrased; https://vicflora.rbg.vic.gov.au/pages/eucalypts):
- **Persistence:** wholly smooth, partly rough, or wholly rough.
- **Rough-bark types:**
  - ironbark: hard, deeply and widely furrowed, dark; only 3 Victorian taxa have it;
  - tessellated: short fibres breaking into plates;
  - stringy or fibrous: long fibres that pull off in strips;
  - box: short-fibred, with narrow longitudinal fissures; looks thin;
  - imperfectly shed ribbons or curls;
  - compacted;
  - loose basal slabs.
- **Other features:** nine smooth-bark colours; insect "scribbles"; ribbons of bark in the upper branches.

## 5. Bark thickness

### 5a. FOFEM single-bark-thickness multiplier (BT = v × DBH, any consistent unit)

Main source: FOFEM67, p. 58 (https://research.fs.usda.gov/sites/default/files/2023-03/rmrs-fofem_6-7_user_guide.pdf). That table gives *v* by bark-thickness index (1–39). Species-to-index mapping is the FOFEM `barkEq` used in BPX (secondary; all rights reserved): Douglas-fir, ponderosa pine and western larch are index 36. The BT values at 30 cm and 60 cm DBH are **[DERIVED]** (v × DBH).

FOFEM 5.0 (`xfblib.cpp`, `Fofem_Sbt[36]`) has **0.065** for the same index. FOFEM 6.7 p. 58 has **0.063**, and the FFE / RMRS-GTR-116 addendum lists Douglas-fir and ponderosa pine as 0.063. The table uses the official 6.7 value. See §9.

| Species | FOFEM code | v | BT at 30 cm DBH | BT at 60 cm DBH |
|---|---|---|---|---|
| Douglas-fir (*Pseudotsuga menziesii*) | PSEMEN | 0.063 | 1.89 cm | 3.78 cm |
| Ponderosa pine (*Pinus ponderosa*) | PINPON | 0.063 | 1.89 | 3.78 |
| Western larch (*Larix occidentalis*) | LAROCC | 0.063 | 1.89 | 3.78 |
| Sugar pine (*P. lambertiana*) | PINLAM | 0.072 | 2.16 | 4.32 |
| Giant sequoia / redwood | SEQGIG / SEQSEM | 0.081 | 2.43 | 4.86 |
| Loblolly pine (*P. taeda*) | PINTAE | 0.052 | 1.56 | 3.12 |
| Eastern white pine (*P. strobus*) | PINSTR | 0.045 | 1.35 | 2.70 |
| Northern red oak (*Q. rubra*) | QUERUB | 0.042 | 1.26 | 2.52 |
| White oak (*Q. alba*) | QUEALB | 0.040 | 1.20 | 2.40 |
| Eastern hemlock (*Tsuga canadensis*) | TSUCAN | 0.039 | 1.17 | 2.34 |
| Red spruce (*Picea rubens*) | PICRUB | 0.034 | 1.02 | 2.04 |
| Sugar maple (*Acer saccharum*) | ACESACU | 0.033 | 0.99 | 1.98 |
| Balsam fir (*Abies balsamea*) | ABIBAL | 0.031 | 0.93 | 1.86 |
| Scots pine (*Pinus sylvestris*) | PINSYL | 0.030 | 0.90 | 1.80 |
| Norway spruce (*Picea abies*) | PICABI | 0.029 | 0.87 | 1.74 |
| Red maple (*Acer rubrum*) | ACERUB | 0.028 | 0.84 | 1.68 |
| Paper birch (*Betula papyrifera*) | BETPAP | 0.027 | 0.81 | 1.62 |
| American beech (*Fagus grandifolia*) | FAGGRA | 0.025 | 0.75 | 1.50 |

Other values in the same arrays:

| Species | v |
|---|---|
| yellow birch | 0.031 |
| white spruce | 0.025 |
| western redcedar | 0.035 |
| western hemlock | 0.040 |
| Engelmann spruce | 0.036 |
| lodgepole pine | 0.028 |
| white fir | 0.048 |
| black oak | 0.045 |
| chestnut oak | 0.049 |
| coast live oak | 0.050 |
| Pacific madrone | 0.060 |

**Cross-check against FOFEM4 Table 1** (inches; Keane et al. 1989, 1996; **[OCR]**), https://www.fs.usda.gov/rm/pubs_int/int_gtr344.pdf (also at https://research.fs.usda.gov/download/treesearch/25257.pdf):

| Species | FOFEM4 equation |
|---|---|
| Douglas-fir | BTH = 0.065 · DBH, which **matches FOFEM 5** (`Fofem_Sbt[36]`) and is **thicker than FOFEM 6.7** (v = 0.063) |
| Ponderosa pine | −0.0376 + 0.0584 · DBH |
| Western larch | −0.045 + 0.0629 · DBH |
| Grand fir | 0.043 · DBH |
| Subalpine fir | 0.015 · DBH |
| Lodgepole pine | 0.027 + 0.0143 · DBH |
| Engelmann spruce | 0.126 + 0.025 · DBH |
| Quaking aspen | 0.052 + 0.033 · DBH |
| Western white pine | 0.054 + 0.025 · DBH |

The western redcedar intercept prints as ".I52", read as 0.152, which is uncertain.

### 5b. FVS Northeast bark ratios (DIB = BRATIO × DOB)

Source: FVSNE §4.2, Table 4.2.1, with species groups from Table 3.2.1. https://www.fs.usda.gov/sites/default/files/forest-management/fvs-ne-overview.pdf

The ratios come from Hilt (1985) for the hardwoods, Martin (1981) for yellow birch (also used for paper birch), and Solomon's 1977 Maine and New Hampshire data for balsam fir and red spruce. The other species use surrogate values.

Single bark thickness = DOB × (1 − BRATIO)/2. The thickness columns are **[DERIVED]**.

| Species (FVS group) | BRATIO | Single bark at 30 cm DOB | Single bark at 60 cm DOB |
|---|---|---|---|
| balsam fir (1) | 0.9349 | 0.98 cm | 1.95 cm |
| red spruce, Norway spruce (4) | 0.9324 | 1.01 | 2.03 |
| white spruce (3) | 0.956 | 0.66 | 1.32 |
| red pine (5); eastern white pine (6) | 0.920 | 1.20 | 2.40 |
| eastern hemlock (10) | 0.934 | 0.99 | 1.98 |
| Scots pine (11) | 0.964 | 0.54 | 1.08 |
| red maple (12) | 0.950 | 0.75 | 1.50 |
| sugar maple (13) | 0.920 | 1.20 | 2.40 |
| yellow birch (14), paper birch (15) | 0.948 | 0.78 | 1.56 |
| American beech (17) | 0.950 | 0.75 | 1.50 |
| white ash (18), quaking aspen (20), northern red oak (25) | 0.900 | 1.50 | 3.00 |
| black cherry (21) | 0.940 | 0.90 | 1.80 |
| white oak (22) | 0.910 | 1.35 | 2.70 |
| swamp white oak (24) | 0.880 | 1.80 | 3.60 |

### 5c. Measured bark thickness (Europe, *Eucalyptus*)

| Species | Measure | Value (spread) | n / context | Source / URL |
|---|---|---|---|---|
| *Pinus sylvestris* | double bark thickness at 1.3 m | mean **2.05 cm** (min 0.60, max 4.90; SD 1.14; CV 55.53 %) | 40 trees; mean DBH 20.59 cm (10.11–31.30); mean age 55 years; western Poland | KPS09 Table 2, https://bibliotekanauki.pl/articles/9695.pdf |
| *Larix decidua* | double bark thickness at 1.3 m | mean **2.01 cm** (0.92–4.50; SD 0.82; CV 40.71 %) | 24 trees; mean DBH 26.35 cm (8.35–49.35) | KPS09 |
| *Quercus robur* | double bark thickness at 1.3 m | mean **1.87 cm** (0.90–3.40; SD 0.63; CV 33.59 %) | 33 trees; mean DBH 31.48 cm (14.55–56.85); mean age 92 years | KPS09 |
| (same three) | regression on DBH outside bark (cm) | oak k = 0.0452 d + 0.4487; larch k = 0.0578 d + 0.4847; pine k = 0.1362 d − 0.7506 | — | KPS09 eq. 3.1–3.3 |
| *Fagus sylvatica* | relative double bark thickness (% of diameter over bark) | **2.30–6.45 %** (lowest of the three species) | Romanian national data, 2,389 trees (DBH 16–68 cm) | VAS24 §3, https://www.mdpi.com/1999-4907/15/12/2264 |
| *Picea abies* | relative double bark thickness | **3.51–20.00 %** | 5,403 trees (DBH 12–60 cm) | VAS24 |
| *Quercus robur* | relative double bark thickness | **5.71–26.92 %** (highest) | 4,394 trees (DBH 8–70 cm) | VAS24 |
| *Eucalyptus globulus* | bark thickness at 1.3 m | coppice (2nd rotation), by spacing: **8.5 ± 2.6, 7.7 ± 1.5, 8.6 ± 2.0, 7.9 ± 2.6 and 8.6 ± 1.6 mm**; single-stem (1st rotation): **6.6 ± 2.8 to 10.3 ± 2.5 mm** | 18-year-old trees, Portugal; 5 spacings (4×5, 4×4, 4×3, 3×3, 3×2) | MGP15 Table 3, https://silvafennica.fi/article/1141 |
| *E. globulus* | along the stem | 12.1 mm at the base, falling to 3.4 mm at 15.3 m; fit y = 10.987 X^−0.584 (R² 0.914; X = height level) | coppiced trees | MGP15 §3.3 |
| *P. menziesii* var. *menziesii* | mature bark thickness | 10–30 cm (4–12 in) | descriptive | FEIS (URL §4) |

## 6. Bark surface: furrows and roughness

### 6a. UK nine-species dataset (MIT20)

The values are **[DERIVED]**. I took the mean of the 8 measurements (mm) in each 30 × 30 cm quadrat; each tree has 4 quadrats at 0.5–1.7 m height. The table gives the median, IQR and range of those quadrat means across all quadrats. Furrow measurements were taken only where the bark pattern was scored "fissured", so n counts fissured quadrats.

DBH is the dataset's `DBH` column, computed from girth (cm). Sampling was 2018, at six UK sites. Data: https://doi.org/10.5285/f539567f-a8cd-482e-89b8-64a951b52d93 (download: https://data-package.ceh.ac.uk/data/f539567f-a8cd-482e-89b8-64a951b52d93.zip).

| Species | Trees (DBH median, cm) | Bark pattern (quadrats) | Furrow depth, mm: median (IQR; range) | Furrow width, mm: median (IQR) | Ridge width, mm: median (IQR) |
|---|---|---|---|---|---|
| *Quercus robur* | 34 (124) | fissured 122/136 | **18.8** (13.6–27.1; 4.6–44.6), n = 126 | 16.6 (11.7–24.8) | 17.3 (14.4–21.6) |
| *Quercus petraea* | 13 (118) | fissured 49/52 | **18.2** (12.2–28.2; 5.4–39.0), n = 49 | 14.8 (9.8–20.7) | 15.5 (12.9–19.1) |
| *Quercus cerris* | 14 (118.5) | fissured 56/56 | **23.7** (18.0–30.2; 11.6–41.9), n = 56 | 26.8 (19.6–35.3) | 25.8 (21.5–30.8) |
| *Quercus rubra* | 15 (89) | fissured 53/60; smooth 14 | **7.0** (4.5–10.2; 2.6–16.9), n = 53 | 12.4 (7.6–19.6) | 21.5 (17.4–26.1) |
| *Castanea sativa* | 24 (141) | fissured 92/96 | **19.6** (12.0–24.4; 3.1–35.2), n = 92 | 18.2 (12.1–27.8) | 19.9 (15.9–22.7) |
| *Fraxinus excelsior* | 32 (102.5) | fissured 122/128 | **17.8** (13.4–22.2; 6.9–34.0), n = 120 | 19.2 (13.9–24.6) | 15.4 (12.8–18.5) |
| *Tilia × europaea* | 31 (103) | fissured 110/124 | **11.1** (7.6–15.1; 2.6–39.6), n = 108 | 12.4 (7.8–16.5) | 17.8 (15.3–20.6) |
| *Fagus sylvatica* | 35 (124) | **smooth 136/140** | one fissured quadrat only (17.5) | — | — |
| *Acer pseudoplatanus* | 32 (97) | smooth 119 and flaky 122 of 128 (scored as non-exclusive) | not measured (no fissured quadrats) | — | — |

The dataset also has 2 *Acer platanoides* and 2 *Liriodendron tulipifera* trees; there are too few to summarise.

### 6b. Douglas-fir furrows, Oregon Coast Range (SHE13 Table 5)

Values are means with 95 % CIs. Furrow depth is that of the single largest furrow per tree, measured with a contour gauge. Source: https://www.cof.orst.edu/cof/fs/kpuettmann/WJAF%2028%202013.pdf

| Stand | DBH (cm) | Furrow depth (cm) | Furrow area (cm²) | Bark roughness (unitless) |
|---|---|---|---|---|
| Young, unthinned (62–73 yr) | 52.97 (51.33–54.61) | **1.87** (1.52–2.3) | 6.54 (4.78–8.96) | 50.52 (36.56–69.8) |
| Young, thinned | 55.58 (53.9–57.26) | **2.05** (1.66–2.52) | 7.49 (5.46–10.27) | 64.37 (46.48–89.15) |
| Old (> 195 yr) | 102.6 (93.27–111.93) | **6.79** (5.09–8.49) | 57.37 (37.22–77.52) | 136.19 (117.93–154.45) |

Bark roughness = 1,000 × (C_a − C_t)/C_t, where C_a is the circumference traced into the furrows with wire and C_t is the tape circumference.

### 6c. Bark surface development coefficient C_sd (IK14)

C_sd = actual bark surface ÷ smooth-cylinder surface, so 1.0 is perfectly smooth. Values are at breast height on the thickest sampled tree of each species (77 samples, southern Poland). Source: https://link.springer.com/article/10.1007/s00468-014-0995-0

| Species | C_sd (DBH) |
|---|---|
| *Larix decidua* | 2.56 (58 cm) |
| *Betula pendula* | 2.44 (52 cm) |
| *Pinus sylvestris* | 2.28 (56 cm) |
| *Picea abies* | 1.75 (58 cm) |
| *Quercus robur* | 1.72 (56 cm) |
| *Abies alba* | 1.45 (59 cm) |
| *Fagus sylvatica* | **1.07** (59 cm); varies little with age |
| *Acer pseudoplatanus* | close to 1.00 when young; the surface starts to break up at about 37 cm DBH (no data above 37 cm) |

C_sd rises with DBH in every species except beech.

## 7. Root flare and buttresses

### 7a. Temperate trees: stump diameter against DBH (WHA84, NE USA)

Equation: DBH (in) = a + b × stump diameter inside bark (in), at roughly a 1-ft (30 cm) stump. Source: https://research.fs.usda.gov/download/treesearch/21612.pdf (**[OCR]**)

| Group | a | b | n | r² | Mean stump diameter (in) | Source table |
|---|---|---|---|---|---|---|
| Spruce and fir | 0.96596 | 0.79667 | 337 | 0.863 | 11.8 | Table 2; also the worked example in the text |
| All softwoods | 0.03796 | 0.91308 | 2,373 | 0.939 | 12.5 | Table 3 |
| All hardwoods | 1.08806 | 0.83487 | 4,680 | 0.902 | 17.0 | Table 3 |

- Worked example in the source: a 14.0-in spruce stump gives a predicted DBH of 12.1 in, with a ±2 SD range of 10.1–14.1 in.
- Stump diameters in the data ranged from 2.1 to 48.5 in, from more than 7,000 trees in 14 states (1969–1979).
- **[DERIVED] flare ratio:** inverting the group equations at DBH 12 in (30.5 cm) gives a stump of about 13.1 in for both softwoods and hardwoods. That is a stump/DBH ratio of about 1.09 at about 1 ft, measured inside bark. Using the spruce–fir equation gives 13.8 in (ratio ≈ 1.15).
- The species rows for the other 15 groups are in Table 2, but the scan's text layer is garbled, so I did not use them (see §8).

### 7b. Tropical buttresses

| Item | Value | Context | Source / URL |
|---|---|---|---|
| Share of trees buttressed | 45 % of trees across 11 species; *Artocarpus chaplasha* 87 %; *Alstonia scholaris* 0 % | Lawachara NP, Bangladesh | MKC12 **[ABSTRACT]**, https://link.springer.com/article/10.1007/s11676-012-0285-x |
| Mean buttress height (species means) | **0.71–2.13 m** | 11 species | MKC12 [ABSTRACT] |
| Mean buttress length (species means) | **0.37–1.37 m** | 11 species | MKC12 [ABSTRACT] |
| What drives buttress size | Height and length grow with DBH and tree height. Buttress number does not. Emergent trees have larger buttresses than understorey and mid-canopy trees (p < 0.01); slope had no effect. | — | MKC12 [ABSTRACT] |
| Buttresses per tree | 1–7; **69.57 % of trees have 3–5** | 500 buttressed trees (buttress length > 20 cm), mean DBH 28.5 ± 13.3 cm; Diaoluo Mountain, Hainan | WAN25, https://www.ncbi.nlm.nih.gov/pmc/articles/PMC12078330/ |
| Buttress-root biomass | 8.5 t/ha in buttresses against 44.04 t/ha above ground for buttressed trees; buttresses = **16.18 %** of total tree biomass | same | WAN25 |
| How dimensions are defined | **Height:** ground to where the buttress merges with the stem. **Length:** horizontal distance at ground level from the stem to the buttress's farthest point. **Widths:** measured near the trunk and at the far end. | same | WAN25 |
| Trunk taper above buttresses | Taper is best fit by an exponential model whose rate depends on measured diameter, measurement height (buttress height) and species. At BCI, more than half of forest biomass is in trees measured above 1.3 m. Correcting for taper turns a −0.21 %/yr biomass trend (1985–2010) into +0.18 %/yr. | 190 stems, Barro Colorado Island, Panama | CUS14, https://si-forestgeo01.si.edu/Public/pdfs/CushmanConditEtAl_2014_MEE.pdf |

## 8. Not found

- ***Fagus sylvatica* leaf dimensions and vein-pair count:**
  - Flora iberica's *Fagus* PDF returned HTTP 500 to curl and 502 to WebFetch.
  - Flora of New Zealand gives no numbers.
  - FNA gives only "crenate margin".
  - A search summary gave 5–10 cm and 5–10 vein pairs, but I could not open the primary source, so it is not tabulated.
- **Leaf data for spruce/Douglas-fir needle cross-sections beyond FNA, and for *Betula alba* / *Fagus crenata* / *Acer rufinerve*** (E1 species): not looked up in time. *Picea mariana* and *Picea glauca* are in FNA V2 if needed.
- **Furrow depth for conifers other than Douglas-fir, for *Betula*, and for *Eucalyptus* stringybarks/ironbarks:** no measured mm values found. The VicFlora key is descriptive only.
- **Bark thickness for *Olea europaea*:** not found.
- **Primary-source buttress dimensions by named species:**
  - He, Tang, Deng & Cao 2013, *J. Plant Ecol.* 6:187–192 (20-ha Xishuangbanna plot): OUP returned 403 / timed out; abstract elided in Semantic Scholar.
  - Warren et al. 1988, *Ecology* 69:532 (*Tachigalia*): closed access.
- **De Ridder et al. 2010, *Ann. For. Sci.* 67:501 (*Terminalia superba*):**
  - The search index snippet quotes a mean buttress height of 202.5 ± 69.2 cm (n = 2,365) and H_buttress = 96.16·e^(0.012·DAB).
  - The Springer page shows only the abstract and references, and the PDF is behind a bot check. **Not checked against the primary text.**
  - Licence: **CC BY-NC 2.0 (non-commercial).**
- ***Ceiba pentandra* buttress dimensions** (E1 species): not found.
- **Root flare of urban trees** (trunk-flare diameter at ground line against DBH):
  - Hilbert et al. 2020, *Urban For. Urban Green.* 49:126645;
  - Vendruscolo et al. 2026, *J. Sustain. For.*, doi 10.1080/10549811.2026.2699694;
  - UMN thesis hdl 11299/162376.
  
  None had an abstract in Crossref, and the pages returned 403 or timed out. A search snippet gave a mean TFD/DBH ratio of 2.36 (1.51–3.66); this is **unverified and not tabulated**.
- **WHA84 species rows other than spruce–fir:** the OCR of Table 2 is garbled. A search summary listed red maple a = 0.14487, b = 0.93115, but the scan's text reads differently ("−.46354 ,931"). Sugar maple 0.80040 / 0.88053, beech 0.90346 / 0.83463 and white pine 0.31469 / 0.89376 appear in the OCR, but I can't tie each with certainty to its row label. **Not tabulated.**
- **IK14 erratum (7 Oct 2014):** not read. The C_sd values could be affected.
- **Silvics Manual** (srs.fs.usda.gov): unreachable from the box (TLS error), as in E4.

## 9. Sources disagree

| Quantity | Source A | Source B | Note |
|---|---|---|---|
| FOFEM bark index 36 (Douglas-fir, ponderosa pine, western larch) | FOFEM 6.7 User Guide p. 58, and FFE / RMRS-GTR-116 addendum: v = **0.063** | FOFEM 5.0 (`xfblib.cpp` `Fofem_Sbt[36]`): v = **0.065** | Same index scheme; only this coefficient differs. The table in §5a uses 0.063. FOFEM 4 also uses 0.065 · DBH for Douglas-fir. |
| Scots pine bark at about 20 cm DBH | KPS09 measured double bark 2.05 cm at mean DBH 20.6 cm, so single ≈ 1.0 cm | FOFEM 5 / 6.7 v = 0.030 gives 0.62 cm; FVS NE ratio 0.964 gives 0.37 cm | The measured Polish pines have 1.7–2.8 × the bark the US models assume. KPS09's pine regression slope (0.136) is also about 4.5 × FOFEM's. |
| Ponderosa pine and western larch | FOFEM4 (1997): BTH = −0.0376 + 0.0584 · DBH (PP); −0.045 + 0.0629 · DBH (WL) | FOFEM 5 (BPX): 0.065 · DBH for both; FOFEM 6.7: 0.063 · DBH | At 10–20 in DBH the old intercept form gives about 13–16 % (PP) and 7–14 % (WL) thinner bark than FOFEM 5 [DERIVED]. |
| Lodgepole pine, Engelmann spruce | FOFEM4: 0.027 + 0.0143 · DBH; 0.126 + 0.025 · DBH | FOFEM 5 and 6.7: 0.028 · DBH; 0.036 · DBH | Large disagreement for lodgepole at large DBH (e.g. 20 in: 0.31 against 0.56 in). FOFEM4 numbers are [OCR]. |
| Norway spruce relative bark | VAS24: double bark 3.51–20.00 % of diameter (varies with diameter and height) | FVS NE group-4 ratio 0.9324, i.e. 6.76 % constant | FVS uses a single constant; the Romanian data vary strongly with stem position. |
| Bark of *Quercus robur* | FNA: "light grey, scaly" (North American cultivated and naturalised trees) | MIT20: fissured in 122 of 136 UK quadrats, median furrow depth 18.8 mm | The difference is in the descriptors, not the data. Old European *Q. robur* is clearly furrowed. Use the measured data for rendering. |
| Douglas-fir needle length | FNA: 15–30(–40) mm | FEIS: about 1 inch (25 mm) | Consistent; FEIS gives a single typical value. |

## 10. References (all accessed 2026-11-16)

1. Flora of North America Editorial Committee (eds). *Flora of North America North of Mexico*, Vols 2 (1993), 3 (1997), 13. Pages:
   - http://floranorthamerica.org/Quercus_alba
   - http://floranorthamerica.org/Quercus_rubra
   - http://floranorthamerica.org/Quercus_robur
   - http://floranorthamerica.org/Fagus_grandifolia
   - http://floranorthamerica.org/Fagus
   - http://floranorthamerica.org/Betula_papyrifera
   - http://floranorthamerica.org/Betula_pendula
   - http://floranorthamerica.org/Acer_saccharum
   - http://floranorthamerica.org/Acer_rubrum
   - http://floranorthamerica.org/Pinus_sylvestris_var._sylvestris
   - http://floranorthamerica.org/Picea_abies
   - http://floranorthamerica.org/Pseudotsuga_menziesii
   - http://floranorthamerica.org/Pseudotsuga_menziesii_var._menziesii
2. VicFlora, Royal Botanic Gardens Victoria. *Eucalyptus globulus* https://vicflora.rbg.vic.gov.au/flora/taxon/1dfdd922-7fa3-4fc4-bb6e-96cfa112337a ; *Olea europaea* subsp. *europaea* https://vicflora.rbg.vic.gov.au/flora/taxon/d89d5b58-21a1-4acc-9d0e-0502dd9b7b83 ; *Acer pseudoplatanus* https://vicflora.rbg.vic.gov.au/flora/taxon/f9f0b3aa-8980-44fb-b0b5-ce9af79b4e58 ; Eucalypt key character descriptions https://vicflora.rbg.vic.gov.au/pages/eucalypts ; Copyright and reproduction https://vicflora.rbg.vic.gov.au/pages/copyright
3. Leaf Architecture Working Group. 1999. *Manual of Leaf Architecture: morphological description and categorization of dicotyledonous and net-veined monocotyledonous angiosperms.* Smithsonian Institution, Washington DC. 65 p. https://personal.ems.psu.edu/~pwilf/1999_MLA.pdf
4. Uchytil, R.J. 1991. *Pseudotsuga menziesii* var. *menziesii*. Fire Effects Information System, USDA Forest Service. https://www.fs.usda.gov/database/feis/plants/tree/psemenm/all.html
5. Reinhardt, E.D., Keane, R.E. & Brown, J.K. 1997. *First Order Fire Effects Model: FOFEM 4.0, User's Guide.* Gen. Tech. Rep. INT-GTR-344. USDA Forest Service. https://www.fs.usda.gov/rm/pubs_int/int_gtr344.pdf ; https://research.fs.usda.gov/download/treesearch/25257.pdf
6. Lutes, D.C. 2020. *FOFEM 6.7 First Order Fire Effects Model: User Guide.* USDA Forest Service, February 2020. p. 58. https://research.fs.usda.gov/sites/default/files/2023-03/rmrs-fofem_6-7_user_guide.pdf
7. Bevins, C.D. 2002–2011. `xfblib.cpp`, BehavePlus 5 fire behavior library (FOFEM 5.0 bark equations). https://www.frames.gov/documents/behaveplus/software/xfblib.cpp
8. USDA Forest Service. *Forest Vegetation Simulator: Fire and Fuels Extension, Addendum to RMRS-GTR-116.* https://www.fs.usda.gov/rm/pubs_series/rmrs/gtr/rmrs_gtr116_addendum.pdf
9. USDA Forest Service. *Northeast (NE) Variant Overview, Forest Vegetation Simulator.* Forest Management Service Center. https://www.fs.usda.gov/sites/default/files/forest-management/fvs-ne-overview.pdf
10. Kaźmierczak, K., Pazdrowski, W., Szymański, M. & Nawrot, M. 2009. "Statistical characteristics of double bark thickness at breast height based on pine (*Pinus sylvestris* L.), larch (*Larix decidua* Mill.) and oak (*Quercus robur* L.)." *Colloquium Biometricum* 39:157–168. https://bibliotekanauki.pl/articles/9695.pdf
11. Vasilescu, M.M., Cieszewski, C., Ferraz Filho, A.C. & De Araújo, E.J.G. 2024. "Bark Biometry Along the Stem for Three Commercial Tree Species in Romania." *Forests* 15(12):2264. https://www.mdpi.com/1999-4907/15/12/2264
12. Miranda, I., Gominho, J. & Pereira, H. 2015. "Heartwood, sapwood and bark variation in coppiced *Eucalyptus globulus* trees in 2nd rotation and comparison with the single-stem 1st rotation." *Silva Fennica* 49(1):1141. https://doi.org/10.14214/sf.1141 ; https://silvafennica.fi/article/1141
13. Ilek, A. & Kucza, J. 2014. "Hydrological properties of bark of selected forest tree species. Part I: the coefficient of development of the interception surface of bark." *Trees* 28:831–839. https://doi.org/10.1007/s00468-014-0995-0
14. Mitchell, R.J., Hewison, R.L., Beaton, J., Haghi, R.K., Robertson, A.H.J., Main, A.M., Owen, I.J. & Douglass, J. 2020. *Functional and epiphytic biodiversity differences between nine tree species in the UK.* NERC Environmental Information Data Centre. https://doi.org/10.5285/f539567f-a8cd-482e-89b8-64a951b52d93 ; record https://catalogue.ceh.ac.uk/documents/f539567f-a8cd-482e-89b8-64a951b52d93
15. Sheridan, C.D., Puettmann, K.J., Huso, M.M.P., Hagar, J.C. & Falk, K.R. 2013. "Management, morphological, and environmental factors influencing Douglas-fir bark furrows in the Oregon Coast Range." *Western Journal of Applied Forestry* 28(3):97–106. https://www.cof.orst.edu/cof/fs/kpuettmann/WJAF%2028%202013.pdf ; https://academic.oup.com/wjaf/article/28/3/97/4683604
16. Mehedi, M.A.H., Kundu, C. & Chowdhury, M.Q. 2012. "Patterns of tree buttressing at Lawachara National Park, Bangladesh." *Journal of Forestry Research* 23:461–466. https://link.springer.com/article/10.1007/s11676-012-0285-x
17. Wang, X., Mwangi, B.N., Zhou, G., Yang, M. & Li, Y. 2025. "An assessment of the role of buttress roots in the carbon stocks of tropical forests." *Frontiers in Plant Science* 16:1538583. https://doi.org/10.3389/fpls.2025.1538583 ; PMC12078330
18. Cushman, K.C., Muller-Landau, H.C., Condit, R.S. & Hubbell, S.P. 2014. "Improving estimates of biomass change in buttressed trees using tree taper models." *Methods in Ecology and Evolution* 5:573–582. https://doi.org/10.1111/2041-210X.12187 ; public-domain ForestGEO PDF: https://si-forestgeo01.si.edu/Public/pdfs/CushmanConditEtAl_2014_MEE.pdf
19. Wharton, E.H. 1984. *Predicting Diameter at Breast Height from Stump Diameters for Northeastern Tree Species.* Res. Note NE-322. USDA Forest Service, Northeastern Forest Experiment Station. https://doi.org/10.2737/NE-RN-322 ; https://research.fs.usda.gov/download/treesearch/21612.pdf
20. *(Not used; listed for §8.)* De Ridder, M., Hubau, W., Van Den Bulcke, J., Van Acker, J. & Beeckman, H. 2010. *Ann. For. Sci.* 67:501. https://doi.org/10.1051/forest/2010003 (CC BY-NC 2.0)
21. *(Not used; listed for §8.)* He, Z., Tang, Y., Deng, X. & Cao, M. 2013. "Buttress trees in a 20-hectare tropical dipterocarp rainforest in Xishuangbanna, SW China." *J. Plant Ecol.* 6:187–192. https://doi.org/10.1093/jpe/rts031
