# E4 verification: vegetation by biome

Verifier: Library Verify (adversarial re-check). Written 2026-11-16, Australia/Sydney (AEDT, UTC+11).
**Do not treat this file as a source of numbers for the engine.** It is a check of `E4-vegetation-by-biome.md`. E4 itself was not edited.

## Version checked

- Path: `/workspace/specula-research/E4-vegetation-by-biome.md`
- Size: 38,089 bytes, 427 lines
- File mtime: **2026-11-16 10:22:51 AEDT (Australia/Sydney, UTC+11)**
- Last re-read before this file was written: **2026-11-16 ~11:00 AEDT**. The research file was unchanged.
- Access date for live URLs: **2026-11-16**.

### Grok live-search pass

A Grok CLI research pass was run for this file (and for E2), after the CLI was signed in, per the grok-cli-research-pass skill:

`~/.grok/bin/grok --no-auto-update --reasoning-effort xhigh -m grok-4.6 --always-approve`

Live web search was left on. The process was started about 10:47 AEDT and was waited out until it exited (output in `/tmp/grok-e4e2/grok-out.txt`). The pass was used especially for the abstract-only and second-hand rows (Pretzsch & Biber, Körner & Paulsen, Yoda, Crowther 2017) and as a cross-check on a sample of the rest. **A Grok answer was treated as a lead only.** Every number below comes from a URL opened independently (curl, WebFetch, Crossref, pdftotext, or a downloaded PDF). Numbers were never taken from Grok alone.

**Conflicts or leads the Grok pass raised, and what was done with them:**

1. **FOFEM 5 vs later official USDA table, index 36** — Grok reported `Fofem_Sbt[36] = 0.065` in the Bevins code but **0.063** in the official FOFEM 6.x user-guide table. Independently confirmed from the FOFEM 6.7 User Guide (Feb 2020) and from the FFE / RMRS-GTR-116 addendum (Douglas-fir multiplier 0.063). This is an E2 finding (see that verify file). It does not change any E4 number.
2. **Crowther 2017 is not a Nature paper.** Grok distinguished the 2017 bioRxiv preprint from Crowther et al. 2015 *Nature* (“Mapping tree density…”, subscription, not CC) and from Bastin / Crowther 2019 *Science*. Independently confirmed. **E4 already labels CROW17 correctly** as a CC BY-ND 4.0 preprint and does not claim a Nature version of that paper. No wording FIX. The brief’s “published Nature version” does not exist for this preprint.
3. **Yoda 1963 original PDF was not found** by Grok or by this check. Secondaries (opened: Encyclopedia.com *Dictionary of Plant Sciences*; ZHA15; SEW18) support mean plant mass / dry weight ∝ *N*^(−3/2). ZHA15 adds a parenthetical “(or volume)”; the encyclopedia entry does not mention volume. See Sources disagree. The original remains unopened.
4. **Pretzsch & Biber 2005** — Grok found a Wayback PDF (`…/576.pdf`) whose download timed out here too (TLS error). Docslib first-page / Crossref abstract numbers match E4’s [ABSTRACT] slopes. Full text was not independently opened. Status stays as E4 labelled it.
5. **Körner & Paulsen 2004** — Grok did not obtain a downloadable PDF (SciSpace 403; Basel edoc has no PDF; Arizona/Wayback pointer not fetched here). Abstract numbers match. A later Körner revision (≈6.4 °C) is a different paper and is not a conflict with the 2004 abstract as cited. A SciSpace extract of Table 7 (California 8.7 °C vs the abstract’s “7–8 °C”) was **not used**, because that PDF was not opened here.
6. Jobbágy & Jackson 2000 Appendix 1: Grok did not open it. This check had already parsed the appendix independently; the derived latitude bands stand.

## Summary

Unit of counting: **one data row** (a table row, or a clearly bounded multi-value note such as the BUR13 large-tree / age / min-DBH notes, the OST14 tree-fern sentence, or a single treeline definition). The licence table in §2 and the “Sources disagree” table in E4 §10 are not in the row counts; they are discussed in the source table and in “Sources disagree” below. Multi-number rows were checked number-by-number; a wrong figure in a region row makes that whole row FIX.

