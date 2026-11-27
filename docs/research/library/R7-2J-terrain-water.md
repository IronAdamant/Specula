# R7-2J Terrain and water: ground reflectance, erosion algorithms, water colour, waterfalls, bed forms, lake shores (Q44 to Q49)

Round 7 library research. **Access date for every source: 2026-11-24** unless stated. Values are as printed unless marked **computed**. Much of this section is **not found**: two of the three erosion papers sit behind a bot-check page I did not try to get past, the Jerlov Kd tables are in a paywalled paper, and the US Army Corps of Engineers manual answered HTTP 403.

## Sources used (licence and access)

| Key | Citation | Licence / access | Read |
|---|---|---|---|
| J1 | Kokaly RF, Clark RN, Swayze GA, Livo KE, Hoefen TM, Pearson NC, Wise RA, Benzel WM, Lowers HA, Driscoll RL, Klein AJ (2017). *USGS Spectral Library Version 7*. U.S. Geological Survey Data Series 1035, 61 p. doi:10.3133/ds1035. Data: splib07a ASCII files (data release doi:10.5066/F7RR1WDJ) | **US Government work, public domain**; dataset named, not copied | individual spectra (local copy of ASCIIdata_splib07a.zip, downloaded 2026-11-23); point values and error bars **computed** by me |
| J2 | Brock BW, Willis IC, Sharp MJ (2000). Measurement and parameterization of albedo variations at Haut Glacier d'Arolla, Switzerland. *Journal of Glaciology* 46(155):675-688. doi:10.3189/172756500781832675 | Free to read on Cambridge Core; licence shown by Crossref as Cambridge Core terms (not an open licence) | full text as served by Cambridge Core (tables are images and were not readable; text values only) |
| J3 | Simons DB, Richardson EV (1966). *Resistance to Flow in Alluvial Channels*. U.S. Geological Survey Professional Paper 422-J, 61 p. doi:10.3133/pp422J | **US Government work, public domain** | full PDF (scanned with a text layer), glossary, summary, "Ripples", antidune section |

## Q44. Colour and roughness of rock, soil, sand and snow (Improves a plan)

### Laboratory reflectance at 450, 550, 650 nm (J1)