| Status | Rows | What it means |
|---|---:|---|
| **VERIFIED** | **144** | The cited source opens (or an open copy does), and it states that number, units, and population, at the level E4 claims (including [ABSTRACT] / [SECONDARY] / [DERIVED] / [PREPRINT] flags). |
| **FIX** | **4** | A stated number, column, URL, or wording is wrong or misleading. Correction below. |
| **UNVERIFIABLE** | **25** | The EIVE SM08 spreadsheet could not be downloaded (Zenodo file 403). Values were not independently recomputed. |
| **Total data rows** | **173** | Table rows in §§3–8 plus the bounded notes listed above. |

**Most serious problems**

1. **FIX — BUR13 Table 2, Europe and North America columns are swapped.** Burrascano et al. 2013, Table 2 p.472, column order is Global, Asia, Australia, Europe, North America, South America. E4 put the North American medians and ranges in the Europe row, and the European values in the North America row, across basal area (mature and old-growth), stems/ha (both), and QMD OG. Global, Asia, Australia and South America are right, as are the large-tree (16.3 vs 72.6 /ha, DBH > 50 cm), stand-age (125 vs 250 yr) and min-DBH test (p = 0.198) notes. **This is the error that would actually change a biome generator.** Corrected values are in the row table.
2. **FIX — GLICK16 “Table 4” URL is the wrong Nature table index.** The numbers E4 quotes (e.g. Boreal 8,688 plots, 749.3 ± 50.1 billion; Global 429,775 plots, 3,041.2 ± 96.1 billion) match *Scientific Data* Table 4, but `https://www.nature.com/articles/sdata201669/tables/4` is Table 3 (validation). The real Table 4 is `https://www.nature.com/articles/sdata201669/tables/5`. Licence CC BY 4.0, as stated. Numbers stay VERIFIED; the URL needs changing.
3. **FIX — JJ00 derived band, *Larix sibirica* 200–770 m (69.5–67.8°N).** Appendix 1 independently parsed (113 extratropical sites). All eight 10° bands match the claimed n / min / max / mean (means rounded). The 770 m figure is Khakoma at **67.9°N**, not 67.8°N. 67.8°N is Beldunchana at 765 m. Bulun 70.6°N / 195 m *Larix gmelinii*, Hermite 450 m and Bio-Bío 1900 m are right. The bands are correctly labelled [DERIVED] and extratropical / above 30°.
4. **FIX (wording, minor) — PK14 growing-season threshold written “> 0.9 °C”.** The GMBA PDF’s text layer has “[0.9 °C” (standard ≥ OCR) and later “0.9 °C for DTMIN”. The parameter is a 0.9 °C daily-mean threshold; writing “≥ 0.9 °C” matches the typography. Season ≥ 94 days, season mean ≥ 6.4 °C, 376 sites, predicted = 1.005 × measured − 1 m, r² = 0.997, RMSE 78 m, lapse −0.0055 K/m, and the area / NZ figures all match.

**What is in good shape.** Whittaker 775-vertex mins and maxes (1 d.p.) recompute from the `plotbiomes` RDA (MIT licence; Stefan & Levin; digitised from Ricklefs 2008 Fig. 5.5; Whittaker 1975 primary not opened, as E4 says). PAN13 Table 1 climate / canopy / biomass cells checked. CROW17 Table 2 per-hectare means match the bioRxiv PDF; licence is CC-BY-ND 4.0 and the preprint was not certified by peer review. OST14 Table 2 selected plots, mean 4,733 ± 722 SE (n = 14), and the tree-fern mid-canopy “approximately 5 m above the ground” match; CC BY. LEW13 425.6 ± 11.1 stems, BA 30.3 ± 0.77, AGB 395.7 ± 14.3, WMD 0.648, 260 plots (mean 1.2 ha), 12 African countries, and “cf. approx. 600” match; CC BY 3.0. Reineke 1933 (public domain): log N = −1.605 log D + k, k = 4.605 through 1,000 at 10 in, white fir 830, Douglas-fir “approximately 595”, red fir Fig. 2 fits the reference curve; ×2.471 trees/ha conversion is right. FVS NE Table 3.5.1 SDI maxima (BF 655, TA 387, … RO 414), Shaw unpublished FIA note, Zeide method and default 0.85 SDIU hold (public domain). SK19 Table 1 and layer definitions match (CC BY 4.0). ZHA15 posterior −1.47 and prior N(−1.52, 0.39) match (CC BY). NV06 all 24 species in Appendix A match; ESA archives CC BY 4.0 (paper © ESA). FEIS Douglas-fir elevations and −10 °C frost match. MON20 layer cuts match (CC BY 4.0). TES23 and JAC21 abstract numbers and JAC21 CC BY 4.0 match. JJ00 Table 1 regressions and Table 2 life-form temperatures match; the introduction’s latitude pattern is correctly [SECONDARY].

**§9 Not found and §10 Sources disagree** are present and, where checked, honest. rdrr.io failed here too; the raw GitHub DESCRIPTION and data.R for `plotbiomes` succeeded (MIT, 775 rows). Several publisher DOIs (Wiley, OUP, Annual Reviews) return 403 to curl; open copies or abstracts were used as E4 already flags.

## Row table

Columns: row id, claim (short), status, note/correction, URL checked. Access date 2026-11-16 throughout.

### §3 Whittaker polygons (WHIT)

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| W1–W9 | Nine biome boxes: T and P minima/maxima at 1 d.p., vertex counts (e.g. boreal −6.7 to 4.4 °C, 18.7–201.3 cm, 96 vertices; subtropical desert 14.1–30.0 °C, 0.0–99.9 cm, 40 vertices) | VERIFIED | Recomputed from the `plotbiomes` Whittaker_biomes RDA (775 rows). Package MIT. Ricklefs 2008 Fig. 5.5 is the stated digitisation source. Whittaker 1975 not opened, as labelled. | https://github.com/valentinitnelav/plotbiomes (raw DESCRIPTION / data.R); package authors Stefan & Levin |

### §3b PAN13 Table 1

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| P1–P16 | FAO-zone climate, canopy height and biomass cells on pp.598–599 (e.g. tropical rainforest ∼20–25 °C, >1500 mm, canopy 25–50 m, 145 ± 53 Mg C ha−1) | VERIFIED | All checked cells match. Minor omission: some dry-month notes in the source include “in winter”, which E4 shortens. © Annual Reviews; facts cited. Treesearch reprint used. | https://research.fs.usda.gov/download/treesearch/45295.pdf |

### §4a CROW17 / GLICK16 densities

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| D1–D14 | CROW17 Table 2 per-hectare means by biome (trees ≥ 10 cm), and GLICK16 Table 4 totals and plot counts (Boreal 8,688 plots, 749.3 ± 50.1 billion; Global 429,775 / 3,041.2 ± 96.1, and the other biome rows) | VERIFIED | Numbers match the bioRxiv PDF (Table 2) and Nature *Sci. Data* Table 4. CROW17 is a preprint, CC-BY-ND 4.0, “not certified by peer review.” There is **no Nature version of this 2017 paper**. Crowther et al. 2015 *Nature* doi 10.1038/nature14967 is a related earlier map (≈3.04 trillion trees); Crossref/page: Springer Nature subscription, not CC. Bastin et al. 2019 *Science* is a later related paper, not a republication. | https://www.biorxiv.org/content/10.1101/210062v2.full.pdf ; https://www.nature.com/articles/sdata201669/tables/5 |
| D-url | GLICK16 Table 4 cited at `…/tables/4` | **FIX** | That URL is Table 3 (validation). Use `…/tables/5`. CC BY 4.0. | https://www.nature.com/articles/sdata201669/tables/4 (wrong); https://www.nature.com/articles/sdata201669/tables/5 (correct) |

### §4b–4c Tropical plots (OST14, LEW13)

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| O1–O14 | OST14 Table 2 selected ForestGEO plots; mean 4,733 ± 722 SE (n = 14) | VERIFIED | CC BY. | https://pmc.ncbi.nlm.nih.gov/articles/PMC4146472/ |
| O-tf | Hawaiian tree-fern mid-canopy “approximately 5 m above the ground” | VERIFIED | In the full text, not only the abstract. | same |
| L1–L3 | LEW13: 425.6 ± 11.1 stems ha−1, BA 30.3 ± 0.77, AGB 395.7 ± 14.3, WMD 0.648; 260 plots, mean 1.2 ha, 12 African countries; “cf. approx. 600” | VERIFIED | CC BY 3.0. | https://pmc.ncbi.nlm.nih.gov/articles/PMC3720018/ |

### §4d BUR13 Table 2

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| B-glob | Global BA M 41.7 [15.6–144.5], OG 47.3 [24.4–379]; stems M 536 [217.5–6,015], OG 501 [96.7–9,680]; QMD OG 34.6 [13.4–171.6] | VERIFIED | Table 2 p.472. | https://www.uvm.edu/giee/pubpdfs/Burrascano_2013_Forest_Ecology_and_Management.pdf |
| B-EU | Europe BA M 29.1 [24–59], OG 44.4 [28.7–379]; stems M 488.5 [217.5–1,000], OG 392 [96.7–1,720]; QMD OG 32.8 [15.7–171.6] | **FIX** | These are the **North America** cells. Correct Europe: BA M **42.2 [30.7–69.2]**, OG **38.5 [24.4–57.4]**; stems M **444 [259–2,182]**, OG **341 [124–1,835]**; QMD OG **39.3 [14.6–64.3]**. | same |
| B-NA | North America BA M 42.2 [30.7–69.2], OG 38.5 [24.4–57.4]; stems M 444 [259–2,182], OG 341 [124–1,835]; QMD OG 39.3 [14.6–64.3] | **FIX** | These are the **Europe** cells. Correct NA: BA M **29.1 [24–59]**, OG **44.4 [28.7–379]**; stems M **488.5 [217.5–1,000]**, OG **392 [96.7–1,720]**; QMD OG **32.8 [15.7–171.6]**. | same |
| B-SA, B-As, B-Au | South America, Asia, Australia cells as tabulated | VERIFIED | Australia QMD OG 40 (no range in the table) as written. | same |
| B-notes | LLT (DBH > 50 cm) 16.3 vs 72.6 /ha; ages 125 vs 250 yr; min-DBH test p = 0.198 | VERIFIED | | same |

### §4e, §5 Reineke / Yoda / Pretzsch / Zhang, §5b FVS SDI, §5c SK19

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| R1–R5 | Reineke equation, k = 4.605, white fir 830, Douglas-fir ≈595, red fir fits 1,000-at-10-in reference, ×2.471 | VERIFIED | Public domain USDA PDF. | https://www.fs.usda.gov/psw/publications/cfres/cfres_1933_reineke001.pdf ; also treesearch/60134.pdf |
| Y1 | Yoda −3/2: “mean plant mass or volume ∝ N^(−3/2)”, original not opened, [SECONDARY] | VERIFIED as secondary, with a wording caution | Original 1963 *J. Biol. Osaka City Univ.* 14:107–129 was not found (CiNii bibliographic only). ZHA15 says “mean plant biomass (or volume)”. Encyclopedia.com (opened) states *W* = C ρ^(−3/2) with *W* the **dry weight** of surviving plants; no volume. SEW18 cites the −3/2 law but also muddles it with Reineke (1933), as E4 already implies by using it only as a secondary. Keep the [SECONDARY] flag; prefer “mean plant mass (dry weight)” unless the 1963 PDF is opened. | https://www.encyclopedia.com/science/dictionaries-thesauruses-pictures-and-press-releases/yodas-power-law ; PMC6262744; PMC4594911 |
| Z1 | ZHA15 posterior −1.47, prior N(−1.52, 0.39); fixed slopes −1.605 / −1.5 | VERIFIED | CC BY. | https://pmc.ncbi.nlm.nih.gov/articles/PMC4594911/ |
| PB1 | Pretzsch & Biber slopes −1.789, −1.664, −1.593, −1.424; 28 stands since 1870; significance statements | VERIFIED at the [ABSTRACT] level E4 already claims | Crossref / TUM abstract match. Full text not independently opened (Wayback 576.pdf TLS timeout). Licence: © 2005 Society of American Foresters; not OA (Unpaywall closed). | https://doi.org/10.1093/forestscience/51.4.304 ; https://api.crossref.org/works/10.1093/forestscience/51.4.304 |
| SDI1–SDI16 | FVS NE Table 3.5.1 maxima (BF 655, TA 387, … RO 414) and method notes | VERIFIED | Public domain. Table 3.5.1 is SDI, not bark. | https://www.fs.usda.gov/sites/default/files/forest-management/fvs-ne-overview.pdf |
| SK1–SK5 | SK19 Table 1 cells; layers T1 >10 m, T2 8.1–10, S1 3.1–8, S2 0.6–3, herb ≤0.5; 200 m² plots; Kruskal–Wallis p&lt;0.001 | VERIFIED | CC BY 4.0. | https://pmc.ncbi.nlm.nih.gov/articles/PMC6622550/ |

### §6 layers, §7a NV06, §7b EIVE, §7c FEIS

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| M1 | MON20 layers: tree &gt;7 m, shrub 20 cm–7 m, herb &lt;20 cm | VERIFIED | CC BY 4.0. | (as cited in E4) |
| N1–N24 | NV06 Appendix A, all 24 species shade / drought / waterlogging | VERIFIED | ESA archives CC BY 4.0; paper © ESA. | ESA data archives / the cited NV06 appendix |
| E1–E25 | EIVE 1.0 niche position and width values (SM08 xlsx, Zenodo 7534792) | **UNVERIFIABLE** | Zenodo API metadata confirms **cc-by-4.0** and that the file exists. File downloads returned **HTTP 403** this pass, so the tabulated values were not independently recomputed. Paper doi 10.3897/vcs.98324 is CC BY 4.0. | https://doi.org/10.5281/zenodo.7534792 |
| F1–F2 | FEIS Douglas-fir elevations (WA/OR; S Oregon Cascades and N Sierra 609–1,829 m; S Sierra 2,286 m) and −10 °C frost | VERIFIED | | https://www.fs.usda.gov/database/feis/plants/tree/psemenm/all.html |

### §8 Treeline (KP04, PK14, JJ00, TES23, JAC21)

| Id | Claim | Status | Note | URL |
|---|---|---|---|---|
| KP1 | 6.7 °C ± 0.8 SD, 46 sites, 68°N–42°S; regional 7–8 / 5–6 / 6–7 °C | VERIFIED at the [ABSTRACT] level E4 already claims | Wiley 403 to curl. Crossref and Basel edoc abstracts match. Full text not opened (no public PDF found). Closed Wiley VOR. | https://doi.org/10.1111/j.1365-2699.2003.01043.x ; https://edoc.unibas.ch/entities/publication/242ab9b5-108b-409d-8c0b-3d950a7043d1 ; Crossref API |
| PK1 | TREELIM: 94 days, mean 6.4 °C, 376 sites, 1.005× − 1 m, r²=0.997, RMSE 78 m, lapse −0.0055 K/m, arid &lt;200 mm, area figures, NZ 18 sites 854–1535 m, mean error 11 m | VERIFIED | GMBA PDF opened with pdftotext. | https://www.gmba.unibe.ch/unibe/portal/microsites/micro_gmba/content/e1594357/e1599410/e1599467/pane1599468/e1599482/KrnerandPaulsen._AlpineBotany_2014_eng.pdf |
| PK-th | “days with daily mean &gt; 0.9 °C” | **FIX** (minor wording) | PDF text layer: “[0.9 °C” and “0.9 °C for DTMIN”. Write **≥ 0.9 °C**. | same |
| J1 | JJ00 Table 1 regressions and Table 2 life-form temperatures | VERIFIED | | (JJ00 PDF as cited) |
| J2 | Introduction latitude pattern, labelled [SECONDARY] | VERIFIED as secondary | Matches the introduction’s citation of Körner 1998 / Leuschner 1996. | same |
| J3–J10 | Eight 10° forest-line bands [DERIVED] from Appendix 1 (extratropical, above 30°) | VERIFIED | Independently parsed 113 sites; n, min, max and rounded means all match. | same |
| J-lar | “*Larix sibirica* 200–770 m (69.5–67.8°N)” | **FIX** | 770 m is Khakoma at **67.9°N**. 67.8°N is Beldunchana, 765 m. | same |
| T1, Ja1 | TES23 and JAC21 abstract numbers; JAC21 CC BY 4.0 | VERIFIED | | (as cited) |

## Source table

| Source | Opened? | Licence as checked | Notes |
|---|---|---|---|
| plotbiomes / Whittaker RDA | Yes (GitHub raw; rdrr.io failed) | MIT | 775 vertices; Ricklefs Fig. 5.5. Whittaker 1975 not opened. |
| PAN13 | Yes, Treesearch reprint | © 2013 Annual Reviews. All rights reserved | Facts cited. |
| CROW17 bioRxiv 210062 | Yes, PDF and HTML | **CC-BY-ND 4.0**; preprint, not peer reviewed | No Nature version of this paper. |
| Crowther et al. 2015 *Nature* 10.1038/nature14967 | HTML / Crossref (body paywalled) | Springer Nature subscription; © 2015 Springer Nature Limited; not CC | Related earlier tree-density map, not a “Nature version” of CROW17. |
| Bastin et al. 2019 *Science* | Abstract / licence lines | Science Journals Default License; © 2019 AAAS | Later related work; different numbers. |
| GLICK16 *Sci. Data* | Yes, table pages | **CC BY 4.0** | Table URL index is off by one (see FIX). |
| BUR13 author PDF (UVM) | Yes | © Elsevier; author copy | Table 2 column order caused the Europe/NA swap. |
| LEW13 PMC3720018 | Yes | **CC BY 3.0** | |
| OST14 PMC4146472 | Yes | **CC BY** | |
| REI33 USDA PDF | Yes | US government work, public domain | |
| FVS NE overview | Yes | US government work, public domain | |
| SK19 PMC6622550 | Yes | **CC BY 4.0** | |
| ZHA15 PMC4594911 | Yes | **CC BY** | Source of the “or volume” parenthesis on Yoda. |
| SEW18 PMC6262744 | Yes | **CC BY** | Muddles Reineke with the −3/2 law. |
| Pretzsch & Biber 2005 | Abstract / Crossref only | © 2005 SAF; not OA | Full text not opened. |
| KP04 | Abstract only | Closed Wiley VOR | Full text not opened. |
| PK14 GMBA PDF | Yes | © (as on the PDF); facts cited | |
| JJ00 | Yes, including Appendix 1 | © 2000 ESA; facts cited | |
| NV06 Appendix A | Yes | ESA archives **CC BY 4.0** (paper © ESA) | |
| EIVE / Zenodo 7534792 | Metadata only (file 403) | Paper and data **CC BY 4.0** (API) | Values not recomputed. |
| FEIS *P. menziesii* | Yes | US government work | |
| MON20, TES23, JAC21 | Yes (JAC21 CC BY 4.0 confirmed) | as E4 | |

## Not found

- **Yoda et al. 1963** original PDF: not found. CiNii records only. Equation confirmed only in secondaries, as mean dry weight / plant mass.
- **Pretzsch & Biber 2005** full text: publisher closed; TUM `576.pdf` now redirects; Wayback snapshot listed but TLS/timeout here.
- **Körner & Paulsen 2004** full text: Wiley 403; Basel edoc has no PDF; SciSpace 403.
- **EIVE SM08 xlsx**: Zenodo file download 403.
- **rdrr.io** plotbiomes page: failed; GitHub raw used instead.
- **Whittaker 1975** primary book: not opened (E4 already says this).
- Silvics Manual (srs.fs.usda.gov): TLS error, as E4 reports.

## Sources disagree

- **Yoda’s quantity:** ZHA15 writes “mean plant biomass (or volume)”. Encyclopedia.com and the usual statement of the law use dry weight / mean plant mass only. Original unopened. E4 already flags [SECONDARY].
- **SEW18** attributes the −3/2 law jointly to Reineke 1933 and Yoda 1963. Reineke’s published slope is −1.605 on log N vs log D, not −3/2. E4 uses SEW18 only as a secondary pointer to Yoda.
- **CROW17 vs Crowther 2015 vs Bastin 2019:** three different papers. E4 cites the first two roles correctly (preprint densities; Glick 2016 totals). Do not treat 2015 *Nature* as a CC substitute for the ND preprint.
- **KP04 6.7 ± 0.8 vs later Körner ≈6.4 °C:** a later revision, not a conflict inside the 2004 paper as cited.
- BUR13 Europe/NA swap is a **documentation error in E4**, not a disagreement between sources.

## Licence findings (short)

- CROW17: CC-BY-ND 4.0 preprint, no derivatives — E4 states this correctly. There is no CC Nature version of this paper. Crowther 2015 *Nature* is subscription / © Springer Nature.
- GLICK16, LEW13, OST14, SK19, ZHA15, SEW18, JAC21, MON20, EIVE (paper+data metadata), NV06 archive, plotbiomes: open licences as E4 says (EIVE file not downloaded).
- REI33, FVS NE, FEIS, PAN13 Treesearch reprint: US government or stated ©; public-domain government works as labelled.
- PB05, KP04, BUR13, PAN13 journal, JJ00, PK14: closed or ©; facts only, as labelled.