Reflectance factor (directional-hemispherical "AREF", not field albedo) of named splib07a samples. Each value is the mean of the library spectrum within ±5 nm of the stated wavelength, and VIS is the mean over 400 to 700 nm (**computed** from J1). Spectrometers: ASD FieldSpec (ASDFR) or Beckman (BECK), as in the file name. Where splib07a ships an error-bar file, its value (the library's uncertainty for that spectrum) is shown as ±.

| Q | Material (splib07a record) | 450 nm | 550 nm | 650 nm | VIS 400-700 | Dry / wet |
|---|---|---|---|---|---|---|
| Q44 | melting snow, mSnw01a (rec. 13518) | 0.833 | 0.832 | 0.822 | 0.829 | wet (melting) |
| Q44 | melting snow, mSnw08 (rec. 13542) | 0.597 | 0.614 | 0.607 | 0.605 | wet (melting) |
| Q44 | sand, Grand Isle, GrndIsle1 no oil (rec. 13249) | 0.181 | 0.244 | 0.271 | 0.230 | dry |
| Q44 | sand DWO-3-DEL2ar1, no oil (rec. 13218) | 0.156 ± 0.007 | 0.218 ± 0.009 | 0.254 ± 0.010 | 0.209 | dry |
| Q44 | same sand wetted, DEL2ar2 wet (rec. 13234) | 0.090 ± 0.002 | 0.115 ± 0.002 | 0.129 ± 0.003 | 0.111 | wet |
| Q44 | limestone CU02-11A (rec. 12770) | 0.158 | 0.191 | 0.214 | 0.186 | dry |
| Q44 | basalt, fresh, BR93-46B (rec. 11848) | 0.077 ± 0.001 | 0.091 ± 0.001 | 0.107 ± 0.001 | 0.092 | dry |
| Q44 | basalt, weathered, BR93-43 (rec. 11856) | 0.102 ± 0.001 | 0.128 ± 0.001 | 0.145 ± 0.001 | 0.126 | dry |
| Q44 | chlorite phyllite CU91-238A (rec. 12212), nearest splib07a rock to slate | 0.095 | 0.124 | 0.148 | 0.124 | dry |
| Q44 | goethite phyllite CU91-236A (rec. 12468), iron-stained | 0.184 | 0.304 | 0.400 | 0.298 | dry |
| Q44 | playa clay mud, Stonewall Playa dry mud 2001 (rec. 13304) | 0.239 | 0.360 | 0.478 | 0.359 | dry |
| Q44 | Stonewall Playa CU93-52A (rec. 13302) | 0.222 | 0.345 | 0.454 | 0.343 | dry |
| Q44 | burn area ash surface WRF00-02 (rec. 11928) | 0.025 | 0.028 | 0.031 | 0.028 | dry |
| Q44 | for comparison: concrete road GDS375 (rec. 18568) / old asphalt road GDS376 (rec. 18269) | 0.224 / 0.069 | 0.286 / 0.085 | 0.314 / 0.103 | 0.273 / 0.086 | dry |

Wetting the DWO sand drops its visible reflectance from 0.209 to 0.111 (a factor of about 0.53, **computed**). The H2O-ice spectrum in splib07a (GDS136, 77 K) has no data in the visible range, so glacier ice is not covered by J1.

### Field albedo of snow and glacier ice (J2)

Broadband albedo (0.3 to 2.8 µm), Kipp and Zonen CM7B albedometer, cloud-free, within 3 h of solar noon (pyranometer accuracy ±2%), repeated surveys over the 1993 and 1994 ablation seasons on Haut Glacier d'Arolla, Switzerland (J2, Methods).

| Q | Surface | Value (as stated in the J2 text) | Source |
|---|---|---|---|
| Q44 | snow, late May | about 0.80 | J2 Results, item 5 |
| Q44 | melting snow with impurities, July and August | 0.40 to 0.60 | J2 Results, item 5 |
| Q44 | fresh snowfall in summer | raised snow albedo to about 0.80, then fell to that of the underlying old snow or ice within a few days | J2 Results, item 7 |
| Q44 | thin patchy snow or old snow with high impurity load | below 0.40 | J2 Results, item 9 |
| Q44 | glacier ice | "generally < 0.30"; a fall of about 0.10 over July to August 1993 and a similar rise in 1994 | J2 Discussion and Results, item 8 |
| Q44 | point-to-point repeatability within the 3 h window | < ±0.02 | J2 Methods |
| Q44 | literature range quoted by J2 | "< 0.10 for dirty glacier ice to > 0.90 for fresh snow" (citing Oerlemans 1993, Paterson 1994, Cutler and Munro 1996) | J2 Introduction (secondary) |

Survey means by surface type are in J2's tables, which were images and could not be read. A search-engine summary gave "0.52 for snow, 0.18 for ice, 0.56 for deep snow"; **not seen in the source, not used**.

### Roughness

**Not found** this round (neither optical nor aerodynamic roughness length for these materials).

## Q45. The steps of the erosion methods as published (Improves a plan)

| Q | Paper | Free to read? | Result |
|---|---|---|---|
| Q45 | Musgrave FK, Kolb CE, Mace RS (1989). The synthesis and rendering of eroded fractal terrains. *ACM SIGGRAPH Computer Graphics* 23(3):41-50. doi:10.1145/74334.74337 (Crossref-checked) | ACM Digital Library; not confirmed free this round | **Not read** |
| Q45 | Mei X, Decaudin P, Hu B-G (2007). Fast hydraulic erosion simulation and visualization on GPU. *Pacific Graphics 2007*. HAL inria-00402079 | Free on HAL (open archive) | **Not read**: HAL served a bot-check page ("Making sure you're not a bot!") to my machine; I did not try to get past it |
| Q45 | Cordonnier G, Braun J, Cani M-P, Benes B, Galin É, Peytavie A, Guérin É (2016). Large scale terrain generation from tectonic uplift and fluvial erosion. *Computer Graphics Forum* 35(2):165-175. doi:10.1111/cgf.12820 (Crossref-checked). HAL hal-01262376 | Free on HAL | **Not read**, same bot-check page |

(Musgrave and Cordonnier citations checked on Crossref; the Mei et al. 2007 details come from the HAL record identifier and were **not checked** against the paper.)

## Q46. Water colour by depth and Jerlov water type (Improves a plan)

**Not found** from an open primary source. The tables wanted (Kd(λ) for Jerlov I, IA, IB, II, III and coastal 1C to 9C) are in Solonenko MG, Mobley CD (2015), "Inherent optical properties of Jerlov water types", *Applied Optics* 54(17):5392 ff., doi:10.1364/AO.54.005392 (Crossref-checked), which is paywalled at Optica. A search result pointed to an unofficial third-party copy, which I did not use.

## Q47. A waterfall's path (Improves a plan)

**Not found** (brink-depth/critical-depth ratio and measured jet trajectories were not reached from an open primary source).

## Q48. Where eddies and standing waves form (Improves a plan)

From J3 (flume experiments with bed materials of median fall diameter 0.19 to 0.93 mm, plus field observations including the Rio Grande):

| Q | Quantity | Value | Source |
|---|---|---|---|
| Q48 | bed forms of the lower flow regime | ripples, ripples on dunes, dunes; "flow is tranquil, water-surface undulations are out of phase with bed undulations, and resistance to flow is large" | J3 glossary |
| Q48 | bed forms of the upper flow regime | "plane bed with sediment moving, standing waves, antidunes, or chutes and pools"; water surface in phase with the bed, except in breaking antidune or chute-and-pool flow | J3 glossary |
| Q48 | Froude-number rule | antidunes: water and bed in phase, "a positive indication that the flow is rapid (F > 1)"; dunes: out of phase, "tranquil (F < 1)"; F = mean velocity / gravity-wave celerity | J3 p. J9 |
| Q48 | resistance by regime | lower regime: 7.0 < C/√g < 13.2, or 0.04 < f < 0.16; upper regime (except chutes and pools): 0.02 < f < 0.07, or 10.7 < C/√g < 20 (C = Chézy coefficient, f = Darcy-Weisbach friction factor) | J3 Abstract/summary |
| Q48 | antidune wavelength | L = 2πV²/g (Kennedy 1961, as quoted by J3); two-dimensional antidune waves break at a height/length ratio of about 0.14 | J3 p. J9 |
| Q48 | ripple size | 0.4 to 2 ft long, 0.02 to 0.2 ft high (0.12 to 0.61 m; 6 to 61 mm, **computed**); ripples do not form in bed material coarser than 0.6 mm median fall diameter | J3 "Ripples" and Summary |
| Q48 | dune size | 2.0 ft to more than 100 ft long, 0.2 ft to more than 10 ft high (0.61 to more than 30 m; 0.06 to more than 3 m, **computed**) | J3 Summary |
| Q48 | eddy size against channel width and obstruction size | **Not found** | |

## Q49. Lakes' shores (Improves a plan)

**Not found.** The US Army Corps of Engineers *Coastal Engineering Manual* EM 1110-2-1100 Part II (fetch-limited wave growth, public domain) answered HTTP 403 from publications.usace.army.mil; no other primary source for shore slopes by material was read.

## Not found

- Q44: granite, sandstone, slate (a phyllite is given instead), loam, peat and desert sand in splib07a; field albedo with spread by material; glacier-ice visible reflectance; any roughness values.
- Q45: all three papers' step lists (bot check on HAL; ACM not checked).
- Q46: Jerlov Kd tables (paywalled), and the colour of clear, coastal and lake water at stated depths.
- Q47: all.
- Q48: eddy size relations.
- Q49: all.

## Sources disagree

- J2's text says snow albedo was about 0.80 in late May and 0.40 to 0.60 in summer, while J2 quotes earlier literature reaching "> 0.90 for fresh snow". These measure different snow ages; both stand.
- The splib07a melting-snow sample mSnw01a (0.83 visible reflectance factor, lab geometry) is brighter than J2's summer field albedos (0.40 to 0.60, broadband, includes the near-infrared where snow is darker). Different quantities: do not mix them in a material table without saying which one.
